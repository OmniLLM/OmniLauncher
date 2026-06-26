use std::sync::Arc;

use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
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

    log::info!(
        "a2a: server listening on http://{}:{}",
        host,
        port
    );

    loop {
        let (mut stream, addr) = match listener.accept().await {
            Ok(parts) => parts,
            Err(error) => {
                log::warn!("a2a: accept error: {}", error);
                continue;
            }
        };

        let state = state.clone();
        tokio::spawn(async move {
            let request = match read_http_request(&mut stream).await {
                Ok(r) => r,
                Err(resp) => {
                    let bytes = encode_response(resp);
                    let _ = stream.write_all(&bytes).await;
                    let _ = stream.shutdown().await;
                    return;
                }
            };

            let first_line = request.lines().next().unwrap_or_default();
            let mut parts_iter = first_line.split_whitespace();
            let method = parts_iter.next().unwrap_or("GET");
            let target = parts_iter.next().unwrap_or("/");
            let (path, _query) = split_path_query(target);

            let response = handle_a2a_request(&state, method, &path, &request).await;

            let bytes = encode_response(response);
            if let Err(error) = stream.write_all(&bytes).await {
                log::debug!("a2a: write error to {}: {}", addr, error);
            }
            let _ = stream.shutdown().await;
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
    match extract_bearer_token(request) {
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
            match parse_json::<MessageSendRequest>(&body) {
                Ok(req) => match adapter::handle_message_send(&state.adapter, req).await {
                    Ok(task) => json_response(&task),
                    Err(err) => error_response("500 Internal Server Error", &err),
                },
                Err(resp) => resp,
            }
        }

        // Streaming — explicitly unsupported
        ("POST", "/message:stream") => {
            error_response(
                "501 Not Implemented",
                &A2aError::unsupported_operation(
                    "Streaming is not supported in this version",
                ),
            )
        }

        // Task list
        ("GET", "/tasks") => {
            let tasks = adapter::handle_task_list(&state.adapter).await;
            json_response(&TaskListResponse { tasks })
        }

        // Task-specific routes: /tasks/{id} and /tasks/{id}:cancel
        _ if path.starts_with("/tasks/") => {
            handle_task_route(state, method, path).await
        }

        // 404
        _ => LiveResponse::text("404 Not Found", "Not Found".to_string()),
    }
}

/// Handle routes under `/tasks/{id}` and `/tasks/{id}:cancel` and
/// `/tasks/{id}:subscribe`.
async fn handle_task_route(
    state: &A2aServerState,
    method: &str,
    path: &str,
) -> LiveResponse {
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

// ── HTTP helpers ────────────────────────────────────────────────────────────
//
// These are duplicated from `live_server.rs` / `server.rs` — both files keep
// their copies private, which is the established pattern in this codebase.

/// Read a complete HTTP request from `stream`, returning it as a `String`.
async fn read_http_request(stream: &mut tokio::net::TcpStream) -> Result<String, LiveResponse> {
    const HEADER_CAP: usize = 64 * 1024;
    const BODY_CAP: usize = 16 * 1024 * 1024;
    const TIMEOUT_SECS: u64 = 30;

    let result = tokio::time::timeout(std::time::Duration::from_secs(TIMEOUT_SECS), async {
        let mut raw: Vec<u8> = Vec::with_capacity(4096);
        let mut tmp = [0u8; 4096];
        let header_end = loop {
            let n = stream
                .read(&mut tmp)
                .await
                .map_err(|_| LiveResponse::text("400 Bad Request", "read error".to_string()))?;
            if n == 0 {
                return Err(LiveResponse::text(
                    "400 Bad Request",
                    "connection closed".to_string(),
                ));
            }
            raw.extend_from_slice(&tmp[..n]);
            if raw.len() > HEADER_CAP {
                return Err(LiveResponse::text(
                    "431 Request Header Fields Too Large",
                    "header too large".to_string(),
                ));
            }
            if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
        };

        let header_str = String::from_utf8_lossy(&raw[..header_end]);
        let content_length: Option<usize> = header_str
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
            .and_then(|l| l["content-length:".len()..].trim().parse().ok());

        if let Some(cl) = content_length {
            if cl > BODY_CAP {
                return Err(LiveResponse::text(
                    "413 Payload Too Large",
                    "request body too large".to_string(),
                ));
            }
            let already = raw.len() - header_end;
            let remaining = cl.saturating_sub(already);
            if remaining > 0 {
                let old_len = raw.len();
                raw.resize(old_len + remaining, 0);
                stream.read_exact(&mut raw[old_len..]).await.map_err(|_| {
                    LiveResponse::text("400 Bad Request", "body read error".to_string())
                })?;
            }
        }

        Ok(String::from_utf8_lossy(&raw).into_owned())
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(_elapsed) => Err(LiveResponse::text(
            "408 Request Timeout",
            "request timed out".to_string(),
        )),
    }
}

fn split_path_query(target: &str) -> (String, String) {
    match target.split_once('?') {
        Some((p, q)) => (normalize_path(p), q.to_string()),
        None => (normalize_path(target), String::new()),
    }
}

fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        "/".to_string()
    } else {
        format!("/{}", trimmed.trim_matches('/'))
    }
}

fn read_body(request: &str) -> String {
    match request.find("\r\n\r\n") {
        Some(pos) => request[pos + 4..].to_string(),
        None => String::new(),
    }
}

fn parse_json<T: DeserializeOwned>(body: &str) -> Result<T, LiveResponse> {
    serde_json::from_str(body).map_err(|e| {
        LiveResponse::text(
            "400 Bad Request",
            format!("invalid JSON: {e}"),
        )
    })
}

fn json_response<T: Serialize>(value: &T) -> LiveResponse {
    match serde_json::to_string(value) {
        Ok(json) => LiveResponse::json(json),
        Err(e) => LiveResponse::text(
            "500 Internal Server Error",
            format!("serialization error: {e}"),
        ),
    }
}

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

fn encode_response(response: LiveResponse) -> Vec<u8> {
    let header = format!(
        "HTTP/1.1 {}\r\n\
         Content-Type: {}\r\n\
         Cache-Control: no-store, no-cache, must-revalidate\r\n\
         Pragma: no-cache\r\n\
         Expires: 0\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type, Authorization\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    );
    [header.into_bytes(), response.body.into_bytes()].concat()
}

// ── Auth ────────────────────────────────────────────────────────────────────

/// Extract a Bearer token from the HTTP request headers.
fn extract_bearer_token(request: &str) -> Option<&str> {
    for line in request.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("authorization:") {
            let value = line["authorization:".len()..].trim();
            if value.len() >= 7 && value[..7].eq_ignore_ascii_case("bearer ") {
                return Some(value[7..].trim());
            }
        }
        // Stop at the header/body boundary.
        if line.is_empty() {
            break;
        }
    }
    None
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

    #[test]
    fn extract_bearer_token_standard() {
        let req = "GET / HTTP/1.1\r\nAuthorization: Bearer abc123\r\n\r\n";
        assert_eq!(extract_bearer_token(req), Some("abc123"));
    }

    #[test]
    fn extract_bearer_token_case_insensitive() {
        let req = "GET / HTTP/1.1\r\nauthorization: bearer MyToken\r\n\r\n";
        assert_eq!(extract_bearer_token(req), Some("MyToken"));
    }

    #[test]
    fn extract_bearer_token_missing() {
        let req = "GET / HTTP/1.1\r\nContent-Type: application/json\r\n\r\n";
        assert_eq!(extract_bearer_token(req), None);
    }

    #[test]
    fn extract_bearer_token_no_bearer_prefix() {
        let req = "GET / HTTP/1.1\r\nAuthorization: Basic abc123\r\n\r\n";
        assert_eq!(extract_bearer_token(req), None);
    }

    #[test]
    fn normalize_path_strips_trailing_slash() {
        assert_eq!(normalize_path("/tasks/"), "/tasks");
    }

    #[test]
    fn normalize_path_handles_root() {
        assert_eq!(normalize_path("/"), "/");
    }

    #[test]
    fn normalize_path_handles_empty() {
        assert_eq!(normalize_path(""), "/");
    }

    #[test]
    fn split_path_query_with_query_string() {
        let (path, query) = split_path_query("/tasks?limit=10");
        assert_eq!(path, "/tasks");
        assert_eq!(query, "limit=10");
    }

    #[test]
    fn split_path_query_without_query() {
        let (path, query) = split_path_query("/tasks");
        assert_eq!(path, "/tasks");
        assert_eq!(query, "");
    }

    #[test]
    fn read_body_from_request() {
        let req = "POST / HTTP/1.1\r\nContent-Length: 13\r\n\r\n{\"key\":\"val\"}";
        assert_eq!(read_body(req), "{\"key\":\"val\"}");
    }

    #[test]
    fn parse_json_valid() {
        #[derive(serde::Deserialize)]
        struct T {
            x: i32,
        }
        let result: Result<T, _> = parse_json("{\"x\": 42}");
        assert_eq!(result.unwrap().x, 42);
    }

    #[test]
    fn parse_json_invalid_returns_400() {
        #[derive(Debug, serde::Deserialize)]
        struct T {
            _x: i32,
        }
        let result: Result<T, _> = parse_json("not json");
        let err = result.unwrap_err();
        assert_eq!(err.status, "400 Bad Request");
    }

    #[test]
    fn json_response_serializes() {
        let resp = json_response(&serde_json::json!({"ok": true}));
        assert_eq!(resp.status, "200 OK");
        assert!(resp.body.contains("\"ok\":true"));
    }

    #[test]
    fn error_response_includes_error_code() {
        let err = A2aError::unsupported_operation("streaming not supported");
        let resp = error_response("501 Not Implemented", &err);
        assert_eq!(resp.status, "501 Not Implemented");
        assert!(resp.body.contains("-32004"));
    }
}
