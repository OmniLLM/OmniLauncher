use std::sync::Arc;
use tokio::sync::Mutex;

use omnilauncher_lib::{
    ai::{client::AiClient, router::ConversationContext},
    create_plugin_manager_builtin_only, load_settings,
    server::{ai_query_backend, EventBus, ServerState},
    skills::SkillManager,
};

fn integration_enabled() -> bool {
    std::env::var("OMNILAUNCHER_RUN_LIVE_AI_TESTS")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

#[tokio::test]
async fn frontend_like_ai_query_alibaba_ecs_returns_a_number() {
    if !integration_enabled() {
        eprintln!(
            "skipping live AI integration test; set OMNILAUNCHER_RUN_LIVE_AI_TESTS=1 to enable"
        );
        return;
    }

    let settings = load_settings();
    if settings.ai_base_url.trim().is_empty() {
        eprintln!("skipping live AI integration test; no AI base URL configured");
        return;
    }

    let mut skill_manager = SkillManager::new();
    skill_manager.reload();

    let state = ServerState {
        plugin_manager: Arc::new(Mutex::new(create_plugin_manager_builtin_only())),
        ai_client: Arc::new(Mutex::new(AiClient::new(
            settings.ai_base_url.clone(),
            settings.resolve_ai_api_key(),
            settings.ai_model.clone(),
        ))),
        settings: Arc::new(Mutex::new(settings.clone())),
        conversation: Arc::new(Mutex::new(ConversationContext::default())),
        ai_in_flight: Arc::new(tokio::sync::Semaphore::new(1)),
        current_ai_task: Arc::new(Mutex::new(None)),
        skill_manager: Arc::new(Mutex::new(skill_manager)),
        event_bus: EventBus::default(),
        latest_selection: Arc::new(Mutex::new(None)),
        auth_token: Arc::new(omnilauncher_lib::server::generate_auth_token()),
    };

    let mut done_rx = state.event_bus.subscribe("omnilauncher://ai-done").await;
    let mut err_rx = state.event_bus.subscribe("omnilauncher://ai-error").await;

    ai_query_backend("?how many ECS VMs in alibaba".to_string(), state.clone())
        .await
        .unwrap_or_else(|e| panic!("ai_query_backend failed: {e}"));

    let payload = tokio::select! {
        done = tokio::time::timeout(std::time::Duration::from_secs(180), done_rx.recv()) => {
            done.expect("timed out waiting for ai-done").expect("ai-done channel closed")
        }
        err = tokio::time::timeout(std::time::Duration::from_secs(180), err_rx.recv()) => {
            let msg = err.expect("timed out waiting for ai-error").expect("ai-error channel closed");
            panic!("backend emitted ai-error: {msg}");
        }
    };

    let response: omnilauncher_lib::AiResponse =
        serde_json::from_str(&payload).expect("invalid ai-done payload");

    eprintln!("AI content: {}", response.content);
    assert!(response.is_ai, "expected an AI response");
    assert!(
        !response.content.starts_with("AI error:"),
        "expected a successful AI answer, got backend error: {}",
        response.content
    );
    assert!(
        response
            .content
            .split(|c: char| !c.is_ascii_digit())
            .any(|part| !part.is_empty()),
        "expected the final answer to contain a number, got: {}",
        response.content
    );
}
