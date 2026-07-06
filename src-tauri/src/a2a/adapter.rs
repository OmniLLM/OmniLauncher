use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    ai::{
        client::{AiClient, Message},
        router::ConversationContext,
    },
    log_masking,
    plugins::PluginManager,
    AppSettings, SkillManager,
};

use super::{
    capabilities::{self, build_capabilities, capability_to_agent_skill},
    tasks::TaskRegistry,
    types::{
        A2aArtifact, A2aError, A2aMessage, A2aPart, A2aTask, AgentAuthentication,
        AgentCapabilities, AgentCard, MessageSendRequest,
    },
};

// ── Adapter state ───────────────────────────────────────────────────────────

/// Shared state for the A2A adapter. Mirrors the same `Arc<Mutex<...>>` pattern
/// used by `server::ServerState` so we can share the same underlying instances.
#[derive(Clone)]
pub struct A2aAdapterState {
    pub plugin_manager: Arc<Mutex<PluginManager>>,
    pub ai_client: Arc<Mutex<AiClient>>,
    pub settings: Arc<Mutex<AppSettings>>,
    pub conversation: Arc<Mutex<ConversationContext>>,
    pub skill_manager: Arc<Mutex<SkillManager>>,
    pub task_registry: Arc<Mutex<TaskRegistry>>,
}

// ── Agent Card generation ───────────────────────────────────────────────────

/// Build the Agent Card from current runtime state.
///
/// Iterates all plugin tool schemas and generates conservative descriptions for
/// plugins that lack explicit schemas.
pub fn build_agent_card(base_url: &str, pm: &PluginManager) -> AgentCard {
    build_agent_card_with_skills(base_url, pm, None)
}

/// Build the Agent Card, optionally including installed skills as capabilities.
///
/// When a `SkillManager` is supplied, each loaded skill is advertised as an
/// A2A skill (`skill:<name>`) alongside the plugin-derived capabilities.
pub fn build_agent_card_with_skills(
    base_url: &str,
    pm: &PluginManager,
    skills: Option<&SkillManager>,
) -> AgentCard {
    let skills = build_capabilities(pm, skills)
        .iter()
        .map(capability_to_agent_skill)
        .collect();

    AgentCard {
        name: "OmniLauncher".to_string(),
        description: "OmniLauncher desktop agent — launcher, AI chat, and developer tools"
            .to_string(),
        url: base_url.to_string(),
        version: Some("0.1.0".to_string()),
        capabilities: AgentCapabilities {
            streaming: false,
            push_notifications: false,
            state_transition_history: false,
        },
        authentication: AgentAuthentication {
            schemes: vec!["bearer".to_string()],
        },
        skills,
        default_input_modes: vec!["text/plain".to_string()],
        default_output_modes: vec!["text/plain".to_string()],
    }
}

// ── Message handling ────────────────────────────────────────────────────────

/// Handle a `message/send` request.
///
/// Detects whether the request is conversational (plain text, no `tool` field)
/// or a direct tool invocation. Creates a submitted task, runs the appropriate
/// execution path, and marks the task completed or failed.
///
/// `context_id` is stored on the task and echoed back so callers (typically the
/// A2A hub) can correlate turns of a multi-turn conversation.
pub async fn handle_message_send(
    state: &A2aAdapterState,
    request: MessageSendRequest,
    context_id: Option<String>,
) -> Result<A2aTask, A2aError> {
    // Summarize the request for the task record.
    let summary = extract_text_summary(&request);

    // Create the task.
    let task_id = {
        let mut reg = state.task_registry.lock().await;
        reg.create_submitted(summary.clone(), None, context_id)
    };

    // Mark working.
    {
        let mut reg = state.task_registry.lock().await;
        reg.mark_working(&task_id);
    }

    // Determine execution path.
    let result = if let Some(ref tool_name) = request.tool {
        execute_direct_tool(state, tool_name, &request).await
    } else {
        execute_conversational(state, &request).await
    };

    // Finalize the task.
    match result {
        Ok((messages, artifacts)) => {
            let mut reg = state.task_registry.lock().await;
            // Check for late cancellation.
            if reg.is_cancel_requested(&task_id) {
                reg.cancel(&task_id);
            } else {
                reg.mark_completed(&task_id, messages, artifacts);
            }
        }
        Err(err_msg) => {
            let masked = log_masking::mask_str(&err_msg);
            let mut reg = state.task_registry.lock().await;
            reg.mark_failed(&task_id, masked);
        }
    }

    // Return the final task state.
    let reg = state.task_registry.lock().await;
    reg.get(&task_id)
        .map(|r| r.to_a2a_task())
        .ok_or_else(|| A2aError::internal_error("task unexpectedly missing"))
}

/// Execute a direct tool/plugin call.
async fn execute_direct_tool(
    state: &A2aAdapterState,
    tool_name: &str,
    request: &MessageSendRequest,
) -> Result<(Vec<A2aMessage>, Vec<A2aArtifact>), String> {
    let pm = state.plugin_manager.lock().await;
    capabilities::execute_capability(&pm, tool_name, request).await
}

/// Execute a conversational AI request.
///
/// A2A is stateless: we build a fresh, request-scoped `ConversationContext` from
/// the request's own `messages` rather than reusing the desktop UI's shared
/// conversation. This (1) guarantees the caller's latest message is present as a
/// user turn — `Router::ai_route` reads the prompt from the context, it does not
/// add the query itself — and (2) isolates external A2A callers from the desktop
/// chat so unrelated history never leaks across.
async fn execute_conversational(
    state: &A2aAdapterState,
    request: &MessageSendRequest,
) -> Result<(Vec<A2aMessage>, Vec<A2aArtifact>), String> {
    let pm = state.plugin_manager.lock().await;
    let ai = state.ai_client.lock().await;
    let mut sm = state.skill_manager.lock().await;
    let settings = state.settings.lock().await;

    // Request-scoped, isolated context (NOT the shared desktop conversation).
    let max_turns = {
        let shared = state.conversation.lock().await;
        shared.max_turns
    };
    let conversation = build_request_context(request, max_turns);
    let query = latest_user_text(request);

    let response = crate::ai::router::Router::ai_route(
        &query,
        &pm,
        &ai,
        &conversation,
        &mut sm,
        None, // no progress channel for A2A
        settings.ai_max_tool_iterations,
        settings.ai_loop_detector_enabled,
    )
    .await;

    let message = A2aMessage {
        role: "agent".to_string(),
        parts: vec![A2aPart::Text {
            text: response.content,
        }],
    };

    // If the AI returned structured results, include them as an artifact.
    let artifacts = if response.results.is_empty() {
        vec![]
    } else {
        vec![A2aArtifact {
            artifact_id: super::tasks::generate_task_id(),
            name: Some("results".to_string()),
            description: Some("Structured query results".to_string()),
            parts: vec![A2aPart::Data {
                data: serde_json::to_value(&response.results).unwrap_or_default(),
            }],
            index: 0,
        }]
    };

    Ok((vec![message], artifacts))
}

// ── Task operations ─────────────────────────────────────────────────────────

/// Handle `GET /tasks/{id}`.
pub async fn handle_task_get(state: &A2aAdapterState, task_id: &str) -> Result<A2aTask, A2aError> {
    let reg = state.task_registry.lock().await;
    reg.get(task_id)
        .map(|r| r.to_a2a_task())
        .ok_or_else(|| A2aError::task_not_found(task_id))
}

/// Handle `GET /tasks`.
pub async fn handle_task_list(state: &A2aAdapterState) -> Vec<A2aTask> {
    let reg = state.task_registry.lock().await;
    reg.list().iter().map(|r| r.to_a2a_task()).collect()
}

/// Handle `POST /tasks/{id}:cancel`.
pub async fn handle_task_cancel(
    state: &A2aAdapterState,
    task_id: &str,
) -> Result<A2aTask, A2aError> {
    let mut reg = state.task_registry.lock().await;
    if reg.get(task_id).is_none() {
        return Err(A2aError::task_not_found(task_id));
    }
    reg.cancel(task_id);
    reg.get(task_id)
        .map(|r| r.to_a2a_task())
        .ok_or_else(|| A2aError::internal_error("task unexpectedly missing after cancel"))
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Build a fresh, request-scoped conversation context from an A2A request.
///
/// A2A is stateless from the server's perspective: each request carries its own
/// `messages` history (the client owns the conversation). We therefore build a
/// brand-new `ConversationContext` containing exactly the request's turns —
/// never sharing or mutating the desktop UI's live conversation.
fn build_request_context(request: &MessageSendRequest, max_turns: usize) -> ConversationContext {
    let mut ctx = ConversationContext::new(max_turns);
    for msg in &request.messages {
        let text = request_message_text(msg);
        if text.is_empty() {
            continue;
        }
        // Normalize the A2A role onto our two conversational roles. Anything
        // that isn't explicitly the agent/assistant is treated as a user turn.
        let normalized = match msg.role.as_str() {
            "agent" | "assistant" => Message::assistant(&text),
            _ => Message::user(&text),
        };
        ctx.messages.push(normalized);
    }
    ctx.trim_to_max();
    ctx
}

/// Return the latest user-authored text in the request, used for skill matching.
fn latest_user_text(request: &MessageSendRequest) -> String {
    for msg in request.messages.iter().rev() {
        if matches!(msg.role.as_str(), "agent" | "assistant") {
            continue;
        }
        let text = request_message_text(msg);
        if !text.is_empty() {
            return text;
        }
    }
    String::new()
}

/// Concatenate all text parts of a single A2A message.
fn request_message_text(msg: &A2aMessage) -> String {
    let mut out = String::new();
    for part in &msg.parts {
        if let A2aPart::Text { text } = part {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(text);
        }
    }
    out.trim().to_string()
}

/// Extract a short text summary from a message-send request.
fn extract_text_summary(request: &MessageSendRequest) -> String {
    for msg in &request.messages {
        for part in &msg.parts {
            if let A2aPart::Text { text } = part {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    // Truncate to a reasonable summary length.
                    return if trimmed.len() > 200 {
                        format!("{}…", &trimmed[..197])
                    } else {
                        trimmed.to_string()
                    };
                }
            }
        }
    }
    "(empty request)".to_string()
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use crate::{
        ai::{client::AiClient, router::ConversationContext},
        plugins::{Plugin, Query, QueryResult},
        AppSettings, SkillManager,
    };
    use async_trait::async_trait;
    use tokio::sync::Mutex;

    struct QueryOnlyPlugin;

    #[async_trait]
    impl Plugin for QueryOnlyPlugin {
        fn name(&self) -> &str {
            "Query Only Test"
        }

        fn description(&self) -> &str {
            "Searches query-only test data"
        }

        fn keyword(&self) -> Option<&str> {
            Some("qo")
        }

        async fn query(&self, q: &Query) -> Vec<QueryResult> {
            if q.raw.contains("needle") {
                vec![QueryResult {
                    id: "query-only-hit".to_string(),
                    title: "Needle Result".to_string(),
                    subtitle: Some("Found by query-only plugin".to_string()),
                    icon: None,
                    score: 100,
                    action_type: "none".to_string(),
                    action_data: String::new(),
                    source: Some("Query Only Test".to_string()),
                }]
            } else {
                vec![]
            }
        }
    }

    fn test_adapter_state_with_plugin(plugin: Box<dyn Plugin>) -> A2aAdapterState {
        let mut pm = PluginManager::new();
        pm.register(plugin);

        A2aAdapterState {
            plugin_manager: Arc::new(Mutex::new(pm)),
            ai_client: Arc::new(Mutex::new(AiClient::new(
                String::new(),
                String::new(),
                String::new(),
            ))),
            settings: Arc::new(Mutex::new(AppSettings::default())),
            conversation: Arc::new(Mutex::new(ConversationContext::new(10))),
            skill_manager: Arc::new(Mutex::new(SkillManager::new())),
            task_registry: Arc::new(Mutex::new(TaskRegistry::new(10))),
        }
    }

    #[test]
    fn agent_card_includes_auth_and_capabilities() {
        let pm = PluginManager::new();
        let card = build_agent_card("http://127.0.0.1:1423", &pm);

        assert_eq!(card.name, "OmniLauncher");
        assert_eq!(card.authentication.schemes, vec!["bearer"]);
        assert!(!card.capabilities.streaming);
        assert!(!card.capabilities.push_notifications);
        assert_eq!(card.url, "http://127.0.0.1:1423");
    }

    #[test]
    fn agent_card_with_plugin_manager() {
        let pm = crate::create_plugin_manager_builtin_only();
        let card = build_agent_card("http://127.0.0.1:1423", &pm);

        // Should have at least some skills from built-in plugins.
        assert!(
            !card.skills.is_empty(),
            "expected skills from built-in plugins"
        );
    }

    #[test]
    fn agent_card_includes_query_only_plugin_capability() {
        let mut pm = PluginManager::new();
        pm.register(Box::new(QueryOnlyPlugin));

        let card = build_agent_card("http://127.0.0.1:1423", &pm);

        let query_skill = card
            .skills
            .iter()
            .find(|skill| skill.id == "plugin:query:Query Only Test")
            .expect("query-only plugin should be exposed as an A2A capability");
        assert_eq!(query_skill.name, "Query Only Test");
        assert_eq!(
            query_skill.description.as_deref(),
            Some("Searches query-only test data")
        );
        assert!(query_skill.tags.iter().any(|tag| tag == "qo"));
        assert!(query_skill.input_schema.is_some());
    }

    #[test]
    fn agent_card_includes_loaded_skill_capability() {
        let skill_root = tempfile::tempdir().unwrap();
        let skill_dir = skill_root.path().join("demo-skill");
        std::fs::create_dir(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: demo-skill
description: Demo skill for A2A discovery
tags: demo, a2a
---

# Demo Skill
"#,
        )
        .unwrap();
        let mut skill_manager = SkillManager::new();
        skill_manager.load_from_dir(skill_root.path());
        let pm = PluginManager::new();

        let card = build_agent_card_with_skills("http://127.0.0.1:1423", &pm, Some(&skill_manager));

        let skill = card
            .skills
            .iter()
            .find(|skill| skill.id == "skill:demo-skill")
            .expect("loaded skill should be exposed as an A2A capability");
        assert_eq!(skill.name, "demo-skill");
        assert_eq!(
            skill.description.as_deref(),
            Some("Demo skill for A2A discovery")
        );
        assert!(skill.tags.iter().any(|tag| tag == "demo"));
    }

    #[tokio::test]
    async fn message_send_invokes_query_only_capability() {
        let state = test_adapter_state_with_plugin(Box::new(QueryOnlyPlugin));
        let request = MessageSendRequest {
            tool: Some("plugin:query:Query Only Test".to_string()),
            messages: vec![A2aMessage {
                role: "user".to_string(),
                parts: vec![A2aPart::Data {
                    data: serde_json::json!({ "query": "needle" }),
                }],
            }],
        };

        let task = handle_message_send(&state, request, None).await.unwrap();

        assert_eq!(
            task.status.state,
            crate::a2a::types::A2aTaskState::Completed
        );
        let artifact = task.artifacts.first().expect("query results artifact");
        let A2aPart::Data { data } = &artifact.parts[0] else {
            panic!("query results artifact should be structured data");
        };
        assert_eq!(data["results"][0]["title"], "Needle Result");
    }

    #[test]
    fn extract_text_summary_from_request() {
        let req = MessageSendRequest {
            messages: vec![A2aMessage {
                role: "user".to_string(),
                parts: vec![A2aPart::Text {
                    text: "What time is it?".to_string(),
                }],
            }],
            tool: None,
        };
        assert_eq!(extract_text_summary(&req), "What time is it?");
    }

    #[test]
    fn extract_text_summary_truncates_long_input() {
        let long = "x".repeat(300);
        let req = MessageSendRequest {
            messages: vec![A2aMessage {
                role: "user".to_string(),
                parts: vec![A2aPart::Text { text: long }],
            }],
            tool: None,
        };
        let summary = extract_text_summary(&req);
        assert!(summary.len() <= 201);
        assert!(summary.ends_with('…'));
    }

    fn text_msg(role: &str, text: &str) -> A2aMessage {
        A2aMessage {
            role: role.to_string(),
            parts: vec![A2aPart::Text {
                text: text.to_string(),
            }],
        }
    }

    #[test]
    fn build_request_context_includes_current_user_query_as_user_turn() {
        // Regression: the A2A conversational route used to call ai_route without
        // ever adding the incoming query as a user turn, so the model answered
        // from stale shared context and ignored the request entirely.
        let req = MessageSendRequest {
            messages: vec![text_msg("user", "show me all blz cn aws accounts")],
            tool: None,
        };

        let ctx = build_request_context(&req, 10);

        assert_eq!(ctx.messages.len(), 1, "the user query must be present");
        assert_eq!(ctx.messages[0].role, "user");
        assert_eq!(
            ctx.messages[0].content_str(),
            "show me all blz cn aws accounts"
        );
    }

    #[test]
    fn build_request_context_is_isolated_and_preserves_multi_turn_order() {
        // A2A is stateless: the request carries its own history, and roles map
        // onto our user/assistant turns in order. Agent/assistant roles become
        // assistant turns; everything else becomes a user turn.
        let req = MessageSendRequest {
            messages: vec![
                text_msg("user", "first question"),
                text_msg("agent", "first answer"),
                text_msg("user", "second question"),
            ],
            tool: None,
        };

        let ctx = build_request_context(&req, 10);

        let roles: Vec<&str> = ctx.messages.iter().map(|m| m.role.as_str()).collect();
        assert_eq!(roles, vec!["user", "assistant", "user"]);
        assert_eq!(ctx.messages[2].content_str(), "second question");
    }

    #[test]
    fn latest_user_text_returns_most_recent_user_message() {
        let req = MessageSendRequest {
            messages: vec![
                text_msg("user", "old question"),
                text_msg("agent", "an answer"),
                text_msg("user", "newest question"),
            ],
            tool: None,
        };

        assert_eq!(latest_user_text(&req), "newest question");
    }

    #[tokio::test]
    async fn message_send_echoes_context_id_into_task() {
        let state = test_adapter_state_with_plugin(Box::new(QueryOnlyPlugin));
        let request = MessageSendRequest {
            tool: Some("plugin:query:Query Only Test".to_string()),
            messages: vec![A2aMessage {
                role: "user".to_string(),
                parts: vec![A2aPart::Data {
                    data: serde_json::json!({ "query": "needle" }),
                }],
            }],
        };

        let task = handle_message_send(&state, request, Some("ctx-777".to_string()))
            .await
            .unwrap();

        assert_eq!(task.context_id.as_deref(), Some("ctx-777"));
        assert!(!task.artifacts.is_empty());
        assert!(
            !task.artifacts[0].artifact_id.is_empty(),
            "artifact_id must be populated for wire-compatible output"
        );
    }
}
