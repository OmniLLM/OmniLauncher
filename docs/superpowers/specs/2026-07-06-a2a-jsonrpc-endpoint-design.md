# A2A JSON-RPC 2.0 Endpoint

**Date:** 2026-07-06
**Status:** Design

## Summary

Replace OmniLauncher's legacy REST A2A routes (`POST /message:send`, `GET /tasks`,
`GET /tasks/{id}`, `POST /tasks/{id}:cancel`) with a single JSON-RPC 2.0 endpoint
at `POST /`. This aligns OmniLauncher with the A2A spec used by the Omni Agent
Hub, which currently receives `HTTP 404` when it tries to forward requests.

The agent-card route (`GET /.well-known/agent.json`), auth, and CORS handling
stay unchanged.

## Motivation

The Omni Agent Hub aggregates upstream A2A agents and forwards client requests
as JSON-RPC 2.0:

```
POST http://<upstream>/
{"jsonrpc":"2.0","id":"...","method":"message/send",
 "params":{"message":{...},"contextId":"...","skillId":"skill:alibaba"}}
```

OmniLauncher today handles messages at `POST /message:send` with a custom
non-JSON-RPC body (`{messages:[...], tool}`), so every hub request returns 404
and the hub marks the upstream unhealthy. All 75 OmniLauncher-provided skills
are unreachable through the hub.

The hub is the only client of this endpoint today, so we replace rather than
dual-mount.

## Scope

### In scope

- JSON-RPC 2.0 dispatch for methods:
  - `message/send`
  - `tasks/get`
  - `tasks/cancel`
- Route `POST /` handles JSON-RPC; all legacy REST routes are removed.
- Task response shape gains `contextId` and per-artifact `artifactId` fields to
  match the hub's `a2a.Task` schema.

### Out of scope

- SSE streaming (`message/sendSubscribe`) — returns JSON-RPC error `-32004`
  ("Unsupported operation"), same behavior as today's `POST /message:stream`.
- Authentication: bearer token check is unchanged.
- Agent card contents and URL: unchanged; the hub tolerates the existing URL.
- Legacy REST compatibility: hub is the only client (confirmed), so we do NOT
  keep `POST /message:send` / `GET /tasks` alive.

## Architecture

### Files changed

| File | Change |
|---|---|
| `src-tauri/src/a2a/mod.rs` | Add `pub mod jsonrpc;` |
| `src-tauri/src/a2a/jsonrpc.rs` | New. Envelope types and `dispatch()`. |
| `src-tauri/src/a2a/server.rs` | Replace four legacy routes with one `POST /` that calls `jsonrpc::dispatch`. Delete legacy route tests. |
| `src-tauri/src/a2a/types.rs` | Add JSON-RPC envelope + method-param structs. Add `context_id: Option<String>` to `A2aTask`. Add `artifact_id: String` to `A2aArtifact`. |
| `src-tauri/src/a2a/adapter.rs` | Thread `context_id` from params into the created `TaskRecord`. Generate `artifactId` when creating artifacts. `handle_message_send` gains one new argument (see below); other public functions unchanged. |
| `src-tauri/src/a2a/tasks.rs` | Store `context_id: Option<String>` on `TaskRecord` and echo in `to_a2a_task()`. |

### Data flow — `message/send`

```
Client (hub) POST /
    body = {"jsonrpc":"2.0","id":..,"method":"message/send",
            "params":{"message":{...},"contextId":..,"skillId":..}}
  |
  v
server.rs
  - CORS preflight bypass for OPTIONS
  - auth guard (bearer)
  - route ("POST", "/") -> jsonrpc::dispatch(state, body)
  |
  v
jsonrpc::dispatch
  1. Parse envelope; on failure -> JSON-RPC error -32700
  2. Validate jsonrpc == "2.0" && method non-empty; else -32600
  3. Match method:
     - "message/send"           -> handle_message_send_rpc(...)
     - "message/sendSubscribe"  -> -32004
     - "tasks/get"              -> handle_tasks_get_rpc(...)
     - "tasks/cancel"           -> handle_tasks_cancel_rpc(...)
     - other                    -> -32601
  4. handle_message_send_rpc:
     - Parse params as SendMessageParams (else -32602)
     - Build MessageSendRequest {
           messages: vec![params.message],
           tool:     params.skillId,        // Option<String>
       }
     - Call adapter::handle_message_send(state.adapter, req, params.context_id)
     - Wrap returned A2aTask in JSONRPCResponse.result
     - Map A2aError -> JSONRPCErrorObj (codes already align)
```

`tasks/get` and `tasks/cancel` follow the same pattern, calling
`adapter::handle_task_get(state, params.id)` and
`adapter::handle_task_cancel(state, params.id)`.

## Wire format

### Request

```json
{
  "jsonrpc": "2.0",
  "id": <any JSON value>,
  "method": "message/send" | "tasks/get" | "tasks/cancel",
  "params": { <method-specific> }
}
```

### Params by method

**`message/send`**
```json
{
  "message": {
    "messageId": "m1",
    "role": "user",
    "parts": [{"type": "text", "text": "..."}]
  },
  "contextId": "ctx-42",
  "skillId":   "skill:alibaba"
}
```

`contextId` and `skillId` are optional. When `skillId` is present, the request
bypasses conversational AI routing and directly invokes the named capability
(same behavior as today's `tool` field). When absent, the request is routed
through the AI conversational path.

**`tasks/get`** — `{"id": "<task-id>"}`
**`tasks/cancel`** — `{"id": "<task-id>"}`

### Response — success

```json
{
  "jsonrpc": "2.0",
  "id": <echoed>,
  "result": <A2aTask>
}
```

### Response — error

```json
{
  "jsonrpc": "2.0",
  "id": <echoed | null>,
  "error": {
    "code":   -32602,
    "message": "invalid params",
    "data":    "missing 'message'"
  }
}
```

### Error code mapping

| Situation | Code |
|---|---|
| Body isn't valid JSON | `-32700` |
| `jsonrpc != "2.0"` or missing `method` | `-32600` |
| Unknown method | `-32601` |
| Params don't match method schema | `-32602` |
| Adapter returned internal error | `-32603` |
| Task not found | `-32001` |
| `message/sendSubscribe` (streaming) | `-32004` |

### A2aTask changes

```json
{
  "id":        "<task-id>",
  "contextId": "<echoed-if-present>",       // NEW; skipped when None
  "status": {
    "state":     "completed",
    "message":   {...},
    "timestamp": "2026-07-06T02:35:13Z"
  },
  "artifacts": [
    { "artifactId": "<16-hex>",             // NEW
      "name":       "results",
      "parts":      [...] }
  ],
  "history": [...]
}
```

## Type additions (in `types.rs`)

```rust
#[derive(Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: serde_json::Value,       // opaque; echoed back
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Serialize)]
pub struct JsonRpcResponse<T: Serialize> {
    pub jsonrpc: &'static str,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcErrorObj>,
}

#[derive(Serialize)]
pub struct JsonRpcErrorObj {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct SendMessageParams {
    pub message: A2aMessage,
    #[serde(default, rename = "contextId")]
    pub context_id: Option<String>,
    #[serde(default, rename = "skillId")]
    pub skill_id: Option<String>,
}

#[derive(Deserialize)]
pub struct TaskIdParams {
    pub id: String,
}
```

## Isolation & boundaries

- `jsonrpc.rs` is pure translation: JSON-RPC envelope in, `A2aTask`
  out. It has no I/O, no clocks, no plugin knowledge; it delegates all business
  logic to the existing `adapter::handle_*` functions.
- `server.rs` remains a thin HTTP shim: parse method + path, run auth, hand off.
- `types.rs` stays pure serde types.
- `adapter.rs` public API changes only additively: `handle_message_send`'s
  signature becomes
  `handle_message_send(state, request, context_id: Option<String>) -> Result<A2aTask, A2aError>`.
  All other public adapter functions (`handle_task_get`, `handle_task_cancel`,
  `handle_task_list`, `build_agent_card*`) are unchanged.
- Every unit can be tested independently: envelope parsing without hitting the
  adapter, adapter without hitting the network, server routing without needing
  a real listener (already the pattern in current tests).

## Testing

### `jsonrpc.rs` unit tests

- `dispatch_message_send_wraps_task_in_result` — happy path; assert
  `.result.id` present and `.error` absent.
- `dispatch_message_send_forwards_skill_id_to_tool` — assert
  `MessageSendRequest.tool == params.skillId` by using a spy adapter.
- `dispatch_message_send_echoes_context_id` — set
  `contextId:"ctx-x"`, assert response `.result.contextId == "ctx-x"`.
- `dispatch_message_send_omits_context_id_when_absent` — verify
  serialization skips the field.
- `dispatch_tasks_get_returns_stored_task` — pre-populate registry, assert
  task returned.
- `dispatch_tasks_get_missing_returns_32001`.
- `dispatch_tasks_cancel_marks_canceled`.
- `dispatch_unknown_method_returns_32601`.
- `dispatch_invalid_json_returns_32700`.
- `dispatch_missing_jsonrpc_field_returns_32600`.
- `dispatch_message_send_bad_params_returns_32602`.
- `dispatch_streaming_returns_32004`.
- `error_response_echoes_request_id`.

### `server.rs` tests — updated set

Delete:
- Any test that exercises `POST /message:send`, `GET /tasks`,
  `GET /tasks/{id}`, `POST /tasks/{id}:cancel` at the HTTP layer.

Keep or add:
- `agent_card_route_requires_bearer_token_and_returns_card` — unchanged.
- `options_returns_204_without_auth` — unchanged.
- `unknown_route_returns_404` — unchanged (still 404 for e.g. `GET /foo`).
- `post_root_requires_bearer_token` — new; sends a valid JSON-RPC envelope
  without auth, expects `401`.
- `post_root_message_send_end_to_end` — new; posts the hub's exact envelope
  shape (with `skillId`, `contextId`) at `POST /` and asserts a JSON-RPC
  response with `.result.id` and `.result.contextId`.

## Migration steps

Order matters so the tree stays buildable at every step.

1. **types.rs, tasks.rs, adapter.rs** — add JSON-RPC types, add
   `context_id: Option<String>` on `TaskRecord`, add `artifact_id: String`
   on `A2aArtifact`, thread `context_id` through `handle_message_send` (new
   `Option<String>` param; existing callers pass `None`), generate `artifactId`
   at artifact-construction points. `cargo test -p omnilauncher-backend`
   remains green (existing tests only check fields they were already
   checking).
2. **jsonrpc.rs** — implement `dispatch()` and its unit tests. `cargo test`
   green.
3. **server.rs** — replace the four legacy match arms with a single
   `("POST", "/")` arm calling `jsonrpc::dispatch`. Delete stale server
   tests, add the two new `POST /` server tests. `cargo test` green.
4. **Format / lint** — `cargo fmt`, `cargo clippy -- -D warnings`,
   `cargo check` at workspace root.

## End-to-end verification

Before calling the change done:

1. Rebuild and restart the OmniLauncher backend.
2. Refresh the hub's upstream: `curl -X POST -H "Authorization: Bearer
   <admin_key>" http://localhost:8222/admin/upstreams/refresh`.
3. Hub health flips to healthy:
   ```
   curl http://localhost:8222/health
   → {"status":"ok","upstreams":{"healthy":1,"total":1}}
   ```
4. Composite agent card is populated:
   ```
   curl http://localhost:8222/.well-known/agent-card.json | jq '.skills | length'
   → 75
   ```
5. End-to-end forward succeeds:
   ```
   curl -X POST -H "Authorization: Bearer <hub_api_key>" \
        -H "Content-Type: application/json" http://localhost:8222/ \
        -d '{"jsonrpc":"2.0","id":1,"method":"message/send",
             "params":{"skillId":"omnilauncher.skill:alibaba",
                       "message":{"role":"user","messageId":"m1",
                                  "parts":[{"type":"text",
                                            "text":"how many VMs in alibaba"}]}}}'
   ```
   → JSON-RPC response with `.result.status.state == "completed"` and text
   containing the VM count (currently 11,018 based on live upstream data).

Only after all five steps succeed is the change complete.

## Risks and mitigations

- **Breaking clients we didn't know about** — mitigation: user confirmed the
  hub is the only client. If we discover another client after the fact, we
  can add the legacy routes back as a dedicated ticket; the adapter is unchanged
  so this is a purely additive follow-up.
- **`contextId` collision with existing stored tasks** — mitigation: field is
  `Option<String>`; pre-existing terminal tasks have `None` and serialize
  without the field.
- **`artifactId` breaking downstream consumers** — mitigation: field is
  additive; no existing client on this branch reads artifact IDs.
