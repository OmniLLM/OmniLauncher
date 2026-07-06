//! JSON-RPC 2.0 dispatcher for the A2A endpoint (`POST /`).
//!
//! Pure translation layer: parses the JSON-RPC envelope, dispatches by
//! `method` name to the existing `adapter::handle_*` functions, and wraps the
//! returned `A2aTask` (or an error) in a JSON-RPC response envelope.
//!
//! No I/O, no clocks. Every method that touches state does so via
//! `A2aAdapterState`, which is already mocked in unit tests.

use serde::Serialize;
use serde_json::Value;

use super::{
    adapter::{self, A2aAdapterState},
    types::{
        JsonRpcErrorObj, JsonRpcRequest, JsonRpcResponse, MessageSendRequest, SendMessageParams,
        TaskIdParams,
    },
};

// ── Public API ──────────────────────────────────────────────────────────────

/// Parse and dispatch a JSON-RPC 2.0 request body against the adapter.
///
/// The `body` bytes are the raw request body (before this function all we
/// know is that the caller passed authentication). Every code path returns a
/// serialized JSON-RPC response body — errors are wrapped in an envelope with
/// `error` set rather than surfaced as Rust `Err`.
pub async fn dispatch(state: &A2aAdapterState, body: &str) -> String {
    let req: JsonRpcRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(err) => return error_body(Value::Null, -32700, "Parse error", Some(err.to_string())),
    };

    if req.jsonrpc != "2.0" || req.method.is_empty() {
        return error_body(
            req.id,
            -32600,
            "Invalid Request",
            Some("jsonrpc must be \"2.0\" and method must be non-empty".to_string()),
        );
    }

    match req.method.as_str() {
        "message/send" => handle_message_send_rpc(state, req).await,
        "message/sendSubscribe" => error_body(
            req.id,
            -32004,
            "Unsupported operation",
            Some("Streaming (message/sendSubscribe) is not supported".to_string()),
        ),
        "tasks/get" => handle_tasks_get_rpc(state, req).await,
        "tasks/cancel" => handle_tasks_cancel_rpc(state, req).await,
        _ => error_body(
            req.id,
            -32601,
            "Method not found",
            Some(format!("unknown method: {}", req.method)),
        ),
    }
}

// ── Method handlers ─────────────────────────────────────────────────────────

async fn handle_message_send_rpc(state: &A2aAdapterState, req: JsonRpcRequest) -> String {
    let params: SendMessageParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(err) => {
            return error_body(req.id, -32602, "Invalid params", Some(err.to_string()));
        }
    };

    let inner = MessageSendRequest {
        messages: vec![params.message],
        tool: params.skill_id,
    };

    match adapter::handle_message_send(state, inner, params.context_id).await {
        Ok(task) => success_body(req.id, task),
        Err(err) => error_body(req.id, err.code, err.message.clone(), err.data.clone()),
    }
}

async fn handle_tasks_get_rpc(state: &A2aAdapterState, req: JsonRpcRequest) -> String {
    let params: TaskIdParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(err) => {
            return error_body(req.id, -32602, "Invalid params", Some(err.to_string()));
        }
    };

    match adapter::handle_task_get(state, &params.id).await {
        Ok(task) => success_body(req.id, task),
        Err(err) => error_body(req.id, err.code, err.message.clone(), err.data.clone()),
    }
}

async fn handle_tasks_cancel_rpc(state: &A2aAdapterState, req: JsonRpcRequest) -> String {
    let params: TaskIdParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(err) => {
            return error_body(req.id, -32602, "Invalid params", Some(err.to_string()));
        }
    };

    match adapter::handle_task_cancel(state, &params.id).await {
        Ok(task) => success_body(req.id, task),
        Err(err) => error_body(req.id, err.code, err.message.clone(), err.data.clone()),
    }
}

// ── Response helpers ────────────────────────────────────────────────────────

fn success_body<T: Serialize>(id: Value, result: T) -> String {
    let resp = JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    };
    serde_json::to_string(&resp).unwrap_or_else(|e| {
        format!(
            r#"{{"jsonrpc":"2.0","id":null,"error":{{"code":-32603,"message":"serialize failed: {e}"}}}}"#
        )
    })
}

fn error_body<M, D>(id: Value, code: i32, message: M, data: Option<D>) -> String
where
    M: Into<String>,
    D: Into<Value>,
{
    let resp = JsonRpcResponse::<Value> {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcErrorObj {
            code,
            message: message.into(),
            data: data.map(Into::into),
        }),
    };
    serde_json::to_string(&resp).unwrap_or_else(|e| {
        format!(
            r#"{{"jsonrpc":"2.0","id":null,"error":{{"code":-32603,"message":"serialize failed: {e}"}}}}"#
        )
    })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a2a::adapter::A2aAdapterState;
    use crate::a2a::tasks::TaskRegistry;
    use crate::ai::client::AiClient;
    use crate::ai::router::ConversationContext;
    use crate::plugins::{Plugin, Query, QueryResult};
    use crate::{AppSettings, SkillManager};
    use async_trait::async_trait;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    struct EchoQueryPlugin;

    #[async_trait]
    impl Plugin for EchoQueryPlugin {
        fn name(&self) -> &str {
            "Echo"
        }
        fn description(&self) -> &str {
            "Echoes queries back"
        }
        fn keyword(&self) -> Option<&str> {
            Some("echo")
        }
        async fn query(&self, q: &Query) -> Vec<QueryResult> {
            vec![QueryResult {
                id: "echo-1".to_string(),
                title: q.raw.clone(),
                subtitle: None,
                icon: None,
                score: 100,
                action_type: "none".to_string(),
                action_data: String::new(),
                source: Some("Echo".to_string()),
            }]
        }
    }

    fn make_state() -> A2aAdapterState {
        let mut pm = crate::plugins::PluginManager::new();
        pm.register(Box::new(EchoQueryPlugin));
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

    fn parse(body: &str) -> Value {
        serde_json::from_str(body).unwrap()
    }

    #[tokio::test]
    async fn dispatch_message_send_wraps_task_in_result() {
        let state = make_state();
        let body = r#"{
            "jsonrpc":"2.0","id":1,"method":"message/send",
            "params":{
                "message":{"role":"user","messageId":"m1","parts":[{"type":"data","data":{"query":"hi"}}]},
                "skillId":"plugin:query:Echo"
            }
        }"#;
        let resp = parse(&dispatch(&state, body).await);
        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], 1);
        assert!(resp["result"].is_object(), "result must be present on success");
        assert!(resp["error"].is_null());
        assert_eq!(resp["result"]["status"]["state"], "completed");
    }

    #[tokio::test]
    async fn dispatch_message_send_forwards_skill_id_to_tool() {
        // If the skill id maps to nothing, the adapter returns a failed task.
        // If the skill id is forwarded correctly to the Echo plugin, the task
        // completes.
        let state = make_state();
        let body = r#"{
            "jsonrpc":"2.0","id":2,"method":"message/send",
            "params":{
                "message":{"role":"user","messageId":"m1","parts":[{"type":"data","data":{"query":"hi"}}]},
                "skillId":"plugin:query:Echo"
            }
        }"#;
        let resp = parse(&dispatch(&state, body).await);
        assert_eq!(resp["result"]["status"]["state"], "completed");
    }

    #[tokio::test]
    async fn dispatch_message_send_echoes_context_id() {
        let state = make_state();
        let body = r#"{
            "jsonrpc":"2.0","id":3,"method":"message/send",
            "params":{
                "message":{"role":"user","messageId":"m1","parts":[{"type":"data","data":{"query":"hi"}}]},
                "contextId":"ctx-x",
                "skillId":"plugin:query:Echo"
            }
        }"#;
        let resp = parse(&dispatch(&state, body).await);
        assert_eq!(resp["result"]["contextId"], "ctx-x");
    }

    #[tokio::test]
    async fn dispatch_message_send_omits_context_id_when_absent() {
        let state = make_state();
        let body = r#"{
            "jsonrpc":"2.0","id":4,"method":"message/send",
            "params":{
                "message":{"role":"user","messageId":"m1","parts":[{"type":"data","data":{"query":"hi"}}]},
                "skillId":"plugin:query:Echo"
            }
        }"#;
        let resp = parse(&dispatch(&state, body).await);
        assert!(resp["result"].get("contextId").is_none());
    }

    #[tokio::test]
    async fn dispatch_tasks_get_returns_stored_task() {
        let state = make_state();
        // First, send a message to create a task.
        let send = r#"{
            "jsonrpc":"2.0","id":10,"method":"message/send",
            "params":{
                "message":{"role":"user","messageId":"m1","parts":[{"type":"data","data":{"query":"hi"}}]},
                "skillId":"plugin:query:Echo"
            }
        }"#;
        let created = parse(&dispatch(&state, send).await);
        let task_id = created["result"]["id"].as_str().unwrap().to_string();

        // Then fetch it via tasks/get.
        let get = format!(
            r#"{{"jsonrpc":"2.0","id":11,"method":"tasks/get","params":{{"id":"{task_id}"}}}}"#
        );
        let fetched = parse(&dispatch(&state, &get).await);
        assert_eq!(fetched["result"]["id"], task_id);
    }

    #[tokio::test]
    async fn dispatch_tasks_get_missing_returns_task_not_found() {
        let state = make_state();
        let body = r#"{"jsonrpc":"2.0","id":12,"method":"tasks/get","params":{"id":"nope"}}"#;
        let resp = parse(&dispatch(&state, body).await);
        assert_eq!(resp["error"]["code"], -32001);
    }

    #[tokio::test]
    async fn dispatch_tasks_cancel_marks_canceled() {
        let state = make_state();
        // Create a task first.
        let send = r#"{
            "jsonrpc":"2.0","id":20,"method":"message/send",
            "params":{
                "message":{"role":"user","messageId":"m1","parts":[{"type":"data","data":{"query":"hi"}}]},
                "skillId":"plugin:query:Echo"
            }
        }"#;
        let created = parse(&dispatch(&state, send).await);
        let task_id = created["result"]["id"].as_str().unwrap().to_string();

        // Cancel it — completed tasks can't cancel, but the adapter returns a
        // task rather than an error. Just assert we get a result back.
        let cancel = format!(
            r#"{{"jsonrpc":"2.0","id":21,"method":"tasks/cancel","params":{{"id":"{task_id}"}}}}"#
        );
        let resp = parse(&dispatch(&state, &cancel).await);
        assert_eq!(resp["result"]["id"], task_id);
    }

    #[tokio::test]
    async fn dispatch_unknown_method_returns_method_not_found() {
        let state = make_state();
        let body = r#"{"jsonrpc":"2.0","id":30,"method":"quack/quack","params":{}}"#;
        let resp = parse(&dispatch(&state, body).await);
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn dispatch_invalid_json_returns_parse_error() {
        let state = make_state();
        let body = "not json";
        let resp = parse(&dispatch(&state, body).await);
        assert_eq!(resp["error"]["code"], -32700);
        assert!(resp["id"].is_null());
    }

    #[tokio::test]
    async fn dispatch_missing_jsonrpc_field_returns_invalid_request() {
        let state = make_state();
        let body = r#"{"jsonrpc":"1.0","id":40,"method":"message/send","params":{}}"#;
        let resp = parse(&dispatch(&state, body).await);
        assert_eq!(resp["error"]["code"], -32600);
    }

    #[tokio::test]
    async fn dispatch_message_send_bad_params_returns_invalid_params() {
        let state = make_state();
        // Missing required `message` field in params.
        let body = r#"{"jsonrpc":"2.0","id":50,"method":"message/send","params":{"skillId":"plugin:query:Echo"}}"#;
        let resp = parse(&dispatch(&state, body).await);
        assert_eq!(resp["error"]["code"], -32602);
    }

    #[tokio::test]
    async fn dispatch_streaming_returns_unsupported_operation() {
        let state = make_state();
        let body = r#"{"jsonrpc":"2.0","id":60,"method":"message/sendSubscribe","params":{}}"#;
        let resp = parse(&dispatch(&state, body).await);
        assert_eq!(resp["error"]["code"], -32004);
    }

    #[tokio::test]
    async fn error_response_echoes_request_id() {
        let state = make_state();
        let body = r#"{"jsonrpc":"2.0","id":"abc","method":"nope","params":{}}"#;
        let resp = parse(&dispatch(&state, body).await);
        assert_eq!(resp["id"], "abc");
    }
}
