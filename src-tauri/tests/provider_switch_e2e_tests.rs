//! End-to-end regression test for the "switch provider/model does not work"
//! bug: switching the active provider through `POST /api/settings` had no
//! effect because the settings request payload dropped the `providers` and
//! `active_provider_id` fields, so the running server kept routing to the old
//! (e.g. GitHub Copilot) provider.
//!
//! This test drives a REAL HTTP server on an ephemeral port, exactly like the
//! desktop shell does, and asserts that after a switch:
//!   1. `GET /api/settings` reports the new active provider, and
//!   2. the live `AiClient` actually re-resolved to the new provider's URL/model
//!      (i.e. it is no longer a Copilot client hitting api.githubcopilot.com).

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock};

use omnilauncher_lib::{
    ai::{client::AiClient, router::ConversationContext},
    server::{bind_api_listener, serve_bound, EventBus, ServerState},
    skills::SkillManager,
    AppSettings, PluginManager, Provider, ProviderKind,
};

const TOKEN: &str = "switch-e2e-token";

/// Build settings with two providers: a GitHub Copilot provider (active) and a
/// custom OpenAI-compatible provider we will switch to.
fn seed_settings() -> AppSettings {
    let copilot = Provider {
        id: "copilot-1".into(),
        name: "GitHub Copilot".into(),
        kind: ProviderKind::GithubCopilot,
        // A pre-exchanged, far-future token so from_settings does not try to
        // reach the network during client construction.
        copilot_github_token: "gho_fake".into(),
        copilot_token: "cop-fake-token".into(),
        copilot_token_expiry: i64::MAX,
        model: "gpt-4.1".into(),
        ..Provider::default()
    };
    let custom = Provider {
        id: "custom-1".into(),
        name: "Local OpenAI".into(),
        kind: ProviderKind::Custom,
        base_url: "http://127.0.0.1:9911".into(),
        api_key: "sk-local".into(),
        model: "llama-3".into(),
        ..Provider::default()
    };

    let mut settings = AppSettings {
        providers: vec![copilot.clone(), custom],
        active_provider_id: copilot.id.clone(),
        ..AppSettings::default()
    };
    // Keep legacy flat fields consistent with the active (Copilot) provider.
    settings.sync_legacy_ai_fields_from_active_provider();
    settings
}

fn make_state(settings: AppSettings) -> ServerState {
    let client = AiClient::from_settings(&settings);
    ServerState {
        plugin_manager: Arc::new(RwLock::new(PluginManager::new())),
        ai_client: Arc::new(RwLock::new(client)),
        settings: Arc::new(RwLock::new(settings)),
        conversation: Arc::new(Mutex::new(ConversationContext::default())),
        ai_in_flight: Arc::new(tokio::sync::Semaphore::new(1)),
        current_ai_task: Arc::new(Mutex::new(None)),
        skill_manager: Arc::new(Mutex::new(SkillManager::new())),
        event_bus: EventBus::default(),
        latest_selection: Arc::new(Mutex::new(None)),
        auth_token: Arc::new(TOKEN.to_string()),
    }
}

/// Minimal raw-HTTP request/response helper: sends a full request and returns
/// the whole response text (headers + body).
async fn http(port: u16, method: &str, path: &str, body: Option<&str>) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let request = match body {
        Some(b) => format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nX-OmniLauncher-Token: {TOKEN}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
            b.len()
        ),
        None => format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nX-OmniLauncher-Token: {TOKEN}\r\n\
             Connection: close\r\n\r\n"
        ),
    };
    stream.write_all(request.as_bytes()).await.unwrap();

    let mut out = String::new();
    // Read to EOF (server sets Connection: close).
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        stream.read_to_string(&mut out),
    )
    .await
    .expect("server should respond promptly");
    out
}

fn body_of(response: &str) -> &str {
    response
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .unwrap_or("")
}

#[tokio::test]
async fn switching_active_provider_reroutes_the_live_client() {
    // Isolate config writes to a temp dir so save_settings does not touch real
    // user config (and is permitted under the test-build guard). Each integration
    // test binary is its own process, so there is no in-process env race here.
    let tmp = std::env::temp_dir().join(format!("oml-switch-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    std::env::set_var("OMNILAUNCHER_CONFIG_DIR", &tmp);

    let state = make_state(seed_settings());

    // Sanity: the server starts out routed to Copilot.
    {
        let client = state.ai_client.read().await;
        assert!(
            client.chat_url().contains("githubcopilot.com"),
            "precondition: active client should be Copilot, got {}",
            client.chat_url()
        );
    }

    let listener = bind_api_listener("127.0.0.1", 0).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server_state = state.clone();
    tokio::spawn(async move { serve_bound(listener, server_state).await });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // 1. GET current settings (as the frontend does on open).
    let get = http(port, "GET", "/api/settings", None).await;
    assert!(
        get.starts_with("HTTP/1.1 200"),
        "GET settings failed: {get}"
    );
    let settings: serde_json::Value = serde_json::from_str(body_of(&get)).unwrap();
    assert_eq!(settings["active_provider_id"], "copilot-1");

    // 2. POST settings switching the active provider to the custom one. The
    //    frontend echoes back the full providers registry plus the new
    //    active_provider_id and the selected provider's flat fields.
    let post_body = serde_json::json!({
        "ai_base_url": "http://127.0.0.1:9911",
        "ai_model": "llama-3",
        "ai_api_key": "sk-local",
        "ai_timeout_secs": 120,
        "ai_max_tool_iterations": 10,
        "ai_max_retry_attempts": 3,
        "ai_retry_base_delay_ms": 2000,
        "theme": "system",
        "hotkey": "Ctrl+Shift+O",
        "max_results": 10,
        "background_url": "",
        "providers": settings["providers"],
        "active_provider_id": "custom-1",
    })
    .to_string();
    let post = http(port, "POST", "/api/settings", Some(&post_body)).await;
    assert!(
        post.starts_with("HTTP/1.1 200"),
        "POST settings should succeed: {post}"
    );

    // 3. The persisted/in-memory settings now report the custom provider.
    let stored = state.settings.read().await.clone();
    assert_eq!(
        stored.active_provider_id, "custom-1",
        "active provider must switch to custom-1"
    );

    // 4. The LIVE client was rebuilt and no longer routes to Copilot — this is
    //    the actual bug: before the fix it stayed on githubcopilot.com.
    {
        let client = state.ai_client.read().await;
        assert!(
            !client.chat_url().contains("githubcopilot.com"),
            "client must NOT route to Copilot after switch, got {}",
            client.chat_url()
        );
        assert!(
            client.chat_url().contains("127.0.0.1:9911"),
            "client should route to the custom provider base_url, got {}",
            client.chat_url()
        );
        assert_eq!(client.model(), "llama-3", "client model should switch");
    }

    std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");
    let _ = std::fs::remove_dir_all(&tmp);
}
