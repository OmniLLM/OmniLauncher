# A2A Full Capability Parity Design

Date: 2026-06-26

## Goal

A2A should expose the same useful capabilities that OmniLauncher agents can use. An authenticated A2A client should be able to discover and invoke plugins, query-style launcher capabilities, executable launcher actions, installed skills, and agent delegation through one consistent A2A surface.

This extends the existing A2A server instead of replacing it. The current server already supports bearer auth, Agent Card discovery, synchronous `message:send`, task listing, task lookup, cancellation, and explicit unsupported errors for streaming. The gap is that A2A discovery and direct invocation are currently centered on plugin tool schemas, while OmniLauncher agents can reach more capability shapes.

## Scope

Implement exact parity for runtime capabilities reachable by OmniLauncher agents:

- Built-in and external plugins with AI tool schemas.
- Query-only or launcher-style plugins that produce `QueryResult` values.
- Executable launcher result actions where the existing backend can execute the result.
- Installed and bundled skills exposed through the existing skill runner capability.
- Agent delegation exposed through the existing agent delegate plugin capability.

This does not add a new user approval gate. The A2A bearer token remains the trust boundary, matching the existing A2A design. LAN exposure remains opt-in.

## Architecture

Add a normalized A2A capability layer under `src-tauri/src/a2a/`.

Primary additions:

- `a2a/capabilities.rs` — builds and owns A2A-facing capability metadata and dispatch ids.
- `A2aCapability` — normalized enum/struct representing tool-schema plugins, query plugins, launcher action execution, skills, and agent delegation.
- Capability discovery helpers — convert normalized capabilities into Agent Card skills.
- Capability execution helpers — route a direct A2A tool request to the correct existing OmniLauncher execution path.

`adapter::build_agent_card` should use the capability registry instead of only `PluginManager::all_tool_schemas()`. It should still produce conservative descriptions for capabilities that lack explicit JSON schemas.

`adapter::handle_message_send` should continue to support two modes:

1. Conversational mode: no explicit tool means route text through the existing AI router.
2. Direct capability mode: explicit tool/capability id means dispatch through the capability registry.

## Capability Model

Each exposed capability has:

- stable id used by A2A clients;
- display name;
- description;
- optional input schema;
- tags;
- kind;
- execution target metadata.

Initial kinds:

- `ToolSchemaPlugin` — direct call to `PluginManager::execute_tool`.
- `QueryPlugin` — run a specific plugin's `query` method and return matching `QueryResult` values as A2A artifact data.
- `LauncherQuery` — run `PluginManager::query_all` and return aggregate launcher results.
- `Skill` — execute through the existing skill runner pathway, not by duplicating skill execution logic.
- `AgentDelegate` — execute through the existing agent delegate plugin/tool pathway.

If a plugin has a structured tool schema, prefer that shape. If it does not, expose a conservative query capability using name, description, and keyword metadata.

## Data Flow

Discovery flow:

1. A2A client sends authenticated `GET /.well-known/agent.json`.
2. A2A server locks `PluginManager` and `SkillManager` snapshots.
3. Capability registry derives normalized capabilities.
4. Adapter converts capabilities to Agent Card skills.
5. Server returns the Agent Card with bearer auth and honest non-streaming capabilities.

Direct execution flow:

1. A2A client sends authenticated `POST /message:send` with a `tool`/capability id and text or data input.
2. Adapter creates a submitted task and marks it working.
3. Capability registry resolves the id.
4. Executor calls the existing OmniLauncher path for that capability kind.
5. Adapter stores output messages/artifacts and marks the task completed, or stores a masked error and marks it failed.

Conversational flow stays unchanged except that the Agent Card now better describes the capabilities the conversation can use.

## Error Handling

- Unknown capability id returns a failed task with `Tool not found: <id>` style message.
- Malformed capability input returns a failed task with a concise validation message.
- Plugin, skill, or agent execution failures are masked before storing on the A2A task.
- Query capabilities should return an empty result artifact rather than fail when no results match.
- Unsupported optional A2A operations remain explicit errors.

The backend must stay responsive after any capability failure.

## Security

A2A bearer auth is required before any discovery or execution. Exact parity means authenticated A2A clients can reach powerful capabilities, including shell, file, network, skill execution, and agent delegation when those are available to OmniLauncher agents.

The implementation must not log raw A2A tokens, backend tokens, API keys, or plugin/skill secrets. Errors stored in tasks must pass through existing masking.

No public or LAN exposure is enabled by this work. Existing local-only defaults remain.

## Testing Strategy

Add tests before production changes.

Unit tests:

- Agent Card includes tool-schema plugins and query-only plugin capabilities.
- Agent Card includes skill/agent delegate capabilities when present in runtime state.
- Direct capability execution dispatches tool-schema plugins through `execute_tool`.
- Direct query capability execution returns structured `QueryResult` artifacts.
- Unknown capability id produces a failed A2A task without panic.
- Capability ids are stable and do not collide.

Integration-style tests where practical:

- Authenticated A2A discovery exposes more capabilities than tool schemas alone.
- Authenticated `message:send` can invoke a query-only capability.
- Authenticated `message:send` can invoke an existing structured tool capability.

Manual verification:

- Run OmniLauncher backend with A2A enabled.
- Fetch Agent Card with bearer token and inspect plugin, skill, and agent capabilities.
- Invoke at least one structured tool and one query-style capability through `curl`.
- Confirm unauthorized clients still get `401`.

## Out of Scope

- Streaming task updates.
- Push notifications.
- Durable A2A task history.
- New per-tool approval prompts.
- Rewriting plugin, skill, or agent execution engines.
- Full external A2A conformance suite.

## Open Implementation Notes

- Prefer reusing `PluginManager` indexes and existing execution methods rather than introducing parallel plugin lookup logic.
- If direct execution of launcher-result actions requires large coupling to `main.rs` command helpers, implement query-result discovery first and leave action execution behind a clearly named executor seam in the same capability model.
- Keep capability generation deterministic so clients can cache ids safely within a running version.
