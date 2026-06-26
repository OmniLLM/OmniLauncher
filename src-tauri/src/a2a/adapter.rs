use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    ai::{client::AiClient, router::ConversationContext},
    log_masking,
    plugins::PluginManager,
    AppSettings, SkillManager,
};

use super::{
    tasks::TaskRegistry,
    types::{
        A2aArtifact, A2aError, A2aMessage, A2aPart, A2aTask, AgentAuthentication,
        AgentCapabilities, AgentCard, AgentSkill, MessageSendRequest,
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
pub fn build_agent_card(
    base_url: &str,
    pm: &PluginManager,
) -> AgentCard {
    let mut skills: Vec<AgentSkill> = Vec::new();

    // Expose all plugins with AI tool schemas as first-class skills.
    for schema in pm.all_tool_schemas() {
        if let Some(func) = schema.get("function") {
            let name = func
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let desc = func
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let params = func.get("parameters").cloned();

            skills.push(AgentSkill {
                id: name.clone(),
                name,
                description: desc,
                input_schema: params,
                tags: vec![],
            });
        }
    }

    // Also expose plugins that lack tool schemas with conservative descriptions.
    for plugin in &pm.plugins {
        let name = plugin.name().to_string();
        // Skip if already covered by a tool schema above.
        if skills.iter().any(|s| s.id == name || s.name == name) {
            continue;
        }
        let desc = plugin.description().to_string();
        let mut tags = Vec::new();
        if let Some(kw) = plugin.keyword() {
            tags.push(kw.to_string());
        }
        skills.push(AgentSkill {
            id: name.clone(),
            name,
            description: if desc.is_empty() { None } else { Some(desc) },
            input_schema: None,
            tags,
        });
    }

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

/// Handle a `POST /message:send` request.
///
/// Detects whether the request is conversational (plain text, no `tool` field)
/// or a direct tool invocation. Creates a submitted task, runs the appropriate
/// execution path, and marks the task completed or failed.
pub async fn handle_message_send(
    state: &A2aAdapterState,
    request: MessageSendRequest,
) -> Result<A2aTask, A2aError> {
    // Summarize the request for the task record.
    let summary = extract_text_summary(&request);

    // Create the task.
    let task_id = {
        let mut reg = state.task_registry.lock().await;
        reg.create_submitted(summary.clone(), None)
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
        execute_conversational(state, &summary).await
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
    // Extract tool arguments from the first message's data part, or use the
    // text as a simple input.
    let args = extract_tool_args(request);

    let pm = state.plugin_manager.lock().await;
    let output = pm.execute_tool(tool_name, args).await;

    if output == "Tool not found" {
        return Err(format!("Tool not found: {tool_name}"));
    }

    let message = A2aMessage {
        role: "agent".to_string(),
        parts: vec![A2aPart::Text { text: output }],
    };
    Ok((vec![message], vec![]))
}

/// Execute a conversational AI request.
async fn execute_conversational(
    state: &A2aAdapterState,
    query: &str,
) -> Result<(Vec<A2aMessage>, Vec<A2aArtifact>), String> {
    let pm = state.plugin_manager.lock().await;
    let ai = state.ai_client.lock().await;
    let conversation = state.conversation.lock().await;
    let mut sm = state.skill_manager.lock().await;
    let settings = state.settings.lock().await;

    let response = crate::ai::router::Router::ai_route(
        query,
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
pub async fn handle_task_get(
    state: &A2aAdapterState,
    task_id: &str,
) -> Result<A2aTask, A2aError> {
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

/// Extract tool arguments from a message-send request.
///
/// If the first message contains a `Data` part, use it as the JSON arguments.
/// Otherwise, wrap the text content as a `{"input": "..."}` object.
fn extract_tool_args(request: &MessageSendRequest) -> serde_json::Value {
    for msg in &request.messages {
        for part in &msg.parts {
            match part {
                A2aPart::Data { data } => return data.clone(),
                A2aPart::Text { text } => {
                    return serde_json::json!({ "input": text });
                }
            }
        }
    }
    serde_json::json!({})
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(!card.skills.is_empty(), "expected skills from built-in plugins");
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

    #[test]
    fn extract_tool_args_from_data_part() {
        let req = MessageSendRequest {
            messages: vec![A2aMessage {
                role: "user".to_string(),
                parts: vec![A2aPart::Data {
                    data: serde_json::json!({"expression": "2+2"}),
                }],
            }],
            tool: Some("calculator".to_string()),
        };
        let args = extract_tool_args(&req);
        assert_eq!(args["expression"], "2+2");
    }

    #[test]
    fn extract_tool_args_from_text_part() {
        let req = MessageSendRequest {
            messages: vec![A2aMessage {
                role: "user".to_string(),
                parts: vec![A2aPart::Text {
                    text: "2+2".to_string(),
                }],
            }],
            tool: Some("calculator".to_string()),
        };
        let args = extract_tool_args(&req);
        assert_eq!(args["input"], "2+2");
    }
}
