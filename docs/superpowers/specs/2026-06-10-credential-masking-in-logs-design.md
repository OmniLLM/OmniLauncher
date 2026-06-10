# Credential Masking in Logs

Date: 2026-06-10
Status: Approved (autonomous-completion goal in effect)

## Problem

`PluginManager::execute_tool` logs every tool invocation's full JSON
arguments at `DEBUG`:

    src-tauri/src/plugins/mod.rs:402
    log::debug!("PluginManager.execute_tool: name='{}' args={}", name, args);

The launcher's debug logger is a `WriteLogger` at `LevelFilter::Trace`
that writes to a file on disk (see `init_debug_logging` in
`src-tauri/src/main.rs`). When the AI calls `file_write` to materialize
a GCP service-account credential to `/tmp/gcp_sa.json`, the full
private key — header, body, footer — is persisted to the debug log
verbatim. Anyone who later reads the log file (support bundle, screen
share, accidental paste) sees the key.

The same risk exists at three other audited log sites that emit user
data that may contain secrets:

- `src-tauri/src/main.rs:1015` — spawned external command args
  (`spawn_external_command`); arguments such as `--password` /
  `-H "Authorization: ..."` would print in the clear.
- `src-tauri/src/main.rs:1859` — launcher CLI args at startup, which
  may include `--server-token <token>` style flags.
- `src-tauri/src/ai/client.rs` — AI request logging; lines 260–272 and
  370–385 already mask the bearer header but a future change could
  regress, so the same masker should be used there.

The `shell_exec` plugin (`src-tauri/src/plugins/bash_exec.rs:133`)
logs the literal command string, which can contain interpolated
secrets, and `execute_skill` in `src-tauri/src/plugins/skill_runner.rs`
forwards JSON args. Both fall under the broader "anywhere we log
user-supplied data" rule that the new utility makes easy to follow.

## Goals

- Sensitive values (private keys, API tokens, passwords) must not
  appear in the debug log going forward.
- Detection works for the JSON-args case (the reported leak) **and**
  the array-of-strings case (spawned-command args, CLI args).
- The fix is auditable: a reviewer can grep the codebase for risky
  log sites and confirm each one routes through the masker.
- No regressions in build or tests.

## Non-goals

- Rewriting existing log files on disk. Per user direction, old logs
  are left untouched; users can delete `debug.log` if they want a
  clean slate.
- A logging-framework wrapper. `simplelog` keeps doing exactly what
  it does today; masking happens at call sites only.
- Allowlist-style masking. Tools have too many ad-hoc arg shapes
  for an allowlist to be maintainable.
- Solving log redaction in plugin-emitted output (e.g. command
  stdout/stderr captured by `bash_exec`). That is a separate concern
  and out of scope for this spec.

## Design

### Module layout

A new file `src-tauri/src/log_masking.rs`, registered as a public
module in `src-tauri/src/lib.rs` so both binary and tests can reach it
(the crate already has `staticlib`/`cdylib`/`rlib` outputs).

### Public API

    /// Mask sensitive fields in a JSON value and return its
    /// pretty-free string form, suitable for `log::debug!` etc.
    pub fn mask_json(value: &serde_json::Value) -> String;

    /// Mask sensitive characters in a single string by running the
    /// value-pattern sweep. Used for free-form text that may embed
    /// PEM blocks or tokens.
    pub fn mask_str(input: &str) -> String;

    /// Mask each element of a `&[impl AsRef<str>]` and join with
    /// spaces, suitable for logging spawned-command argv arrays.
    pub fn mask_argv<S: AsRef<str>>(args: &[S]) -> String;

All three return owned `String`s — they are called at log sites where
allocation is acceptable.

### Detection rules

Two sets of patterns live as `once_cell::sync::Lazy<Regex>` values
inside the module (the crate already uses `regex`; `once_cell` is
brought in if not already present, otherwise `std::sync::OnceLock`).

**Key-name denylist** (case-insensitive, matches whole field name):

    private_key | api_key | apikey | secret | password | passwd
    | token | authorization | credential[s]? | client_secret
    | access_key | refresh_token | session_token | bearer

When `mask_json` walks an object and a key matches the denylist, the
value is replaced with the string `"***"` (regardless of type).

**Value-pattern denylist** (applied to any string value, in `mask_str`
and inside `mask_json`):

- `-----BEGIN [A-Z ]+PRIVATE KEY-----[\s\S]*?-----END [A-Z ]+PRIVATE KEY-----`
  (catches RSA, EC, generic PRIVATE KEY blocks)
- JWT-shape: `eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}`
- GitHub PATs: `gh[pousr]_[A-Za-z0-9]{36,}`
- AWS access key id: `AKIA[0-9A-Z]{16}`
- Generic bearer-header value: `(?i)bearer\s+[A-Za-z0-9._\-+/=]{20,}`

Each match is replaced with `***`. The replacement is deliberately
short and shape-free — per user direction, no length hints, no
"first/last few chars".

### JSON traversal

`mask_json` recursively walks the input:

- Objects: for each `(key, value)` pair, if the key matches the
  key-name denylist, replace `value` with `Value::String("***")`;
  otherwise recurse into `value`.
- Arrays: recurse into each element.
- Strings: run `mask_str` over the string.
- Other primitives: passthrough.

The walked tree is then serialized via `serde_json::to_string`
(compact form to match today's log style).

### Call-site changes

| File                                          | Line | Change                                            |
|-----------------------------------------------|------|---------------------------------------------------|
| `src-tauri/src/plugins/mod.rs`                | 402  | `args={}` → `args={}` with `mask_json(&args)`     |
| `src-tauri/src/main.rs`                       | 1015 | `{args:?}` → `{}` with `mask_argv(args)`          |
| `src-tauri/src/main.rs`                       | 1026 | same                                              |
| `src-tauri/src/main.rs`                       | 1859 | `{:?}` of `Vec<String>` → `mask_argv(&args)`      |
| `src-tauri/src/ai/client.rs` (lines 260, 375) | —    | Already masked; add comment pointing at module    |

Other sites surfaced in the audit (`bash_exec.rs:133`,
`skill_runner.rs:300/332`) are noted in code comments referring to
the new module so future contributors find it; this spec scopes the
edits to the four call sites the user picked.

### Failure modes

- `serde_json::to_string` on a recursive structure cannot recurse
  infinitely because the input is already a borrowed `&Value` (which
  cannot contain cycles). The function returns `String`, never
  `Result`; if the (impossible) serialization fails, it returns the
  literal `"<unserializable>"` so a log line is never lost.
- The regex set is compiled once at startup via `Lazy`/`OnceLock`.
  Compilation failure would be a programmer error caught by unit
  tests, not a runtime concern.

## Testing

A `#[cfg(test)] mod tests` inside `log_masking.rs` covers:

1. Object with a `private_key` field — value is `"***"`, surrounding
   fields untouched.
2. Object with nested credential under
   `{"creds": {"api_key": "xyz"}}` — nested value masked.
3. PEM block embedded in a string-typed field with a benign key
   name (`{"content": "...-----BEGIN PRIVATE KEY-----..."}`) — the
   PEM block is replaced with `***`, surrounding prose preserved.
4. JWT-shaped value masked.
5. `Authorization: Bearer <long-token>` masked.
6. `mask_argv` with `["--user", "alice", "--password", "hunter2"]` —
   `--password` is replaced (case-insensitive substring match in the
   element preceding the secret is **not** the strategy; we mask
   any element that matches a value pattern, and we additionally
   mask the element that follows a key-name token like `--password`,
   `-p`, `--token`, `--api-key`). See implementation notes below.
7. Plain non-sensitive JSON unchanged (sanity).

### `mask_argv` strategy

For an argv array, two sweeps:

- **Value sweep**: each element is passed through `mask_str`.
- **Flag-pair sweep**: when an element matches
  `(?i)^-{1,2}(password|token|api[-_]?key|secret|authorization|bearer)$`
  the *following* element (if any) is replaced with `***`.

This catches `--password hunter2` style flags whose value is a short
benign-looking string the value sweep would miss.

## Rollout

Single commit per feature. Build + test must be green before merge.
Old log files on disk are not touched; users can rotate manually.
