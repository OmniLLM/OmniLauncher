use serde::{Deserialize, Serialize};

// ── Agent Card ──────────────────────────────────────────────────────────────

/// Top-level Agent Card returned by `GET /.well-known/agent.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub url: String,
    /// Protocol version string, e.g. "0.2.1".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub capabilities: AgentCapabilities,
    pub authentication: AgentAuthentication,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<AgentSkill>,
    /// Default input modes accepted by this agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_input_modes: Vec<String>,
    /// Default output modes produced by this agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_output_modes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    pub streaming: bool,
    pub push_notifications: bool,
    /// Indicates whether task state is persisted across restarts.
    #[serde(default)]
    pub state_transition_history: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAuthentication {
    /// Supported auth scheme(s), e.g. `["bearer"]`.
    pub schemes: Vec<String>,
}

/// A single capability/skill advertised in the Agent Card.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional JSON Schema for the skill parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

// ── Messages & Parts ────────────────────────────────────────────────────────

/// A message within an A2A conversation (request or response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aMessage {
    pub role: String,
    pub parts: Vec<A2aPart>,
}

/// A single part of a message — currently only text and structured data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum A2aPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "data")]
    Data { data: serde_json::Value },
}

/// An artifact produced by a completed task — a named collection of parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct A2aArtifact {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parts: Vec<A2aPart>,
    #[serde(default)]
    pub index: u32,
}

// ── Task ────────────────────────────────────────────────────────────────────

/// Task lifecycle states that map to the A2A protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum A2aTaskState {
    Submitted,
    Working,
    Completed,
    Failed,
    Canceled,
    Rejected,
}

impl A2aTaskState {
    /// Returns `true` for terminal states that will never transition further.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Canceled | Self::Rejected
        )
    }
}

/// The status block embedded in a task response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aTaskStatus {
    pub state: A2aTaskState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<A2aMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// An A2A task as returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct A2aTask {
    pub id: String,
    pub status: A2aTaskStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<A2aArtifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<A2aMessage>,
}

// ── Request / Response envelopes ────────────────────────────────────────────

// ── JSON-RPC 2.0 envelope ───────────────────────────────────────────────────

/// A JSON-RPC 2.0 request envelope.
///
/// `id` and `params` default to `Value::Null` when the field is absent, so
/// notifications and parameter-less requests both parse cleanly.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: serde_json::Value,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// A JSON-RPC 2.0 response envelope. Exactly one of `result` and `error` is
/// set on a well-formed response.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse<T: Serialize> {
    pub jsonrpc: &'static str,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcErrorObj>,
}

/// The JSON-RPC error object inside a `JsonRpcResponse`.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcErrorObj {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Params for `message/send`.
#[derive(Debug, Clone, Deserialize)]
pub struct SendMessageParams {
    pub message: A2aMessage,
    #[serde(default, rename = "contextId")]
    pub context_id: Option<String>,
    #[serde(default, rename = "skillId")]
    pub skill_id: Option<String>,
}

/// Params for `tasks/get` and `tasks/cancel`.
#[derive(Debug, Clone, Deserialize)]
pub struct TaskIdParams {
    pub id: String,
}

/// Body of `POST /message:send`.
#[derive(Debug, Clone, Deserialize)]
pub struct MessageSendRequest {
    pub messages: Vec<A2aMessage>,
    /// If present, names a specific tool/skill to invoke directly (bypassing
    /// conversational routing). The tool arguments are expected in the first
    /// message's data part.
    #[serde(default)]
    pub tool: Option<String>,
}

/// Wrapper for task-list responses.
#[derive(Debug, Clone, Serialize)]
pub struct TaskListResponse {
    pub tasks: Vec<A2aTask>,
}

// ── Errors ──────────────────────────────────────────────────────────────────

/// Standard A2A-compatible error payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl A2aError {
    // ── Standard error constructors ─────────────────────────────────────

    pub fn parse_error(detail: impl Into<String>) -> Self {
        Self {
            code: -32700,
            message: detail.into(),
            data: None,
        }
    }

    pub fn invalid_request(detail: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: detail.into(),
            data: None,
        }
    }

    pub fn method_not_found(detail: impl Into<String>) -> Self {
        Self {
            code: -32601,
            message: detail.into(),
            data: None,
        }
    }

    pub fn invalid_params(detail: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: detail.into(),
            data: None,
        }
    }

    pub fn internal_error(detail: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: detail.into(),
            data: None,
        }
    }

    // ── A2A-specific errors ─────────────────────────────────────────────

    pub fn task_not_found(task_id: &str) -> Self {
        Self {
            code: -32001,
            message: format!("Task not found: {task_id}"),
            data: None,
        }
    }

    pub fn unsupported_operation(detail: impl Into<String>) -> Self {
        Self {
            code: -32004,
            message: detail.into(),
            data: None,
        }
    }

    pub fn push_notification_not_supported() -> Self {
        Self {
            code: -32005,
            message: "Push notifications are not supported".to_string(),
            data: None,
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_state_serializes_to_lowercase() {
        assert_eq!(
            serde_json::to_string(&A2aTaskState::Submitted).unwrap(),
            "\"submitted\""
        );
        assert_eq!(
            serde_json::to_string(&A2aTaskState::Working).unwrap(),
            "\"working\""
        );
        assert_eq!(
            serde_json::to_string(&A2aTaskState::Completed).unwrap(),
            "\"completed\""
        );
        assert_eq!(
            serde_json::to_string(&A2aTaskState::Failed).unwrap(),
            "\"failed\""
        );
        assert_eq!(
            serde_json::to_string(&A2aTaskState::Canceled).unwrap(),
            "\"canceled\""
        );
        assert_eq!(
            serde_json::to_string(&A2aTaskState::Rejected).unwrap(),
            "\"rejected\""
        );
    }

    #[test]
    fn task_state_roundtrips() {
        for state in [
            A2aTaskState::Submitted,
            A2aTaskState::Working,
            A2aTaskState::Completed,
            A2aTaskState::Failed,
            A2aTaskState::Canceled,
            A2aTaskState::Rejected,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let back: A2aTaskState = serde_json::from_str(&json).unwrap();
            assert_eq!(back, state);
        }
    }

    #[test]
    fn terminal_states_are_correct() {
        assert!(!A2aTaskState::Submitted.is_terminal());
        assert!(!A2aTaskState::Working.is_terminal());
        assert!(A2aTaskState::Completed.is_terminal());
        assert!(A2aTaskState::Failed.is_terminal());
        assert!(A2aTaskState::Canceled.is_terminal());
        assert!(A2aTaskState::Rejected.is_terminal());
    }

    #[test]
    fn agent_card_roundtrip() {
        let card = AgentCard {
            name: "OmniLauncher".to_string(),
            description: "Desktop agent".to_string(),
            url: "http://127.0.0.1:1423".to_string(),
            version: Some("0.1.0".to_string()),
            capabilities: AgentCapabilities {
                streaming: false,
                push_notifications: false,
                state_transition_history: false,
            },
            authentication: AgentAuthentication {
                schemes: vec!["bearer".to_string()],
            },
            skills: vec![AgentSkill {
                id: "calculator".to_string(),
                name: "Calculator".to_string(),
                description: Some("Evaluate math expressions".to_string()),
                input_schema: None,
                tags: vec!["math".to_string()],
            }],
            default_input_modes: vec!["text/plain".to_string()],
            default_output_modes: vec!["text/plain".to_string()],
        };

        let json = serde_json::to_string_pretty(&card).unwrap();
        let back: AgentCard = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "OmniLauncher");
        assert_eq!(back.capabilities.streaming, false);
        assert_eq!(back.authentication.schemes, vec!["bearer"]);
        assert_eq!(back.skills.len(), 1);
        assert_eq!(back.skills[0].id, "calculator");
    }

    #[test]
    fn a2a_error_serialization() {
        let err = A2aError::unsupported_operation("Streaming is not supported in this version");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("-32004"));
        assert!(json.contains("Streaming is not supported"));

        let err2 = A2aError::task_not_found("abc-123");
        let json2 = serde_json::to_string(&err2).unwrap();
        assert!(json2.contains("-32001"));
        assert!(json2.contains("abc-123"));
    }

    #[test]
    fn a2a_task_roundtrip() {
        let task = A2aTask {
            id: "task-001".to_string(),
            status: A2aTaskStatus {
                state: A2aTaskState::Completed,
                message: Some(A2aMessage {
                    role: "agent".to_string(),
                    parts: vec![A2aPart::Text {
                        text: "Done!".to_string(),
                    }],
                }),
                timestamp: Some("2026-06-25T12:00:00Z".to_string()),
            },
            artifacts: vec![A2aArtifact {
                name: Some("result".to_string()),
                description: None,
                parts: vec![A2aPart::Data {
                    data: serde_json::json!({"answer": 42}),
                }],
                index: 0,
            }],
            history: vec![],
        };

        let json = serde_json::to_string_pretty(&task).unwrap();
        let back: A2aTask = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "task-001");
        assert_eq!(back.status.state, A2aTaskState::Completed);
        assert_eq!(back.artifacts.len(), 1);
    }

    #[test]
    fn message_send_request_with_tool() {
        let json = r#"{
            "messages": [{"role": "user", "parts": [{"type": "text", "text": "hello"}]}],
            "tool": "calculator"
        }"#;
        let req: MessageSendRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.tool, Some("calculator".to_string()));
        assert_eq!(req.messages.len(), 1);
    }

    #[test]
    fn message_send_request_without_tool() {
        let json = r#"{
            "messages": [{"role": "user", "parts": [{"type": "text", "text": "what time is it?"}]}]
        }"#;
        let req: MessageSendRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.tool, None);
    }

    #[test]
    fn part_text_serde() {
        let part = A2aPart::Text {
            text: "hello".to_string(),
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        let back: A2aPart = serde_json::from_str(&json).unwrap();
        match back {
            A2aPart::Text { text } => assert_eq!(text, "hello"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn part_data_serde() {
        let part = A2aPart::Data {
            data: serde_json::json!({"key": "value"}),
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"data\""));
        let back: A2aPart = serde_json::from_str(&json).unwrap();
        match back {
            A2aPart::Data { data } => assert_eq!(data["key"], "value"),
            _ => panic!("expected Data"),
        }
    }

    #[test]
    fn jsonrpc_request_deserializes_full_envelope() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 42,
            "method": "message/send",
            "params": {"skillId":"skill:alibaba"}
        }"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "message/send");
        assert!(req.id.is_number());
        assert_eq!(req.params["skillId"], "skill:alibaba");
    }

    #[test]
    fn jsonrpc_request_defaults_missing_id_and_params_to_null() {
        let json = r#"{"jsonrpc":"2.0","method":"tasks/get"}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert!(req.id.is_null());
        assert!(req.params.is_null());
    }

    #[test]
    fn jsonrpc_response_success_serializes_without_error_field() {
        let resp = JsonRpcResponse::<serde_json::Value> {
            jsonrpc: "2.0",
            id: serde_json::json!(1),
            result: Some(serde_json::json!({"ok": true})),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\":{\"ok\":true}"));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn jsonrpc_response_error_serializes_without_result_field() {
        let resp = JsonRpcResponse::<serde_json::Value> {
            jsonrpc: "2.0",
            id: serde_json::json!(1),
            result: None,
            error: Some(JsonRpcErrorObj {
                code: -32601,
                message: "Method not found".to_string(),
                data: None,
            }),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
        assert!(json.contains("-32601"));
        assert!(!json.contains("\"result\""));
    }

    #[test]
    fn send_message_params_accepts_snake_and_camel_names() {
        let json = r#"{
            "message": {"role":"user","parts":[{"type":"text","text":"hi"}]},
            "contextId": "ctx-1",
            "skillId":   "skill:alibaba"
        }"#;
        let params: SendMessageParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.context_id.as_deref(), Some("ctx-1"));
        assert_eq!(params.skill_id.as_deref(), Some("skill:alibaba"));
        assert_eq!(params.message.role, "user");
    }

    #[test]
    fn task_id_params_deserializes_id_field() {
        let json = r#"{"id":"task-xyz"}"#;
        let params: TaskIdParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.id, "task-xyz");
    }
}
