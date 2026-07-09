# Harness Engineering Guide — Conformance Audit

OmniLauncher's backend is an AI agent runtime, so it is audited against the four
core subsystems defined by the [Harness Engineering Guide](https://harness-guide.com/):
**Agentic Loop, Tool System, Memory & Context, Guardrails** — plus the practice
patterns (Sub-Agent, Skill System, Error Handling, Scheduling).

Status legend: ✅ conforms · ⚠️ partial · ❌ gap

## 1. Agentic Loop  ✅

`src-tauri/src/ai/router.rs :: ai_route`

| Guide requirement | OmniLauncher |
|---|---|
| think → act → observe cycle | `ai_route` loop over tool iterations |
| Turn budget (`max_turns`) | `max_tool_iterations` (settings: `ai_max_tool_iterations`, default 10) |
| Parallel tool calls | executes all `tool_calls` in a turn, appends each result |
| Loop detection (stuck on same call) | `Router::is_loop` — trips only when **both** request AND result fingerprints repeat 3× (retries with changing results are allowed) |
| Streaming to user | progress events via `progress_tx`; SSE `/api/events/*` |
| Exit conditions | no tool calls → done; `finish_reason` handling for `length` / `tool_calls` / `stop`; nudge budget `MAX_CONTINUATION_NUDGES` |

## 2. Tool System  ✅ (⚠️ no MCP)

`src-tauri/src/plugins/` — ~50 plugins, each implements `Plugin::tool_schema` + `execute_tool`.

| Guide requirement | OmniLauncher |
|---|---|
| Schema / implementation separation | `tool_schema()` (JSON schema) vs `execute_tool()` |
| Registry + dispatch | `PluginManager::all_tool_schemas` / `execute_tool` |
| Errors returned as strings (never silent) | tools return `String`; errors formatted as `"Error: …"` |
| Dynamic loading (skill menu) | `load_skill` tool + INSTALLED SKILLS inventory in system prompt |
| Description quality | schemas include format/constraints |
| **MCP (Model Context Protocol)** | ❌ not implemented — external tools are native Rust plugins + `external.rs` process plugins. Candidate for a future `mcp_client` plugin. |

## 3. Memory & Context  ✅

| Concept | OmniLauncher |
|---|---|
| Context (per-call assembly) | `ConversationContext::get_messages_with_system`; token estimate + `compress_if_needed` (70% of 32k budget) |
| Session (per-run state) | `db/conversation.rs` persistent sessions; `session_id` on context |
| Memory (cross-session) | `AGENTS.md` loader (`ai/agent_context.rs`: config → cwd-walk → home) + skills |
| Context pruning | `trim_to_max` + `pairing_safe_drop_count` (never orphans tool results) |
| AGENTS.md pattern | ✅ config-dir `AGENTS.md` becomes base system prompt |

## 4. Guardrails  ✅

`src-tauri/src/guardrails.rs` — **enforced in code, not prompt** (the guide's key rule).

| Guide requirement | OmniLauncher |
|---|---|
| Trust boundary intercepts tool calls | each sensitive plugin calls `Guardrails::check_*` before acting |
| Shell deny-list | pipe-to-shell, process-substitution, fork-bomb, `/etc/passwd` writes |
| Path allow/deny | `check_file_read` / `check_file_write` — system dirs + credential paths, canonicalized against `../` escapes |
| SSRF defense | `check_url` — blocks loopback / private / link-local / cloud-metadata, incl. decimal/hex/IPv6-mapped bypasses |
| Tiered approval (warn vs deny) | `GuardrailAction::{Allow, Warn, Deny}` |
| **Input sanitization / untrusted demarcation** | ✅ AGENTS.md + skills wrapped in `<<<…>>>` with anti-injection wording; **web_fetch** output wrapped in `<<<UNTRUSTED_WEB_CONTENT>>>` (added for conformance) |

Enforcement sites (grep `Guardrails::`): `shell_plugin`, `bash_exec`, `http_client`,
`web_fetch`, `file_read`, `code_tools` (file_edit), `scheduler`.

## Practice Patterns

| Pattern | OmniLauncher |
|---|---|
| Sub-Agent | `plugins/agent_delegate.rs` — delegate to claude/codex/omnicode/opencode, single + parallel, allow-listed, timeout-bounded, inherits AGENTS context |
| Skill System | `skills/` — install/curator/consolidate, trigger-matched injection, `load_skill` on-demand |
| Error Handling | `ai/errors.rs` `classify_ai_error` → ModelError (correct) / ResourceError (compress) / Transient (retry) / Permanent (abort) |
| Scheduling & Automation | `plugins/scheduler.rs` (guardrail-checked at insert + run) |
| A2A / multi-agent | `a2a/` server + omni-agent-hub upstream registration |

## Gaps & Follow-ups

1. **MCP client** (⚠️) — no Model Context Protocol support. Native plugins cover
   current needs; add an `mcp_client` plugin if third-party MCP tool servers are
   required. Tracked as future work.
2. **web_fetch demarcation** (✅ fixed) — external page text is now wrapped as
   untrusted data so embedded instructions can't be read as directives.

Everything else in the guide's Core Concepts is satisfied by the existing backend.
