use std::sync::Arc;

use tokio::net::TcpListener;

use crate::http_util::{
    self, encode_response, extract_auth, json_response, parse_json, read_body,
    split_path_query, AuthScheme, CorsPolicy, HttpLimits,
};
use crate::live_server::LiveResponse;

use super::{
    adapter::{self, A2aAdapterState},
    types::{A2aError, MessageSendRequest, TaskListResponse},
};

// ── A2A server state ────────────────────────────────────────────────────────

/// State shared across all A2A server connections.
#[derive(Clone)]
pub struct A2aServerState {
    pub adapter: A2aAdapterState,
    /// Per-launch auth token for A2A requests.
    pub auth_token: Arc<String>,
}

// ── Public entry point ──────────────────────────────────────────────────────

/// Start the A2A server on the given host/port. Runs until the process exits.
pub async fn spawn_a2a_server(state: A2aServerState, host: String, port: u16) {
    let listener = match TcpListener::bind((host.as_str(), port)).await {
        Ok(listener) => listener,
        Err(error) => {
            log::error!("a2a: failed to bind on {}:{}: {}", host, port, error);
            return;
        }
    };

    log::info!("a2a: server listening on http://{}:{}", host, port);

    loop {
        let (mut stream, _addr) = match listener.accept().await {
            Ok(parts) => parts,
            Err(error) => {
                log::warn!("a2a: accept error: {}", error);
                continue;
            }
        };

        let state = state.clone();
        tokio::spawn(async move {
            let request = match http_util::read_http_request(&mut stream, HttpLimits::DEFAULT).await
            {
                Ok(r) => r,
                Err(resp) => {
                    let bytes = encode_response(resp, Some(CorsPolicy::A2A));
                    http_util::write_and_close(&mut stream, &bytes).await;
                    return;
                }
            };

            let first_line = request.lines().next().unwrap_or_default();
            let mut parts_iter = first_line.split_whitespace();
            let method = parts_iter.next().unwrap_or("GET");
            let target = parts_iter.next().unwrap_or("/");
            let (path, _query) = split_path_query(target);

            let response = handle_a2a_request(&state, method, &path, &request).await;

            let bytes = encode_response(response, Some(CorsPolicy::A2A));
            http_util::write_and_close(&mut stream, &bytes).await;
        });
    }
}

// ── Request handler / router ────────────────────────────────────────────────

async fn handle_a2a_request(
    state: &A2aServerState,
    method: &str,
    path: &str,
    request: &str,
) -> LiveResponse {
    // ── CORS preflight ──────────────────────────────────────────────────
    if method == "OPTIONS" {
        return LiveResponse::text("204 No Content", String::new());
    }

    // ── Auth guard ──────────────────────────────────────────────────────
    let expected = state.auth_token.as_str();
    match extract_auth(request, AuthScheme::Bearer) {
        Some(tok) if tok == expected => {}
        _ => {
            return LiveResponse::text(
                "401 Unauthorized",
                "missing or invalid auth token".to_string(),
            );
        }
    }

    // ── Route ───────────────────────────────────────────────────────────
    match (method, path) {
        // Discovery
        ("GET", "/.well-known/agent.json") => {
            let pm = state.adapter.plugin_manager.lock().await;
            let settings = state.adapter.settings.lock().await;
            let base_url = a2a_base_url(&settings);
            let card = adapter::build_agent_card(&base_url, &pm);
            json_response(&card)
        }

        // Send message (synchronous)
        ("POST", "/message:send") => {
            let body = read_body(request);
            match parse_json::<MessageSendRequest>(&body, false) {
                Ok(req) => match adapter::handle_message_send(&state.adapter, req).await {
                    Ok(task) => json_response(&task),
                    Err(err) => error_response("500 Internal Server Error", &err),
                },
                Err(resp) => resp,
            }
        }

        // Streaming — explicitly unsupported
        ("POST", "/message:stream") => error_response(
            "501 Not Implemented",
            &A2aError::unsupported_operation("Streaming is not supported in this version"),
        ),

        // Task list
        ("GET", "/tasks") => {
            let tasks = adapter::handle_task_list(&state.adapter).await;
            json_response(&TaskListResponse { tasks })
        }

        // Task-specific routes: /tasks/{id} and /tasks/{id}:cancel
        _ if path.starts_with("/tasks/") => handle_task_route(state, method, path).await,

        // 404
        _ => LiveResponse::text("404 Not Found", "Not Found".to_string()),
    }
}

/// Handle routes under `/tasks/{id}` and `/tasks/{id}:cancel` and
/// `/tasks/{id}:subscribe`.
async fn handle_task_route(state: &A2aServerState, method: &str, path: &str) -> LiveResponse {
    // Strip the "/tasks/" prefix to get "{id}" or "{id}:cancel" or "{id}:subscribe".
    let remainder = &path["/tasks/".len()..];

    if remainder.is_empty() {
        return LiveResponse::text("400 Bad Request", "missing task id".to_string());
    }

    // Check for :cancel suffix
    if let Some(task_id) = remainder.strip_suffix(":cancel") {
        if method != "POST" {
            return LiveResponse::text(
                "405 Method Not Allowed",
                "Use POST for cancel".to_string(),
            );
        }
        return match adapter::handle_task_cancel(&state.adapter, task_id).await {
            Ok(task) => json_response(&task),
            Err(err) => error_response("404 Not Found", &err),
        };
    }

    // Check for :subscribe suffix (unsupported)
    if remainder.ends_with(":subscribe") {
        return error_response(
            "501 Not Implemented",
            &A2aError::unsupported_operation(
                "Task subscription (SSE) is not supported in this version",
            ),
        );
    }

    // Plain task lookup: GET /tasks/{id}
    let task_id = remainder;
    if method != "GET" {
        return LiveResponse::text(
            "405 Method Not Allowed",
            "Use GET for task retrieval".to_string(),
        );
    }

    match adapter::handle_task_get(&state.adapter, task_id).await {
        Ok(task) => json_response(&task),
        Err(err) => error_response("404 Not Found", &err),
    }
}

// ── A2A-specific helpers (not shared) ───────────────────────────────────────

/// Build a JSON error response from an [`A2aError`]. Kept here because it
/// carries A2A-specific wire format (error code, JSON shape) that the other
/// servers don't use.
fn error_response(status: &'static str, err: &A2aError) -> LiveResponse {
    match serde_json::to_string(err) {
        Ok(json) => LiveResponse {
            status,
            content_type: "application/json; charset=utf-8",
            body: json,
        },
        Err(e) => LiveResponse::text(status, format!("error serialization failed: {e}")),
    }
}

/// Build the A2A base URL from current settings.
fn a2a_base_url(settings: &crate::AppSettings) -> String {
    let host = if settings.a2a_bind_lan {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };
    format!("http://{}:{}", host, settings.a2a_port)
}

// ── Tests ───────────────────────────────────────────────────────────────────
//
// Helpers that used to live in this file (read_http_request, normalize_path,
// split_path_query, read_body, parse_json, json_response, encode_response,
// extract_bearer_token) are now exhaustively tested in `crate::http_util`.
// We only keep integration-style tests here that exercise A2A routing and
// auth at the request-handler level.

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use super::super::tasks::TaskRegistry;
    use crate::{
        ai::{client::AiClient, router::ConversationContext},
        create_plugin_manager_builtin_only, AppSettings, SkillManager,
    };
    use tokio::sync::Mutex;

    fn test_server_state() -> A2aServerState {
        let mut settings = AppSettings::default();
        settings.a2a_port = 18123;

        A2aServerState {
            adapter: A2aAdapterState {
                plugin_manager: Arc::new(Mutex::new(create_plugin_manager_builtin_only())),
                ai_client: Arc::new(Mutex::new(AiClient::new(
                    String::new(),
                    String::new(),
                    String::new(),
                ))),
                settings: Arc::new(Mutex::new(settings)),
                conversation: Arc::new(Mutex::new(ConversationContext::new(10))),
                skill_manager: Arc::new(Mutex::new(SkillManager::new())),
                task_registry: Arc::new(Mutex::new(TaskRegistry::new(10))),
            },
            auth_token: Arc::new("test-token".to_string()),
        }
    }

    #[tokio::test]
    async fn agent_card_route_requires_bearer_token_and_returns_card() {
        let state = test_server_state();

        let unauthorized = handle_a2a_request(
            &state,
            "GET",
            "/.well-known/agent.json",
            "GET /.well-known/agent.json HTTP/1.1\r\n\r\n",
        )
        .await;
        assert_eq!(unauthorized.status, "401 Unauthorized");

        let authorized = handle_a2a_request(
            &state,
            "GET",
            "/.well-known/agent.json",
            "GET /.well-known/agent.json HTTP/1.1\r\nAuthorization: Bearer test-token\r\n\r\n",
        )
        .await;

        assert_eq!(authorized.status, "200 OK");
        let card: super::super::types::AgentCard = serde_json::from_str(&authorized.body).unwrap();
        assert_eq!(card.name, "OmniLauncher");
        assert_eq!(card.url, "http://127.0.0.1:18123");
        assert!(card
            .authentication
            .schemes
            .iter()
            .any(|scheme| scheme == "bearer"));
        assert!(!card.skills.is_empty());
    }

    #[tokio::test]
    async fn options_returns_204_without_auth() {
        let state = test_server_state();
        let resp = handle_a2a_request(&state, "OPTIONS", "/anything", "OPTIONS / HTTP/1.1\r\n\r\n")
            .await;
        assert_eq!(resp.status, "204 No Content");
    }

    #[tokio::test]
    async fn unknown_route_returns_404() {
        let state = test_server_state();
        let resp = handle_a2a_request(
            &state,
            "GET",
            "/does/not/exist",
            "GET /does/not/exist HTTP/1.1\r\nAuthorization: Bearer test-token\r\n\r\n",
        )
        .await;
        assert_eq!(resp.status, "404 Not Found");
    }

    #[test]
    fn error_response_includes_error_code() {
        let err = A2aError::unsupported_operation("streaming not supported");
        let resp = error_response("501 Not Implemented", &err);
        assert_eq!(resp.status, "501 Not Implemented");
        assert!(resp.body.contains("-32004"));
    }
}
