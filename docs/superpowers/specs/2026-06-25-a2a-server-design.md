# A2A Server Adapter Design

Date: 2026-06-25

## Goal

Expose OmniLauncher as an A2A-compatible local desktop agent. The adapter should let authenticated A2A clients discover OmniLauncher, send conversational tasks, and invoke OmniLauncher plugin/tool capabilities through the official A2A HTTP shape.

This design follows the selected Approach 3: implement the official-compatible server surface first, with a real in-memory task registry and explicit unsupported-operation errors for optional capabilities that are not part of the first version.

## Decisions

- Support both local-only and LAN-accessible modes.
- Default to local-only binding.
- Require token authentication for every A2A request, including local requests.
- Expose both conversational chat and direct tool/plugin capabilities.
- Publish everything OmniLauncher can use, including built-in plugins, external plugins, Flow-adapted plugins, launcher query actions, and AI tool schemas where available.
- Authenticated A2A requests auto-run actions; there is no first-version user approval gate.
- Start and stop the A2A server through settings.
- Use a fixed default port with settings override.
- Target the official A2A HTTP/JSON-RPC-compatible endpoint shape, not a bespoke protocol.

## Architecture

Add a dedicated `a2a` backend subsystem under `src-tauri/src/a2a/`. It should be separate from the frontend API server in `server.rs`, but reuse the same lightweight Tokio TCP listener style already present in `server.rs` and `live_server.rs`.

Primary modules:

- `a2a/mod.rs` — public subsystem entry point.
- `a2a/server.rs` — listener, request parsing, routing, auth, and response encoding.
- `a2a/types.rs` — A2A request/response, Agent Card, message, task, part, artifact, and error types used by this adapter.
- `a2a/tasks.rs` — in-memory task registry and task lifecycle helpers.
- `a2a/adapter.rs` — bridge from A2A operations to `PluginManager` and existing AI chat routing.

The server should run only when enabled in settings. Startup should bind either:

- `127.0.0.1:<port>` for local-only mode, or
- `0.0.0.0:<port>` for LAN mode.

The settings layer should add fields for:

- `a2a_enabled: bool`
- `a2a_bind_lan: bool`
- `a2a_port: u16`
- `a2a_token: Option<String>` or a generated/stored token equivalent

If `a2a_enabled` is true and no token exists, OmniLauncher should generate one and persist it in the same configuration/security style used for the backend token. Secrets must be masked in logs.

## A2A Surface

The first version should implement the endpoints needed for discovery, request/response task execution, and task inspection.

Required or base endpoints:

- Agent Card / discovery endpoint exposing OmniLauncher identity, URL, capabilities, supported input/output modes, and skills/tool descriptions.
- `POST /message:send` for synchronous message submission.
- `GET /tasks/{id}` for task retrieval.
- `GET /tasks` for task listing.
- `POST /tasks/{id}:cancel` for best-effort cancellation.

Optional endpoints should exist only if needed for compatibility; otherwise return explicit A2A errors:

- `POST /message:stream` returns unsupported operation until streaming is implemented.
- `GET /tasks/{id}:subscribe` returns unsupported operation until SSE task updates are implemented.
- Push notification configuration endpoints return push-notification-not-supported until implemented.
- `GET /extendedAgentCard`, if implemented, may return the same card as the base Agent Card with local-only metadata.

The Agent Card must honestly advertise first-version capabilities:

- `streaming: false`
- `pushNotifications: false`
- no claim of persistent task storage beyond the running process
- token/HTTP bearer authentication requirement

The exact Agent Card path and schema should be verified against the current A2A definitions during implementation. Do not hard-code uncertain path assumptions from secondary sources without checking the official definitions/specification in the implementation phase.

## Request Modes

The adapter supports two request patterns.

### Conversational requests

When an A2A message is plain text without an explicit tool directive, the adapter creates a task and routes the text through OmniLauncher’s existing AI chat/tool loop. The result becomes the final task message. Tool calls made by the AI use the existing plugin manager and existing iteration limits.

Initial lifecycle:

1. Create task with state `submitted`.
2. Transition to `working` while the AI route runs.
3. Store final text output as a message part and mark task `completed`.
4. On errors, mark task `failed` with a masked error message.

### Direct tool/plugin requests

The adapter should expose all OmniLauncher-usable capabilities in the Agent Card. Because OmniLauncher has several plugin shapes, direct A2A execution needs a normalized internal invocation model:

1. If a plugin has an AI `tool_schema`, expose that as a callable skill/action and execute through `PluginManager::execute_tool`.
2. If a plugin is query-only or launcher-result based, expose a query capability. A2A clients can submit text for that capability; OmniLauncher runs the plugin query and returns structured results.
3. If a query result has an executable action, expose an execution path that can run the selected action through the existing result execution path.
4. Flow-adapted plugins participate through their synthesized manifests/tool schemas.

Because the user selected “everything OmniLauncher can use,” the first version should not silently hide plugins just because they lack an AI tool schema. For plugins without explicit schemas, generate conservative capability descriptions based on plugin name, description, keyword, and action shape.

## Task Model

Use an in-memory task registry for the first version. To avoid unbounded growth, retain only a bounded number of recent tasks, with a default cap of 100 completed/terminal tasks plus any currently running tasks. Evict oldest terminal tasks when the cap is exceeded.

Each task record should store:

- task id
- created/updated timestamps
- current state
- original A2A request id if present
- request message summary
- output messages/artifacts
- error details, masked
- cancellation flag or cancellation state

Task states should map to A2A concepts as closely as practical:

- `submitted`
- `working`
- `input-required` only if future user approval is added; not expected in v1
- `completed`
- `failed`
- `canceled`
- `rejected` for invalid or unauthorized requests after auth/routing validation

Cancellation is best-effort. If a task has not started, it can be marked canceled. If an AI request is in flight, use the existing cancellation mechanism where practical. If an external plugin process is already running, cancellation may only mark the task canceled after the current timeout boundary.

The registry is intentionally not durable in v1. Restarting OmniLauncher clears A2A task history. The Agent Card and docs should not imply durable task storage.

## Security

Every A2A request must authenticate with a token. The accepted header should include `Authorization: Bearer <token>` and may also support OmniLauncher’s existing custom backend-token header if that reduces implementation duplication. Missing or invalid tokens return `401` without running any plugin or AI action.

LAN mode is an advanced setting. When LAN binding is enabled, the settings UI should clearly show the bind address, port, and token/copy action. Logs must never print the raw token.

Because A2A requests auto-run actions, the token is the trust boundary. This is powerful: authenticated callers can reach shell/file/network-capable OmniLauncher tools. The design intentionally does not add per-action confirmation in v1 because the user selected auto-run, but the implementation should leave room for a future risk policy.

Do not expose the A2A server on public interfaces by default. Do not enable LAN mode by default.

## Error Handling

Use A2A-style JSON error responses for protocol failures:

- parse/invalid JSON
- unsupported method or endpoint
- missing required fields
- unsupported streaming/push operations
- task not found
- unauthorized request
- internal execution failure

Plugin and AI errors should be surfaced as task failures with concise masked messages. The backend must remain responsive after plugin failure, timeout, invalid tool output, or malformed A2A request.

Unsupported optional operations must be explicit. Do not silently accept stream or push requests and then behave like non-streaming requests.

## Settings and UI

Settings should expose enough A2A controls for a user to operate the server:

- enable/disable A2A server
- local-only vs LAN binding
- port override
- token generate/copy/regenerate
- visible current A2A base URL

If settings UI changes are too large for the first implementation increment, the backend settings fields and defaults should still be implemented first, with a minimal UI/status surface added in the same feature before completion.

## Testing Strategy

Unit tests:

- auth rejects missing/invalid token
- auth accepts valid bearer token
- Agent Card reflects configured URL, auth, and capabilities
- endpoint router maps A2A paths correctly
- unsupported streaming/push endpoints return explicit errors
- task registry creates, updates, lists, retrieves, and cancels tasks
- generated tool capabilities include AI-schema plugins and query-only plugins

Integration-style backend tests:

- start A2A server on localhost test port
- fetch Agent Card with token
- send a direct plugin/tool request and retrieve completed task
- send a conversational request through a mocked or controlled AI path where feasible
- verify plugin failures produce failed tasks without crashing server

Manual verification:

- enable server in settings
- copy URL/token
- call discovery and message send with `curl`
- verify local-only binding is not reachable from LAN
- enable LAN mode and verify authenticated LAN request works

## Out of Scope for First Version

- Durable task storage across restarts.
- Push notifications.
- SSE task streaming/subscription.
- Per-tool user approval prompts.
- Public internet exposure or TLS termination inside OmniLauncher.
- Full conformance testing against every A2A client library.

The implementation should keep seams for these features, especially task event emission for later SSE support and risk policy hooks for later approval prompts.
