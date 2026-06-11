## Why

The agentic tool loop in `src-tauri/src/ai/router.rs` falsely halts legitimate
multi-step tool conversations with "Agent stuck in a loop: repeated identical
tool calls detected." The detector trips on three identical tool-call
*fingerprints* in a row regardless of what the tool *results* were. A model
that legitimately retries a transient error (auth expiry, partial result,
region-discovery iteration) is indistinguishable from a model that is
actually wedged. Today this reproduces against `openstack dmz` queries while
the same shape of query against `openstack dev` works, because the dmz path
needs more iterated calls and is therefore the first place the over-eager
guard fires.

Worse, when the guard does fire the user sees a single opaque line and zero
context — no tool name, no arguments, no prior result — so the bug can be
neither self-diagnosed nor reported with usable detail.

## What Changes

- Tighten the loop detector to compare tool *result* shape in addition to
  tool *call* shape, so three identical requests with varying / error
  results no longer count as a loop.
- When the detector does fire, the surfaced message SHALL include the
  repeated tool name and a credential-masked snippet of its arguments and
  last result, so the user can self-diagnose.
- Add Rust unit tests pinning both the false-positive case (3 identical
  requests with differing error results → NOT a loop) and the true-positive
  case (3 identical request+result pairs → IS a loop).
- Add a setting (with a safe default) to disable the detector for advanced
  users debugging long multi-step skills, so a future false-positive doesn't
  block work entirely.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `ai-chat`: the existing "Tool-augmented chat" requirement gains a sibling
  requirement governing the loop-detector's behavior — when it fires, what
  it surfaces, and how the user can override it.

## Impact

- **Code:** `src-tauri/src/ai/router.rs` — the agentic loop's fingerprint
  comparison + the final-content message when the guard trips.
- **Settings:** new optional field on `AppSettings`
  (`ai_loop_detector_enabled: bool`, default `true`) wired through to the
  router via the existing `AiClient` config path; frontend Settings window
  gains one matching toggle on the AI tab.
- **Tests:** `cargo test` gains unit tests for the loop detector. No
  frontend test changes.
- **Logs:** new debug-level log line on loop trip, redacted via the
  existing log-masking layer (no new credential surfaces).
- **Backwards compatibility:** default behavior tightens (fewer false
  positives) but does not change any public API; settings files missing the
  new field fall back to the default.
