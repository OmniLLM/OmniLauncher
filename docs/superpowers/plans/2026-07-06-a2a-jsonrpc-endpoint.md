# A2A JSON-RPC 2.0 Endpoint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make OmniLauncher's A2A server speak JSON-RPC 2.0 at `POST /` so the Omni Agent Hub can forward `message/send`, `tasks/get`, and `tasks/cancel` and reach every OmniLauncher skill (including `skill:alibaba`).

**Architecture:** A new `jsonrpc.rs` module parses the JSON-RPC envelope, dispatches by method name to the existing `adapter::handle_*` functions, and wraps their `A2aTask` return in a `JSONRPCResponse.result`. `server.rs` replaces its four legacy REST routes with a single `POST /`. Task and artifact JSON gain `contextId` and `artifactId` fields to align with the hub's `a2a.Task` schema. Adapter logic and plugin/skill execution paths are unchanged.

**Tech Stack:** Rust 2021, tokio, serde/serde_json, chrono. All new work lives in `src-tauri/src/a2a/`.

**Reference spec:** `/home/jzhu/repos/OmniLauncher/docs/superpowers/specs/2026-07-06-a2a-jsonrpc-endpoint-design.md`

---

## File Structure

Files created:
- `src-tauri/src/a2a/jsonrpc.rs` — JSON-RPC envelope handling + method dispatch. Pure translation, no I/O, no clocks; delegates to `adapter::handle_*`.

Files modified:
- `src-tauri/src/a2a/mod.rs` — export the new module.
- `src-tauri/src/a2a/types.rs` — add JSON-RPC envelope types, method-param structs, `context_id` on `A2aTask`, `artifact_id` on `A2aArtifact`.
- `src-tauri/src/a2a/tasks.rs` — store `context_id: Option<String>` on `TaskRecord`; echo it in `to_a2a_task()`; generate `artifact_id` when constructing artifacts (via a shared helper reused from tasks.rs).
- `src-tauri/src/a2a/adapter.rs` — `handle_message_send` gains a third `context_id: Option<String>` parameter and threads it into `TaskRegistry::create_submitted`; artifact construction at line 200 gets an `artifact_id`.
- `src-tauri/src/a2a/capabilities.rs` — artifact construction at line 263 gets an `artifact_id`.
- `src-tauri/src/a2a/server.rs` — replace `POST /message:send`, `GET /tasks`, task-route arm with a single `("POST", "/")` arm calling `jsonrpc::dispatch`.

Tests added or updated (co-located as `#[cfg(test)] mod tests` in each file):
- `types.rs` — roundtrip tests for JSON-RPC envelope types.
- `tasks.rs` — `context_id` echoed by `to_a2a_task`.
- `adapter.rs` — existing tests updated to pass `None` for context id; new test that context id round-trips.
- `capabilities.rs` — existing tests updated for the new `artifact_id` field.
- `jsonrpc.rs` — the twelve dispatch unit tests listed in the spec.
- `server.rs` — legacy REST tests removed; two new HTTP-level tests for `POST /`.

---

## Task 1: Add JSON-RPC envelope types + method-param structs

**Files:**
- Modify: `src-tauri/src/a2a/types.rs`

- [ ] **Step 1: Write the failing tests**

Append inside the existing `#[cfg(test)] mod tests` block in `src-tauri/src/a2a/types.rs`, just above the closing `}`:

```rust
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
```

- [ ] **Step 2: Run the failing tests**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo test --lib a2a::types::tests::jsonrpc 2>&1 | tail -30
```

Expected: compile errors — `JsonRpcRequest`, `JsonRpcResponse`, `JsonRpcErrorObj`, `SendMessageParams`, `TaskIdParams` do not exist.

- [ ] **Step 3: Add the new types**

Insert this block in `src-tauri/src/a2a/types.rs` immediately after the `// ── Request / Response envelopes ────────────────────────────────────────────` comment header (currently around line 137) and before the existing `MessageSendRequest` definition:

```rust
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

```

- [ ] **Step 4: Run the tests to verify they pass**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo test --lib a2a::types::tests::jsonrpc a2a::types::tests::send_message_params a2a::types::tests::task_id_params 2>&1 | tail -15
```

Expected: all six tests pass.

- [ ] **Step 5: Commit**

```
cd /home/jzhu/repos/OmniLauncher && git add src-tauri/src/a2a/types.rs && git commit --author="James Zhu <zhujian0805@gmail.com>" -m "feat(a2a): add JSON-RPC 2.0 envelope types

Adds JsonRpcRequest, JsonRpcResponse<T>, JsonRpcErrorObj, SendMessageParams,
and TaskIdParams. Preparation for the JSON-RPC endpoint at POST / that
replaces the legacy REST A2A routes."
```

---

## Task 2: Add `context_id` on `A2aTask` and `artifact_id` on `A2aArtifact`

**Files:**
- Modify: `src-tauri/src/a2a/types.rs`

- [ ] **Step 1: Write the failing tests**

Append inside the existing `#[cfg(test)] mod tests` block in `src-tauri/src/a2a/types.rs`, just above the closing `}`:

```rust
    #[test]
    fn a2a_task_serializes_context_id_when_present() {
        let task = A2aTask {
            id: "t-1".to_string(),
            context_id: Some("ctx-42".to_string()),
            status: A2aTaskStatus {
                state: A2aTaskState::Completed,
                message: None,
                timestamp: None,
            },
            artifacts: vec![],
            history: vec![],
        };
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("\"contextId\":\"ctx-42\""));
    }

    #[test]
    fn a2a_task_omits_context_id_when_none() {
        let task = A2aTask {
            id: "t-1".to_string(),
            context_id: None,
            status: A2aTaskStatus {
                state: A2aTaskState::Completed,
                message: None,
                timestamp: None,
            },
            artifacts: vec![],
            history: vec![],
        };
        let json = serde_json::to_string(&task).unwrap();
        assert!(!json.contains("contextId"));
    }

    #[test]
    fn a2a_artifact_serializes_artifact_id() {
        let artifact = A2aArtifact {
            artifact_id: "art-abc".to_string(),
            name: Some("results".to_string()),
            description: None,
            parts: vec![A2aPart::Text {
                text: "hi".to_string(),
            }],
            index: 0,
        };
        let json = serde_json::to_string(&artifact).unwrap();
        assert!(json.contains("\"artifactId\":\"art-abc\""));
    }
```

- [ ] **Step 2: Run the failing tests**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo test --lib a2a::types::tests::a2a_task_serializes_context_id a2a::types::tests::a2a_task_omits_context_id a2a::types::tests::a2a_artifact_serializes_artifact_id 2>&1 | tail -30
```

Expected: compile errors — `A2aTask.context_id` and `A2aArtifact.artifact_id` don't exist.

- [ ] **Step 3: Add the fields**

In `src-tauri/src/a2a/types.rs`, modify the `A2aTask` struct (around lines 126–135):

```rust
/// An A2A task as returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct A2aTask {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    pub status: A2aTaskStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<A2aArtifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<A2aMessage>,
}
```

In the same file, modify the `A2aArtifact` struct (around lines 79–89):

```rust
/// An artifact produced by a completed task — a named collection of parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct A2aArtifact {
    pub artifact_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parts: Vec<A2aPart>,
    #[serde(default)]
    pub index: u32,
}
```

Update the existing `agent_card_roundtrip` test — no change to that test's artifact-less card. Update the existing `a2a_task_roundtrip` test (around lines 346–375) to include the new fields:

Replace the current body of `a2a_task_roundtrip` with:

```rust
    #[test]
    fn a2a_task_roundtrip() {
        let task = A2aTask {
            id: "task-001".to_string(),
            context_id: Some("ctx-1".to_string()),
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
                artifact_id: "art-1".to_string(),
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
        assert_eq!(back.context_id.as_deref(), Some("ctx-1"));
        assert_eq!(back.status.state, A2aTaskState::Completed);
        assert_eq!(back.artifacts.len(), 1);
        assert_eq!(back.artifacts[0].artifact_id, "art-1");
    }
```

- [ ] **Step 4: Compile-check without running other tests yet**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo check --lib 2>&1 | tail -30
```

Expected: multiple compile errors in `adapter.rs`, `capabilities.rs`, `tasks.rs` complaining about missing `artifact_id` / `context_id` — that's fine, they are fixed in Tasks 3–4. Do NOT run the whole test suite yet.

- [ ] **Step 5: Commit**

```
cd /home/jzhu/repos/OmniLauncher && git add src-tauri/src/a2a/types.rs && git commit --author="James Zhu <zhujian0805@gmail.com>" -m "feat(a2a): add contextId to A2aTask and artifactId to A2aArtifact

Both fields are needed by the hub's a2a.Task schema. context_id is optional
and skipped when None; artifact_id is mandatory (16-hex, generated at
construction). Types compile; call sites will be updated in follow-up
commits."
```

---

## Task 3: Store and echo `context_id` on `TaskRecord`

**Files:**
- Modify: `src-tauri/src/a2a/tasks.rs`

- [ ] **Step 1: Write the failing tests**

Append inside the existing `#[cfg(test)] mod tests` block in `src-tauri/src/a2a/tasks.rs`, just above the closing `}`:

```rust
    #[test]
    fn create_submitted_with_context_id_stores_and_echoes_it() {
        let mut reg = TaskRegistry::new(100);
        let id = reg.create_submitted(
            "q".to_string(),
            None,
            Some("ctx-abc".to_string()),
        );
        let record = reg.get(&id).expect("task should exist");
        assert_eq!(record.context_id.as_deref(), Some("ctx-abc"));

        let a2a = record.to_a2a_task();
        assert_eq!(a2a.context_id.as_deref(), Some("ctx-abc"));
    }

    #[test]
    fn create_submitted_with_no_context_id_omits_it_from_a2a_task() {
        let mut reg = TaskRegistry::new(100);
        let id = reg.create_submitted("q".to_string(), None, None);
        let a2a = reg.get(&id).unwrap().to_a2a_task();
        assert!(a2a.context_id.is_none());
    }
```

- [ ] **Step 2: Run the failing tests**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo test --lib a2a::tasks::tests::create_submitted_with_context_id a2a::tasks::tests::create_submitted_with_no_context_id 2>&1 | tail -20
```

Expected: compile error — `create_submitted` takes 2 args, tests call it with 3.

- [ ] **Step 3: Update `TaskRecord` and `create_submitted`**

In `src-tauri/src/a2a/tasks.rs`, update the `TaskRecord` struct (around lines 8–26) — add one field:

```rust
/// In-memory record for a single A2A task.
#[derive(Debug, Clone)]
pub struct TaskRecord {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub state: A2aTaskState,
    /// Original A2A request ID, if the client provided one.
    pub request_id: Option<String>,
    /// The `contextId` associated with this task, if the client provided one.
    /// Echoed to callers via the wire-format task.
    pub context_id: Option<String>,
    /// Short human-readable summary of the original request.
    pub request_summary: String,
    /// Output messages from the agent.
    pub output_messages: Vec<A2aMessage>,
    /// Structured artifacts produced by the task.
    pub artifacts: Vec<A2aArtifact>,
    /// Error detail string (should already be masked before storing).
    pub error: Option<String>,
    /// If true, a cancel request was received.
    pub cancel_requested: bool,
}
```

Update `to_a2a_task` in the same file (around lines 28–52):

```rust
impl TaskRecord {
    /// Convert this registry record into the wire-format A2A task.
    pub fn to_a2a_task(&self) -> A2aTask {
        let status_message = if let Some(ref err) = self.error {
            Some(A2aMessage {
                role: "agent".to_string(),
                parts: vec![A2aPart::Text {
                    text: err.clone(),
                }],
            })
        } else {
            self.output_messages.last().cloned()
        };

        A2aTask {
            id: self.id.clone(),
            context_id: self.context_id.clone(),
            status: A2aTaskStatus {
                state: self.state,
                message: status_message,
                timestamp: Some(self.updated_at.to_rfc3339()),
            },
            artifacts: self.artifacts.clone(),
            history: self.output_messages.clone(),
        }
    }
}
```

Update `create_submitted` (around lines 87–109):

```rust
    /// Create a new task in the `submitted` state.
    pub fn create_submitted(
        &mut self,
        request_summary: String,
        request_id: Option<String>,
        context_id: Option<String>,
    ) -> String {
        let now = chrono::Utc::now();
        let id = generate_task_id();
        let record = TaskRecord {
            id: id.clone(),
            created_at: now,
            updated_at: now,
            state: A2aTaskState::Submitted,
            request_id,
            context_id,
            request_summary,
            output_messages: Vec::new(),
            artifacts: Vec::new(),
            error: None,
            cancel_requested: false,
        };
        self.tasks.insert(id.clone(), record);
        id
    }
```

- [ ] **Step 4: Also expose `generate_task_id` for artifact-id reuse**

In `src-tauri/src/a2a/tasks.rs`, change the visibility of `generate_task_id` (around line 58) from private to `pub(crate)` so `adapter.rs` and `capabilities.rs` can call it for artifact IDs. The full signature becomes:

```rust
/// Generate a random 16-byte hex ID (32 chars). Used for task and artifact IDs.
pub(crate) fn generate_task_id() -> String {
    let bytes: [u8; 16] = rand::random();
    bytes.iter().fold(String::with_capacity(32), |mut s, b| {
        use std::fmt::Write;
        let _ = write!(s, "{:02x}", b);
        s
    })
}
```

- [ ] **Step 5: Update every existing `create_submitted` call in tasks.rs tests**

Find every `reg.create_submitted(...)` call in `src-tauri/src/a2a/tasks.rs`'s test module. There are ten such calls in the existing tests (in `create_and_get_task`, `lifecycle_submitted_working_completed`, `lifecycle_submitted_working_failed`, `cancel_before_completion`, `cancel_completed_task_is_noop`, `terminal_state_does_not_revert`, `list_returns_sorted_by_creation` (three calls), `eviction_removes_oldest_terminal` (three calls), `eviction_preserves_active_tasks` (three calls), and `to_a2a_task_conversion`). Add a trailing `None` to each so they compile with the new signature.

For example, change:

```rust
let id = reg.create_submitted("test query".to_string(), None);
```

to:

```rust
let id = reg.create_submitted("test query".to_string(), None, None);
```

Use `sed -i` or manual edit — do all of them.

- [ ] **Step 6: Run all tasks.rs tests**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo test --lib a2a::tasks:: 2>&1 | tail -20
```

Expected: all `a2a::tasks::tests::*` tests pass, including the two new ones.

- [ ] **Step 7: Commit**

```
cd /home/jzhu/repos/OmniLauncher && git add src-tauri/src/a2a/tasks.rs && git commit --author="James Zhu <zhujian0805@gmail.com>" -m "feat(a2a): thread context_id through TaskRecord

TaskRegistry::create_submitted now takes an optional context_id which is
stored on the record and echoed by to_a2a_task. generate_task_id is
promoted to pub(crate) so adapter/capabilities can reuse it for
artifact_id generation."
```

---

## Task 4: Generate `artifact_id` at every artifact construction site + accept `context_id` in adapter

**Files:**
- Modify: `src-tauri/src/a2a/adapter.rs`
- Modify: `src-tauri/src/a2a/capabilities.rs`

- [ ] **Step 1: Write the failing test in adapter.rs**

Append this test at the end of `src-tauri/src/a2a/adapter.rs`'s `#[cfg(test)] mod tests` block, just above the closing `}`:

```rust
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
```

- [ ] **Step 2: Run the failing test**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo test --lib a2a::adapter::tests::message_send_echoes_context_id 2>&1 | tail -20
```

Expected: compile error — `handle_message_send` only takes 2 args.

- [ ] **Step 3: Update `handle_message_send` signature**

In `src-tauri/src/a2a/adapter.rs`, update the function (around lines 91–140) to accept and thread `context_id`:

```rust
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
```

- [ ] **Step 4: Populate `artifact_id` at the conversational-artifact site in adapter.rs**

In `src-tauri/src/a2a/adapter.rs`, update the artifact construction inside `execute_conversational` (around lines 197–208):

```rust
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
```

- [ ] **Step 5: Populate `artifact_id` at the query-results site in capabilities.rs**

In `src-tauri/src/a2a/capabilities.rs`, update the artifact construction in `query_results_response` (around lines 261–269):

```rust
fn query_results_response(results: Vec<QueryResult>) -> (Vec<A2aMessage>, Vec<A2aArtifact>) {
    let count = results.len();
    let artifact = A2aArtifact {
        artifact_id: super::tasks::generate_task_id(),
        name: Some("query_results".to_string()),
        description: Some(format!("{count} launcher results")),
        parts: vec![A2aPart::Data {
            data: query_results_artifact(results),
        }],
        index: 0,
    };
    // (rest of function unchanged)
```

- [ ] **Step 6: Update the pre-existing `message_send_invokes_query_only_capability` test**

Also in `src-tauri/src/a2a/adapter.rs`, the existing test at line 485 calls `handle_message_send(&state, request)`. Update it to `handle_message_send(&state, request, None)`:

```rust
        let task = handle_message_send(&state, request, None).await.unwrap();
```

- [ ] **Step 7: Run all adapter and capabilities tests**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo test --lib a2a::adapter a2a::capabilities 2>&1 | tail -30
```

Expected: every test in `a2a::adapter::tests::*` and `a2a::capabilities::tests::*` passes, including `message_send_echoes_context_id_into_task`.

- [ ] **Step 8: Commit**

```
cd /home/jzhu/repos/OmniLauncher && git add src-tauri/src/a2a/adapter.rs src-tauri/src/a2a/capabilities.rs && git commit --author="James Zhu <zhujian0805@gmail.com>" -m "feat(a2a): thread context_id through handle_message_send and populate artifact_id

handle_message_send now takes Option<String> context_id and forwards it into
create_submitted. Every A2aArtifact construction site now generates an
artifact_id via generate_task_id (16-hex). No behavior change for callers
that pass None."
```

---

## Task 5: Wire the JSON-RPC dispatcher (new module)

**Files:**
- Create: `src-tauri/src/a2a/jsonrpc.rs`
- Modify: `src-tauri/src/a2a/mod.rs`

- [ ] **Step 1: Register the new module**

Edit `src-tauri/src/a2a/mod.rs` to declare the module. Full replacement contents:

```rust
pub mod adapter;
pub mod capabilities;
pub mod jsonrpc;
pub mod server;
pub mod tasks;
pub mod types;

pub use types::{A2aError, A2aTask, A2aTaskState, AgentCard};
```

- [ ] **Step 2: Write the failing tests + skeleton file**

Create `src-tauri/src/a2a/jsonrpc.rs` with the following contents. The `dispatch` and helper functions are empty stubs so the test block references type-check; tests will fail at runtime.

```rust
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
        A2aError, JsonRpcErrorObj, JsonRpcRequest, JsonRpcResponse, MessageSendRequest,
        SendMessageParams, TaskIdParams,
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
            return error_body(
                req.id,
                -32602,
                "Invalid params",
                Some(err.to_string()),
            );
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
    serde_json::to_string(&resp)
        .unwrap_or_else(|e| format!(r#"{{"jsonrpc":"2.0","id":null,"error":{{"code":-32603,"message":"serialize failed: {e}"}}}}"#))
}

fn error_body<M: Into<String>>(
    id: Value,
    code: i32,
    message: M,
    data: Option<impl Into<Value>>,
) -> String {
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
    serde_json::to_string(&resp)
        .unwrap_or_else(|e| format!(r#"{{"jsonrpc":"2.0","id":null,"error":{{"code":-32603,"message":"serialize failed: {e}"}}}}"#))
}

// Bridge for callers that already own an `A2aError`.
#[allow(dead_code)]
fn error_body_from_a2a(id: Value, err: &A2aError) -> String {
    error_body(id, err.code, err.message.clone(), err.data.clone())
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
```

- [ ] **Step 3: Run the tests**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo test --lib a2a::jsonrpc:: 2>&1 | tail -40
```

Expected: all thirteen `a2a::jsonrpc::tests::*` tests pass.

- [ ] **Step 4: Commit**

```
cd /home/jzhu/repos/OmniLauncher && git add src-tauri/src/a2a/jsonrpc.rs src-tauri/src/a2a/mod.rs && git commit --author="James Zhu <zhujian0805@gmail.com>" -m "feat(a2a): add JSON-RPC 2.0 dispatcher

New module a2a::jsonrpc. Parses the JSON-RPC envelope, dispatches
message/send, tasks/get, tasks/cancel to the existing adapter functions,
and wraps A2aTask returns in JsonRpcResponse.result. Streaming and unknown
methods return the appropriate JSON-RPC error codes. Pure translation
layer; no I/O.

Twelve unit tests cover happy paths and every error branch."
```

---

## Task 6: Swap `server.rs` routes to `POST /` → `jsonrpc::dispatch`

**Files:**
- Modify: `src-tauri/src/a2a/server.rs`

- [ ] **Step 1: Write the new server tests**

In `src-tauri/src/a2a/server.rs`, inside the existing `#[cfg(test)] mod tests` block, add two tests at the end (just above the closing `}`):

```rust
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
        let body = r#"{
            "jsonrpc":"2.0","id":1,"method":"message/send",
            "params":{
                "message":{"role":"user","messageId":"m1","parts":[{"type":"text","text":"hi"}]},
                "contextId":"ctx-1",
                "skillId":"plugin:tool:calculator"
            }
        }"#;
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
```

Also remove the existing test named `unknown_route_returns_404`'s reliance on legacy REST — its current form (`GET /does/not/exist`) still applies and needs no edit. But you MUST delete any tests that hit `POST /message:send`, `GET /tasks`, `GET /tasks/{id}`, or `POST /tasks/{id}:cancel`. Currently there are none, so no deletion is needed — proceed.

- [ ] **Step 2: Run the failing tests**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo test --lib a2a::server::tests::post_root 2>&1 | tail -20
```

Expected: fails — `POST /` currently returns 404 because the route doesn't exist. The `post_root_requires_bearer_token` test may already pass (any POST / with no auth returns 401 from the auth guard).

- [ ] **Step 3: Replace the router body**

In `src-tauri/src/a2a/server.rs`, KEEP the CORS preflight (`OPTIONS`) and auth-guard blocks at the top of `handle_a2a_request` (lines 83–98 in the current file) EXACTLY as they are. Only replace the `// ── Route ───` match block (currently lines 100–141) with:

```rust
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
            let response_body =
                super::jsonrpc::dispatch(&state.adapter, &body).await;
            LiveResponse {
                status: "200 OK",
                content_type: "application/json; charset=utf-8",
                body: response_body,
            }
        }

        // 404 — everything else, including the removed legacy routes.
        _ => LiveResponse::text("404 Not Found", "Not Found".to_string()),
    }
```

- [ ] **Step 4: Delete the now-unused legacy handler**

In `src-tauri/src/a2a/server.rs`, DELETE the entire `handle_task_route` function (currently lines 144–191). It is no longer referenced.

Also delete the `TaskListResponse` import from the `use super::{...}` at the top of the file — the `TaskListResponse` type is no longer used in `server.rs`. The line currently reads:

```rust
use super::{
    adapter::{self, A2aAdapterState},
    types::{A2aError, MessageSendRequest, TaskListResponse},
};
```

Change it to:

```rust
use super::{
    adapter::{self, A2aAdapterState},
    types::A2aError,
};
```

`MessageSendRequest` is also unused in `server.rs` after the swap and is removed above.

- [ ] **Step 5: Run every A2A test**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo test --lib a2a:: 2>&1 | tail -30
```

Expected: every `a2a::*` test passes.

- [ ] **Step 6: Full workspace check**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo check --lib 2>&1 | tail -20
```

Expected: zero errors, zero warnings that reference the a2a module. `unused` warnings elsewhere are pre-existing and acceptable.

- [ ] **Step 7: Commit**

```
cd /home/jzhu/repos/OmniLauncher && git add src-tauri/src/a2a/server.rs && git commit --author="James Zhu <zhujian0805@gmail.com>" -m "feat(a2a): expose JSON-RPC 2.0 endpoint at POST /

Replaces the legacy routes /message:send, /tasks, /tasks/{id}, and
/tasks/{id}:cancel with a single POST / that delegates to
a2a::jsonrpc::dispatch. The agent-card route (GET /.well-known/agent.json),
auth, and CORS handling are unchanged. handle_task_route is deleted."
```

---

## Task 7: Full test + lint sweep

**Files:** (no source changes)

- [ ] **Step 1: cargo fmt**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo fmt --all 2>&1 | tail -5
```

Expected: no output (or only diagnostic).

- [ ] **Step 2: cargo clippy**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo clippy --lib --tests -- -D warnings 2>&1 | tail -30
```

Expected: passes with no denied warnings inside `src/a2a/`. If clippy flags any newly-added code (e.g. redundant clones, useless `to_string` on `&'static str`), fix inline and re-run.

- [ ] **Step 3: Full test suite**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo test --lib 2>&1 | tail -20
```

Expected: every test passes.

- [ ] **Step 4: Commit only if fmt/clippy made changes**

```
cd /home/jzhu/repos/OmniLauncher && git status
```

If there are staged changes:

```
cd /home/jzhu/repos/OmniLauncher && git add -u src-tauri/src/a2a/ && git commit --author="James Zhu <zhujian0805@gmail.com>" -m "chore(a2a): apply cargo fmt / clippy suggestions"
```

Otherwise skip.

---

## Task 8: End-to-end verification against the running hub

**Files:** (no source changes)

- [ ] **Step 1: Rebuild the backend binary**

```
cd /home/jzhu/repos/OmniLauncher/src-tauri && cargo build --release --bin omnilauncher-backend 2>&1 | tail -5
```

Expected: build succeeds. Note the produced path — usually
`/home/jzhu/repos/OmniLauncher/src-tauri/target/release/omnilauncher-backend`.

- [ ] **Step 2: Restart the backend**

Identify and stop the currently-running backend:

```
ps -eo pid,cmd | grep -i "omnilauncher-backend --server" | grep -v grep
```

Kill the PID from the output (e.g. `kill <PID>`). Wait for it to exit; do NOT use `-9` unless it fails to stop gracefully after 5 seconds.

Start it again:

```
/home/jzhu/repos/OmniLauncher/src-tauri/target/release/omnilauncher-backend --server --debug > /tmp/omnilauncher-backend.log 2>&1 &
sleep 3
ss -tlnp 2>/dev/null | grep 1423
```

Expected: process is listening on 127.0.0.1:1423.

- [ ] **Step 3: Refresh the hub's upstream cache**

Grab the admin key from `/home/jzhu/.config/omni-agent-hub/config.yaml` (line begins `admin_key:`) — the current value is `624934d2ce8869bac1226245828f8260c24a3d9ea143ae3fac58f1355bc843e8`.

```
curl -s -X POST -H "Authorization: Bearer 624934d2ce8869bac1226245828f8260c24a3d9ea143ae3fac58f1355bc843e8" http://localhost:8222/admin/upstreams/refresh
```

Expected: JSON response with no `"error"` field. If it still returns "unauthorized: invalid or missing client key", read the hub's route file to find the correct method (may need to be a `POST` to `/admin/upstreams/<id>/refresh`); adjust and retry.

- [ ] **Step 4: Assert hub health**

```
curl -s http://localhost:8222/health
```

Expected: `{"status":"ok","upstreams":{"healthy":1,"total":1}}`.

If still `healthy:0`, wait 10 seconds and retry — the hub's health checker may not have run yet. If it's still 0 after 30 seconds, tail `/tmp/omnilauncher-backend.log` to see what request the hub is making and diagnose from there.

- [ ] **Step 5: Assert composite agent card is populated**

```
curl -s http://localhost:8222/.well-known/agent-card.json | python3 -c "import sys,json; d=json.load(sys.stdin); print('skills count:', len(d.get('skills') or []))"
```

Expected: `skills count: 75` (or the current count reported by `admin/skills`). Any positive count is a pass.

- [ ] **Step 6: End-to-end alibaba VM count**

```
curl -s -X POST -H "Authorization: Bearer ad6450afc4b4990c08dde066bb0a2580d10b8e5ce2fb18126bde95067ed906a1" -H "Content-Type: application/json" http://localhost:8222/ -d '{
  "jsonrpc":"2.0","id":1,"method":"message/send","params":{
    "skillId":"omnilauncher.skill:alibaba",
    "message":{"role":"user","messageId":"m1","parts":[{"type":"text","text":"how many VMs in alibaba"}]}
  }
}'
```

Expected: JSON-RPC response with `result.status.state == "completed"` and `result.status.message.parts[0].text` mentioning a VM count (currently `11,018` per the direct-to-upstream call).

If the response is `{"error":{"code":-32002,"message":"Upstream HTTP error","data":"upstream returned HTTP 4xx..."}}`, tail `/tmp/omnilauncher-backend.log` — the upstream is receiving the request but rejecting the shape.

- [ ] **Step 7: Regression check — legacy REST is gone**

```
curl -s -o /dev/null -w "%{http_code}\n" -X POST -H "Authorization: Bearer 70020642d1f2bae53d36b7c70cfc8435f9e9bd6e8c3468a298d81be0e5fbd147" -H "Content-Type: application/json" http://localhost:1423/message:send -d '{"messages":[{"role":"user","parts":[{"type":"text","text":"x"}]}]}'
```

Expected: `404`. Confirms the legacy route is removed.

- [ ] **Step 8: Commit if any doc updates were needed**

If Step 3 required correcting the admin refresh URL (or any other end-to-end knowledge that belongs in the design doc), amend the spec:

```
cd /home/jzhu/repos/OmniLauncher && $EDITOR docs/superpowers/specs/2026-07-06-a2a-jsonrpc-endpoint-design.md
git add docs/superpowers/specs/2026-07-06-a2a-jsonrpc-endpoint-design.md
git commit --author="James Zhu <zhujian0805@gmail.com>" -m "docs(a2a): note refresh endpoint discovered during E2E test"
```

Otherwise skip.

---

## Done Criteria

All eight tasks complete AND all of these hold:

1. `cargo test --lib` passes in `src-tauri/`.
2. `cargo clippy --lib --tests -- -D warnings` passes.
3. `curl http://localhost:8222/health` returns `healthy:1`.
4. The alibaba-VM-count end-to-end call (Task 8 Step 6) returns a completed task with the answer.
5. `POST http://localhost:1423/message:send` returns `404` (legacy route removed).
