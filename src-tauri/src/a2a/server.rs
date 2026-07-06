use std::sync::Arc;

use tokio::net::TcpListener;

use crate::http_util::{
    self, encode_response, extract_auth, json_response, read_body, split_path_query, AuthScheme,
    CorsPolicy, HttpLimits,
};
use crate::live_server::LiveResponse;

use super::{
    adapter::{self, A2aAdapterState},
    types::A2aError,
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
        // Discovery — unchanged
        ("GET", "/.well-known/agent.json") => {
            let pm = state.adapter.plugin_manager.lock().await;
            let settings = state.adapter.settings.lock().await;
            let base_url = a2a_base_url(&settings);
            let skills = state.adapter.skill_manager.lock().await;
            let card = adapter::build_agent_card_with_skills(&base_url, &pm, Some(&skills));
            json_response(&card)
        }

        // JSON-RPC 2.0 endpoint — the single write route.
        ("POST", "/") => {
            let body = read_body(request);
            let response_body = super::jsonrpc::dispatch(&state.adapter, &body).await;
            LiveResponse {
                status: "200 OK",
                content_type: "application/json; charset=utf-8",
                body: response_body,
            }
        }

        // 404 — everything else, including the removed legacy routes.
        _ => LiveResponse::text("404 Not Found", "Not Found".to_string()),
    }
}

// ── A2A-specific helpers (not shared) ───────────────────────────────────────

/// Build a JSON error response from an [`A2aError`]. Kept here because it
/// carries A2A-specific wire format (error code, JSON shape) that the other
/// servers don't use. Retained for the a2a::server tests that exercise the
/// error-body encoding; not currently used by the live router now that all
/// error paths go through the JSON-RPC dispatcher.
#[allow(dead_code)]
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

    fn test_skill_manager() -> SkillManager {
        let skill_root = tempfile::tempdir().unwrap();
        let skill_dir = skill_root.path().join("route-demo-skill");
        std::fs::create_dir(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: route-demo-skill
description: Route demo skill for A2A discovery
tags: route, a2a
---

# Route Demo Skill
"#,
        )
        .unwrap();

        let mut skill_manager = SkillManager::new();
        skill_manager.load_from_dir(skill_root.path());
        skill_manager
    }

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
                skill_manager: Arc::new(Mutex::new(test_skill_manager())),
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
        assert!(
            card.skills
                .iter()
                .any(|skill| skill.id == "plugin:tool:app_launcher"),
            "expected plugin-derived capabilities to remain exposed"
        );

        let route_skill = card
            .skills
            .iter()
            .find(|skill| skill.id == "skill:route-demo-skill")
            .expect("loaded skill should be exposed by the discovery route");
        assert_eq!(route_skill.name, "route-demo-skill");
        assert_eq!(
            route_skill.description.as_deref(),
            Some("Route demo skill for A2A discovery")
        );
        assert!(route_skill.tags.iter().any(|tag| tag == "route"));
        assert!(route_skill.input_schema.is_some());
    }

    #[tokio::test]
    async fn options_returns_204_without_auth() {
        let state = test_server_state();
        let resp =
            handle_a2a_request(&state, "OPTIONS", "/anything", "OPTIONS / HTTP/1.1\r\n\r\n").await;
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

    #[tokio::test]
    async fn post_root_requires_bearer_token() {
        let state = test_server_state();

        let unauthorized = handle_a2a_request(
            &state,
            "POST",
            "/",
            "POST / HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}",
        )
        .await;
        assert_eq!(unauthorized.status, "401 Unauthorized");
    }

    #[tokio::test]
    async fn post_root_message_send_returns_jsonrpc_task() {
        let state = test_server_state();

        // Hub-shaped envelope: skillId names a plugin capability from the
        // test PluginManager.
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"message/send","params":{"message":{"role":"user","messageId":"m1","parts":[{"type":"text","text":"hi"}]},"contextId":"ctx-1","skillId":"plugin:tool:calculator"}}"#;
        let content_length = body.len();
        let request = format!(
            "POST / HTTP/1.1\r\nAuthorization: Bearer test-token\r\nContent-Length: {content_length}\r\n\r\n{body}"
        );
        let resp = handle_a2a_request(&state, "POST", "/", &request).await;

        assert_eq!(resp.status, "200 OK");
        let parsed: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], 1);
        assert!(
            parsed["result"].is_object() || parsed["error"].is_object(),
            "response must have exactly one of result/error"
        );
        // On success the task carries the context id back.
        if parsed["result"].is_object() {
            assert_eq!(parsed["result"]["contextId"], "ctx-1");
        }
    }
}
