# Configurable AI Retry

Date: 2026-06-10
Status: Approved (autonomous-completion goal in effect)

## Problem

`AiClient::chat_with_tools` hardcodes the retry budget:

    src-tauri/src/ai/client.rs:156-157
    const MAX_ATTEMPTS: u32 = 3;
    const BASE_DELAY_MS: u64 = 2_000;

The numbers were chosen for the original use case but become wrong for
others: a flaky local model warrants more attempts, an upstream that
fails fast warrants fewer, and someone running an interactive session
may want to disable retries entirely to fail fast. Today the only way
to change them is to recompile.

## Goals

- Surface the retry budget (max attempts + base backoff) as
  user-editable settings stored in `AppSettings`.
- Changes apply on next AI request, like other AI settings.
- Defaults preserve current behavior exactly (3 attempts, 2000 ms base).
- UI lives next to the existing "Timeout" and "Tool Iterations"
  inputs in `SettingsWindow.tsx`.

## Non-goals

- Per-model overrides. One global setting per knob; users with
  multi-model needs can change values inline.
- Sophisticated retry strategies (token bucket, circuit breaker).
  Out of scope; the current exponential-backoff + jitter shape is
  preserved verbatim.
- Retrying permanent errors. The classifier stays as-is — settings
  only tune *how many* transient retries we do and *how long* between
  them.
- Disabling retries via "0 attempts". `max_attempts` is clamped to
  `>= 1` so we always make at least the original request; setting it
  to `1` is the way to disable retries.

## Design

### Settings model

Two new fields on `AppSettings` plus matching default helpers, mirroring
the existing `ai_timeout_secs` pattern exactly:

    pub fn default_ai_max_retry_attempts() -> u32 { 3 }
    pub fn default_ai_retry_base_delay_ms() -> u64 { 2_000 }

    pub struct AppSettings {
        ...
        #[serde(default = "default_ai_max_retry_attempts")]
        pub ai_max_retry_attempts: u32,
        #[serde(default = "default_ai_retry_base_delay_ms")]
        pub ai_retry_base_delay_ms: u64,
        ...
    }

`Default::default()` returns the same numbers so fresh installs see no
change. Existing settings files on disk that lack the fields are
populated by `#[serde(default = ...)]`.

`AppSettings` already auto-`derive(Deserialize)` with `serde(default)`
fallbacks for the analogous timeout fields, so no migration code is
needed — the test that pins the deserialization fallback for
`ai_timeout_secs` gets a sibling assertion.

### Client wiring

`AiClient` gains two fields (kept private):

    pub struct AiClient {
        base_url: String,
        api_key: String,
        model: String,
        request_timeout_secs: u64,
        max_retry_attempts: u32,
        retry_base_delay_ms: u64,
    }

A new builder method preserves backwards-compatible constructors:

    impl AiClient {
        pub fn with_retry(
            base_url: String,
            api_key: String,
            model: String,
            request_timeout_secs: u64,
            max_retry_attempts: u32,
            retry_base_delay_ms: u64,
        ) -> Self {
            Self {
                base_url,
                api_key,
                model,
                request_timeout_secs: request_timeout_secs.max(1),
                max_retry_attempts: max_retry_attempts.max(1),
                retry_base_delay_ms,
            }
        }
    }

`AiClient::new` and `AiClient::with_timeout` keep their current
signatures and delegate to `with_retry` using the default constants
(so existing tests and external callers don't need to change).

`chat_with_tools` reads the fields instead of the consts:

    for attempt in 0..self.max_retry_attempts {
        if attempt > 0 {
            let backoff_ms = self.retry_base_delay_ms * (1u64 << (attempt - 1));
            ...
        }
        ...
    }

The shift-by-`(attempt - 1)` could overflow for absurd settings; we
clamp the `max_retry_attempts` to `30` at construction (the natural
ceiling where `1u64 << 29` is already half a billion milliseconds).

### Call-site wiring

Every `AiClient::new` / `AiClient::with_timeout` call site that has a
settings handle in scope switches to `with_retry`, passing
`settings.ai_max_retry_attempts` and `settings.ai_retry_base_delay_ms`.
Sites identified by the earlier audit:

- `src-tauri/src/main.rs:386, 1408, 1883, 1996`
- `src-tauri/src/server.rs:529, 983`

The two `AiClient::new` call sites inside `client.rs` tests stay on
the old constructor — tests want the default budget.

### UI

Add two `<input type="number">` rows in `src/components/SettingsWindow.tsx`
beneath "Tool Iterations":

- "Retry Attempts" — `min=1 max=10`, default 3, tooltip "How many
  times the AI client tries a transient-error request before giving up".
- "Retry Base Delay (ms)" — `min=0 max=60000` (step 100), default 2000,
  tooltip "Base backoff delay; doubled on each subsequent retry plus
  jitter".

The `AppSettings` TypeScript interface in `src/types/app.ts` gets the
matching fields. The seed object in `SettingsWindow.tsx:114` (the
in-memory fallback used while settings load) gains both keys.

### Testing

Rust:

- `settings_tests::test_default_settings_values` — extend to assert the
  two new defaults are 3 and 2000.
- `settings_tests::test_deserializes_missing_ai_timeout_to_default` —
  extend assertions to cover the two new fields as well.
- New `settings_tests::test_preserves_custom_ai_retry_fields` — round-
  trip a JSON blob with custom values.
- New `ai::client::client_tests::test_with_retry_clamps_max_attempts_to_one`
  — pass `max_retry_attempts = 0`, assert that the constructor stores
  `1` so we always make the original request.

Frontend:

- No new tests; the existing `SettingsWindow` is rendered by manual
  smoke and the round-trip is covered by the Rust tests.

## Rollout

Single commit. Build + Rust tests must be green before merge. Frontend
tests (`npm test -- --run` if present) are also run if they exist in
the repo, but the change does not touch any logic frontend tests cover.
