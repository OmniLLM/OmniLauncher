use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

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
        A2aArtifact, A2aError, A2aMessage, A2aPart, A2aTask, AgentCapabilities, AgentCard,
        AgentInterface, HttpAuthSecurityScheme, MessageSendRequest, SecurityRequirement,
        SecurityScheme, StringList, A2A_PROTOCOL_VERSION,
    },
};

// ── Adapter state ───────────────────────────────────────────────────────────

/// Shared state for the A2A adapter. Mirrors the same `Arc<Mutex<...>>` pattern
/// used by `server::ServerState` so we can share the same underlying instances.
#[derive(Clone)]
pub struct A2aAdapterState {
    pub plugin_manager: Arc<RwLock<PluginManager>>,
    pub ai_client: Arc<RwLock<AiClient>>,
    pub settings: Arc<RwLock<AppSettings>>,
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
///
/// The card follows the A2A v1.0 `AgentCard` shape: the endpoint is advertised
/// via `supportedInterfaces` (replacing the pre-1.0 top-level `url`), auth via
/// `securitySchemes`/`securityRequirements`, and — per **§A.2.2** —
/// `extendedAgentCard` lives inside `capabilities`.
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
        supported_interfaces: vec![AgentInterface {
            url: base_url.to_string(),
            protocol_binding: "JSONRPC".to_string(),
            protocol_version: A2A_PROTOCOL_VERSION.to_string(),
        }],
        version: "0.1.0".to_string(),
        capabilities: AgentCapabilities {
            streaming: false,
            push_notifications: false,
            // We serve a single card; there is no authenticated extended
            // variant, so this is honestly false rather than aspirational.
            extended_agent_card: false,
        },
        security_schemes: [(
            "bearer".to_string(),
            SecurityScheme {
                http_auth_security_scheme: Some(HttpAuthSecurityScheme {
                    scheme: "Bearer".to_string(),
                    description: Some(
                        "Per-launch A2A token; see `ol settings get a2a_token`.".to_string(),
                    ),
                }),
            },
        )]
        .into_iter()
        .collect(),
        // No scopes: the token is all-or-nothing.
        security_requirements: vec![SecurityRequirement {
            schemes: [("bearer".to_string(), StringList::default())]
                .into_iter()
                .collect(),
        }],
        default_input_modes: vec!["text/plain".to_string(), "application/json".to_string()],
        default_output_modes: vec!["text/plain".to_string(), "application/json".to_string()],
        skills,
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

    // Create the task and mark it working.
    let task_id = {
        let mut reg = state.task_registry.lock().await;
        let id = reg.create_submitted(summary.clone(), None, context_id);
        reg.mark_working(&id);
        id
    };

    // Resolve the execution path *before* spawning so we can fail fast on
    // unknown tool ids without paying background-task overhead.
    let tool_name = request.tool.clone();
    let resolved_kind = match tool_name.as_deref() {
        Some(name) => {
            let pm = state.plugin_manager.read().await;
            let sm = state.skill_manager.lock().await;
            let kind = capabilities::find_capability(&pm, Some(&sm), name).map(|cap| cap.kind);
            match kind {
                Some(k) => Some((name.to_string(), k)),
                None => {
                    // Fail the task synchronously — no point spawning for an
                    // unknown tool.
                    let mut reg = state.task_registry.lock().await;
                    reg.mark_failed(&task_id, format!("Tool not found: {name}"));
                    return reg
                        .get(&task_id)
                        .map(|r| r.to_a2a_task())
                        .ok_or_else(|| A2aError::internal_error("task unexpectedly missing"));
                }
            }
        }
        None => None,
    };

    // Spawn background execution — the caller receives the task in `working`
    // state immediately and polls `tasks/get` for the final result.
    let bg_state = state.clone();
    let bg_task_id = task_id.clone();
    tokio::spawn(async move {
        let result = match resolved_kind {
            Some((ref name, capabilities::A2aCapabilityKind::Skill)) => {
                execute_conversational(&bg_state, &request, Some(name)).await
            }
            Some((ref name, _)) => execute_direct_tool(&bg_state, name, &request).await,
            None => execute_conversational(&bg_state, &request, None).await,
        };

        // Finalize the task.
        match result {
            Ok((messages, artifacts)) => {
                let mut reg = bg_state.task_registry.lock().await;
                if reg.is_cancel_requested(&bg_task_id) {
                    reg.cancel(&bg_task_id);
                } else {
                    reg.mark_completed(&bg_task_id, messages, artifacts);
                }
            }
            Err(err_msg) => {
                let masked = log_masking::mask_str(&err_msg);
                let mut reg = bg_state.task_registry.lock().await;
                reg.mark_failed(&bg_task_id, masked);
            }
        }
    });

    // Return the task in its current (working) state.
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
    let pm = state.plugin_manager.read().await;
    let sm = state.skill_manager.lock().await;
    capabilities::execute_capability(&pm, Some(&sm), tool_name, request).await
}

/// Execute a conversational AI request.
///
/// A2A is stateless: we build a fresh, request-scoped `ConversationContext` from
/// the request's own `messages` rather than reusing the desktop UI's shared
/// conversation. This (1) guarantees the caller's latest message is present as a
/// user turn — `Router::ai_route` reads the prompt from the context, it does not
/// add the query itself — and (2) isolates external A2A callers from the desktop
/// chat so unrelated history never leaks across.
///
/// `skill_hint`, when set, names a `skill:*` capability the client asked for.
/// It is turned into a short preface prepended to the user's query so the AI
/// router can pick up the intent via its existing skill-loading tools
/// (`load_skill`, `find_relevant`). The preface is intentionally minimal —
/// we tell the AI which skill to prefer but leave *how* to invoke it up to
/// the router's normal reasoning path.
async fn execute_conversational(
    state: &A2aAdapterState,
    request: &MessageSendRequest,
    skill_hint: Option<&str>,
) -> Result<(Vec<A2aMessage>, Vec<A2aArtifact>), String> {
    let pm = state.plugin_manager.read().await;
    let ai = state.ai_client.read().await;
    // Clone the skill manager so we don't hold its Mutex during the
    // potentially long-running AI call.
    let mut sm = state.skill_manager.lock().await.clone();
    let settings = state.settings.read().await;

    // Request-scoped, isolated context (NOT the shared desktop conversation).
    let max_turns = {
        let shared = state.conversation.lock().await;
        shared.max_turns
    };
    let conversation = build_request_context(request, max_turns);
    let base_query = latest_user_text(request);
    let query = match skill_hint.and_then(strip_skill_prefix) {
        Some(name) => format!(
            "The caller has selected the `{name}` skill. Load it if you need \
             its instructions, and use whichever tool the skill's SKILL.md \
             prescribes to answer the following:\n\n{base_query}"
        ),
        None => base_query,
    };

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

    let message = A2aMessage::agent_text(response.content);

    // If the AI returned structured results, include them as an artifact.
    let artifacts = if response.results.is_empty() {
        vec![]
    } else {
        vec![A2aArtifact {
            artifact_id: super::tasks::generate_task_id(),
            name: Some("results".to_string()),
            description: Some("Structured query results".to_string()),
            parts: vec![A2aPart::data(
                serde_json::to_value(&response.results).unwrap_or_default(),
            )],
            metadata: None,
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
        let text = msg.text();
        if text.is_empty() {
            continue;
        }
        // Normalize the A2A role onto our two conversational roles.
        let normalized = if msg.role.is_agent() {
            Message::assistant(&text)
        } else {
            Message::user(&text)
        };
        ctx.messages.push(normalized);
    }
    ctx.trim_to_max();
    ctx
}

/// Return the latest user-authored text in the request, used for skill matching.
fn latest_user_text(request: &MessageSendRequest) -> String {
    for msg in request.messages.iter().rev() {
        if msg.role.is_agent() {
            continue;
        }
        let text = msg.text();
        if !text.is_empty() {
            return text;
        }
    }
    String::new()
}

/// Strip the `skill:` prefix from a capability id, returning `Some(name)` if
/// the id names a Claude-Code-style skill capability. Returns `None` for
/// non-skill ids so the caller can leave the query untouched.
fn strip_skill_prefix(capability_id: &str) -> Option<&str> {
    capability_id.strip_prefix("skill:")
}

/// Extract a short text summary from a message-send request.
fn extract_text_summary(request: &MessageSendRequest) -> String {
    for msg in &request.messages {
        for part in &msg.parts {
            let Some(text) = part.as_text() else {
                continue;
            };
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Truncate to a reasonable summary length, on a char boundary so
            // multi-byte text can't panic the slice.
            if trimmed.len() <= 200 {
                return trimmed.to_string();
            }
            let cut = (0..=197)
                .rev()
                .find(|&i| trimmed.is_char_boundary(i))
                .unwrap_or(0);
            return format!("{}…", &trimmed[..cut]);
        }
    }
    "(empty request)".to_string()
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use crate::a2a::types::{A2aPart, A2aRole};
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
            plugin_manager: Arc::new(RwLock::new(pm)),
            ai_client: Arc::new(RwLock::new(AiClient::new(
                String::new(),
                String::new(),
                String::new(),
            ))),
            settings: Arc::new(RwLock::new(AppSettings::default())),
            conversation: Arc::new(Mutex::new(ConversationContext::new(10))),
            skill_manager: Arc::new(Mutex::new(SkillManager::new())),
            task_registry: Arc::new(Mutex::new(TaskRegistry::new(10))),
        }
    }

    /// Poll until the task reaches a terminal state (completed / failed /
    /// canceled / rejected). Background execution means `handle_message_send`
    /// returns the task in `working` state; tests that inspect the final result
    /// must wait for the background task to finish.
    ///
    /// Timeout is generous (30 s) because the conversational path with an
    /// unreachable AI endpoint runs the retry loop (up to 3 attempts with
    /// 2 s + 4 s backoff). Direct-execution tests typically finish in <100 ms.
    async fn await_terminal(state: &A2aAdapterState, task_id: &str) -> A2aTask {
        for _ in 0..3000 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            let reg = state.task_registry.lock().await;
            if let Some(r) = reg.get(task_id) {
                if r.state.is_terminal() {
                    return r.to_a2a_task();
                }
            }
        }
        panic!("task {task_id} did not reach terminal state within 30 s");
    }

    #[test]
    fn agent_card_includes_auth_and_capabilities() {
        let pm = PluginManager::new();
        let card = build_agent_card("http://127.0.0.1:1423", &pm);

        assert_eq!(card.name, "OmniLauncher");
        assert!(!card.capabilities.streaming);
        assert!(!card.capabilities.push_notifications);

        // v1.0 §A.2.2: extended-card support is a capability, not a top-level
        // field.
        assert!(!card.capabilities.extended_agent_card);

        // v1.0 §4.4.1: the endpoint is advertised via `supportedInterfaces`,
        // which replaced the pre-1.0 top-level `url`.
        assert_eq!(card.supported_interfaces.len(), 1);
        let iface = &card.supported_interfaces[0];
        assert_eq!(iface.url, "http://127.0.0.1:1423");
        assert_eq!(iface.protocol_binding, "JSONRPC");
        assert_eq!(iface.protocol_version, "1.0");

        // Auth is declared via spec `securitySchemes`, not the old
        // `authentication.schemes` block.
        let bearer = card
            .security_schemes
            .get("bearer")
            .expect("bearer scheme should be declared");
        assert_eq!(
            bearer
                .http_auth_security_scheme
                .as_ref()
                .map(|s| s.scheme.as_str()),
            Some("Bearer")
        );
        assert_eq!(card.security_requirements.len(), 1);
    }

    #[test]
    fn agent_card_serializes_without_legacy_fields() {
        let pm = PluginManager::new();
        let card = build_agent_card("http://127.0.0.1:1423", &pm);
        let value = serde_json::to_value(&card).unwrap();

        assert!(value.get("url").is_none(), "pre-1.0 top-level url emitted");
        assert!(
            value.get("authentication").is_none(),
            "pre-1.0 authentication block emitted"
        );
        assert!(
            value.get("supportsExtendedAgentCard").is_none(),
            "§A.2.2: relocated field emitted at top level"
        );
        assert_eq!(value["capabilities"]["extendedAgentCard"], false);
        assert_eq!(value["supportedInterfaces"][0]["protocolVersion"], "1.0");
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
        assert_eq!(query_skill.description, "Searches query-only test data");
        assert!(query_skill.tags.iter().any(|tag| tag == "qo"));
        // Schema-bearing capabilities accept structured JSON input.
        assert!(query_skill
            .input_modes
            .iter()
            .any(|mode| mode == "application/json"));
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
        assert_eq!(skill.description, "Demo skill for A2A discovery");
        assert!(skill.tags.iter().any(|tag| tag == "demo"));
    }

    #[tokio::test]
    async fn message_send_invokes_query_only_capability() {
        let state = test_adapter_state_with_plugin(Box::new(QueryOnlyPlugin));
        let request = MessageSendRequest {
            tool: Some("plugin:query:Query Only Test".to_string()),
            messages: vec![A2aMessage::new(
                A2aRole::User,
                vec![A2aPart::data(serde_json::json!({ "query": "needle" }))],
            )],
        };

        let task = handle_message_send(&state, request, None).await.unwrap();
        // Background execution — wait for completion.
        let task = await_terminal(&state, &task.id).await;

        assert_eq!(
            task.status.state,
            crate::a2a::types::A2aTaskState::Completed
        );
        let artifact = task.artifacts.first().expect("query results artifact");
        let data = artifact.parts[0]
            .as_data()
            .expect("query results artifact should be structured data");
        assert_eq!(data["results"][0]["title"], "Needle Result");
    }

    #[test]
    fn extract_text_summary_from_request() {
        let req = MessageSendRequest {
            messages: vec![A2aMessage::new(
                A2aRole::User,
                vec![A2aPart::text("What time is it?")],
            )],
            tool: None,
        };
        assert_eq!(extract_text_summary(&req), "What time is it?");
    }

    #[test]
    fn extract_text_summary_truncates_long_input() {
        let long = "x".repeat(300);
        let req = MessageSendRequest {
            messages: vec![A2aMessage::new(A2aRole::User, vec![A2aPart::text(long)])],
            tool: None,
        };
        let summary = extract_text_summary(&req);
        assert!(summary.len() <= 201);
        assert!(summary.ends_with('…'));
    }

    fn text_msg(role: A2aRole, text: &str) -> A2aMessage {
        A2aMessage::new(role, vec![A2aPart::text(text)])
    }

    #[test]
    fn build_request_context_includes_current_user_query_as_user_turn() {
        // Regression: the A2A conversational route used to call ai_route without
        // ever adding the incoming query as a user turn, so the model answered
        // from stale shared context and ignored the request entirely.
        let req = MessageSendRequest {
            messages: vec![text_msg(A2aRole::User, "show me all blz cn aws accounts")],
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
                text_msg(A2aRole::User, "first question"),
                text_msg(A2aRole::Agent, "first answer"),
                text_msg(A2aRole::User, "second question"),
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
                text_msg(A2aRole::User, "old question"),
                text_msg(A2aRole::Agent, "an answer"),
                text_msg(A2aRole::User, "newest question"),
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
            messages: vec![A2aMessage::new(
                A2aRole::User,
                vec![A2aPart::data(serde_json::json!({ "query": "needle" }))],
            )],
        };

        let task = handle_message_send(&state, request, Some("ctx-777".to_string()))
            .await
            .unwrap();
        let task = await_terminal(&state, &task.id).await;

        assert_eq!(task.context_id.as_deref(), Some("ctx-777"));
        assert!(!task.artifacts.is_empty());
        assert!(
            !task.artifacts[0].artifact_id.is_empty(),
            "artifact_id must be populated for wire-compatible output"
        );
    }

    // ── skill-routing tests ─────────────────────────────────────────────────
    //
    // Regression coverage for the pass-through fix: A2A `skillId` values that
    // name `skill:*` capabilities must not be dispatched as direct tool calls;
    // they must fall through to the AI conversational path (which understands
    // how to load a SKILL.md and pick the right execution mechanism).

    #[test]
    fn strip_skill_prefix_only_matches_skill_kind() {
        assert_eq!(strip_skill_prefix("skill:gcp"), Some("gcp"));
        assert_eq!(strip_skill_prefix("skill:demo-skill"), Some("demo-skill"));
        // plugin/launcher ids are legitimate direct-execution calls; their
        // prefix must NOT be stripped or the adapter would route them wrong.
        assert_eq!(strip_skill_prefix("plugin:tool:calculator"), None);
        assert_eq!(strip_skill_prefix("plugin:query:Echo"), None);
        assert_eq!(strip_skill_prefix("launcher:query_all"), None);
        assert_eq!(strip_skill_prefix(""), None);
    }

    /// Build a state where `skill:demo-skill` is a discoverable capability.
    /// The plugin manager is otherwise empty so any accidental direct-tool
    /// dispatch (which would call `pm.execute_tool("execute_skill", ...)`)
    /// would surface as a failure.
    fn state_with_demo_skill_only() -> A2aAdapterState {
        let skill_root = tempfile::tempdir().unwrap();
        let skill_dir = skill_root.path().join("demo-skill");
        std::fs::create_dir(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: demo-skill
description: Demo skill used to exercise A2A routing
tags: demo, a2a
---

# Demo Skill
"#,
        )
        .unwrap();
        let mut skill_manager = SkillManager::new();
        skill_manager.load_from_dir(skill_root.path());

        A2aAdapterState {
            plugin_manager: Arc::new(RwLock::new(PluginManager::new())),
            ai_client: Arc::new(RwLock::new(AiClient::new(
                String::new(),
                String::new(),
                String::new(),
            ))),
            settings: Arc::new(RwLock::new(AppSettings::default())),
            conversation: Arc::new(Mutex::new(ConversationContext::new(10))),
            skill_manager: Arc::new(Mutex::new(skill_manager)),
            task_registry: Arc::new(Mutex::new(TaskRegistry::new(10))),
        }
    }

    #[tokio::test]
    async fn skill_capability_does_not_take_the_direct_dispatch_path() {
        // The former routing bug: any `skillId` was fed straight into
        // `capabilities::execute_capability`, which for `skill:*` invoked the
        // `execute_skill` plugin tool. When that tool isn't registered (or,
        // in production, when the skill lacks a `run.py`), the task failed
        // with either "Tool not found: execute_skill" or "Skill 'foo' does
        // not have a run.py entrypoint at ...". This test asserts we no
        // longer surface either of those markers for a `skill:*` request —
        // proving the request took the conversational branch instead of the
        // direct-dispatch branch.
        let state = state_with_demo_skill_only();
        let request = MessageSendRequest {
            tool: Some("skill:demo-skill".to_string()),
            messages: vec![A2aMessage::new(
                A2aRole::User,
                vec![A2aPart::text("run the demo skill")],
            )],
        };

        let task = handle_message_send(&state, request, None).await.unwrap();
        let task = await_terminal(&state, &task.id).await;

        // The AI client is unreachable in tests, so `ai_route` returns a
        // stub response — but critically, the task must NOT carry the
        // direct-path failure markers.
        let all_text = format!("{task:?}");
        assert!(
            !all_text.contains("Tool not found: execute_skill"),
            "skill:* was incorrectly dispatched via the direct-execute path:\n{all_text}"
        );
        assert!(
            !all_text.contains("does not have a run.py entrypoint"),
            "skill:* was incorrectly dispatched via the run.py-only skill runner:\n{all_text}"
        );
    }

    #[tokio::test]
    async fn plugin_query_capability_still_takes_the_direct_dispatch_path() {
        // Positive counterpart: `plugin:query:*` capabilities are structured,
        // direct-execution calls (the client already has the arguments and
        // just wants the plugin's output). They must not be diverted to the
        // AI. We assert the query-only plugin's structured result surfaces
        // as an artifact — something only the direct path produces.
        let state = test_adapter_state_with_plugin(Box::new(QueryOnlyPlugin));
        let request = MessageSendRequest {
            tool: Some("plugin:query:Query Only Test".to_string()),
            messages: vec![A2aMessage::new(
                A2aRole::User,
                vec![A2aPart::data(serde_json::json!({ "query": "needle" }))],
            )],
        };

        let task = handle_message_send(&state, request, None).await.unwrap();
        let task = await_terminal(&state, &task.id).await;

        assert_eq!(
            task.status.state,
            crate::a2a::types::A2aTaskState::Completed
        );
        let artifact = task
            .artifacts
            .first()
            .expect("direct dispatch must yield the plugin's query artifact");
        let data = artifact.parts[0]
            .as_data()
            .expect("query artifact should be structured data, not text");
        assert_eq!(data["results"][0]["title"], "Needle Result");
    }

    #[tokio::test]
    async fn unknown_skill_id_yields_failed_task_not_a_panic() {
        // `Tool not found: <id>` is the correct response when the caller
        // names a `skillId` that no capability advertises. Regression guard
        // against accidentally routing unknown ids to the conversational
        // path (which would swallow them silently).
        let state = test_adapter_state_with_plugin(Box::new(QueryOnlyPlugin));
        let request = MessageSendRequest {
            tool: Some("plugin:tool:does_not_exist".to_string()),
            messages: vec![A2aMessage::new(
                A2aRole::User,
                vec![A2aPart::text("irrelevant")],
            )],
        };

        let task = handle_message_send(&state, request, None).await.unwrap();

        assert_eq!(task.status.state, crate::a2a::types::A2aTaskState::Failed);
        let text = format!("{task:?}");
        assert!(
            text.contains("Tool not found"),
            "expected failure message to name the missing tool:\n{text}"
        );
    }
}
