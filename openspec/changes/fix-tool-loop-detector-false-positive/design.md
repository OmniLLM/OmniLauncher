## Context

The agentic tool loop in `src-tauri/src/ai/router.rs` runs up to
`max_tool_iterations` rounds of model → tool → model. A short-window loop
detector (sliding window of size 3) fires `final_content = "Agent stuck in
a loop..."` and breaks the loop when the last three iterations' tool-call
*fingerprints* are identical. The fingerprint is
`"<tool_name>|<arguments>"` joined across all calls in one iteration.

The detector has two related problems:

1. **It ignores tool RESULTS.** A model that issues the same call three
   times in a row because the first two returned transient errors (auth
   expiry, partial data, timeouts) is correctly retrying, not looping. The
   detector cannot tell the difference because it never inspects the
   `tool_result_messages` that flow back to the model.
2. **It is opaque on failure.** When it fires, the user sees one sentence
   and nothing else — no tool name, no arguments, no prior result. There
   is no debug path from the UI, and the user cannot tell whether their
   prompt or the system is at fault.

The bug reproduces against `openstack dmz` queries (which need multiple
region-iterated calls) while `openstack dev` (a single per-region count
call) works.

There are no existing unit tests for the detector — the function is
embedded in the middle of a 1600-line `process_query` flow. Tests are
part of this change.

## Goals / Non-Goals

**Goals:**

- Stop the detector from firing on three identical *requests* whose
  *results* differed.
- When the detector does fire, surface enough context for the user to
  self-diagnose (repeated tool name, arguments and last result, both
  credential-masked).
- Give the user an escape hatch (a setting) to disable the detector when
  debugging a long multi-step skill.
- Cover both the false-positive and the true-positive cases with unit
  tests so the regression cannot return silently.
- Preserve every other behavior of the agentic loop (iteration cap,
  continuation nudges, error classification).

**Non-Goals:**

- Rewriting the agentic loop. The fix is localized to the loop-detection
  block (`src-tauri/src/ai/router.rs:715–733`) plus the message construction
  on trip.
- Per-tool loop policies. One global detector with one global override.
- Smarter loop heuristics (longer window, hash-of-result-set, fuzzy
  matching). Out of scope; result-equality is the minimum sufficient fix.
- Streaming the loop-trip surface as it accumulates. The new richer
  message is delivered the same way the current one-liner is.
- New telemetry / metrics. The change adds a debug log line, nothing more.

## Decisions

### D1: Fingerprint over (request, result), not request alone

The detector SHALL track tuples of
`(tool_calls_fingerprint, tool_results_fingerprint)` and SHALL fire only
when the last three tuples are pairwise identical.

`tool_results_fingerprint` is the same join as the request fingerprint,
applied to the tool-result-message content for that iteration:
`"<tool_name>|<sha256(result_content)>"` joined across all calls,
truncated to a bounded length. We hash the result content to keep memory
bounded — tool results can be megabytes.

**Alternatives considered:**

- *Compare raw result strings*: simpler, but unbounded memory if a tool
  returns a large blob each iteration.
- *Compare an "is_error" boolean instead of the result*: cheaper, but
  misses the case where the model retries because the result was
  *partially correct but incomplete* (no error flag set).
- *Length-only comparison of the result*: false negatives are easy to
  construct (two error messages of identical length).

SHA-256 of the result content is the sweet spot: bounded memory, no false
matches at the granularity we care about, deterministic.

### D2: Loop-trip message includes redacted context

When the detector fires, the constructed `final_content` SHALL include:

- The repeated tool name(s).
- The arguments JSON, run through the existing `log_masking` redactor
  before formatting.
- A truncated snippet (first ~500 chars) of the last tool result.
- A one-line hint explaining what the user can do (re-prompt with more
  specifics, or disable the detector via Settings).

The existing `log_masking` module already covers credential shapes seen
elsewhere in logs; reusing it keeps the credential surface unified with
the `logging-and-masking` capability seeded earlier.

**Alternatives considered:**

- *Show raw arguments + raw result*: would leak secrets when a tool
  receives or returns credentials.
- *Show only a fixed string*: keeps the bug undiagnosable.

### D3: Settings toggle, default ON

A new `AppSettings` field `ai_loop_detector_enabled: bool` defaulting to
`true` SHALL be threaded through the existing `AiClient` /
`process_query` config path. When `false`, the detector code is bypassed
entirely (still incrementing the iteration cap counter so
`max_tool_iterations` remains the upper bound).

**Alternatives considered:**

- *No setting, just rely on tighter heuristic*: leaves no escape hatch
  for genuinely-long skills. Anyone who hits a false positive after this
  change would be stuck.
- *Per-tool override*: more complex, no demonstrated need yet. Can be
  added in a future change without breaking this one.

The field follows the same `#[serde(default = ...)]` pattern as
`ai_max_retry_attempts` and the timeout fields, so existing
`settings.json` files remain valid after upgrade.

### D4: Tests pin both the false-positive and true-positive

`cargo test` SHALL gain unit tests around an extracted helper function:

```rust
fn is_loop(window: &VecDeque<(String, String)>) -> bool
```

That extraction lets the test exercise the detector in isolation without
spinning up the full agentic loop. Two tests at minimum:

- `test_is_loop_negative_differing_results` — three identical request
  fingerprints with three differing result fingerprints → `false`.
- `test_is_loop_positive_identical_pairs` — three identical
  (request, result) pairs → `true`.

A third test pins the window-size precondition (`window.len() < 3 →
false`).

## Risks / Trade-offs

- **[Risk]** The hash-of-result fingerprint can still match across
  semantically-different but byte-identical results (e.g. an empty list
  from two different but legitimately-empty calls).
  → **Mitigation**: combined with the request fingerprint, this is the
  intended behavior — three iterations of "same call, same empty result"
  IS a loop. Documented explicitly in the spec's scenario.

- **[Risk]** Disabling the detector via the new setting leaves only
  `max_tool_iterations` as the upper bound, which clamps to 100. A model
  could in theory burn 100 iterations of an actual loop.
  → **Mitigation**: that is the documented purpose of the toggle. The
  default stays `true`. The setting's tooltip warns about it. The
  iteration cap is already a hard ceiling.

- **[Risk]** A future tool that returns large non-deterministic content
  (timestamps, request IDs) will hash-mismatch even when the *meaningful*
  result is unchanged, so the detector will under-fire there.
  → **Mitigation**: under-firing is the safer direction; it can only
  cause the agent to run up to `max_tool_iterations` instead of stopping
  at 3. The iteration cap already bounds the worst case. A future
  change can introduce per-tool result-normalization if a real case
  motivates it.

- **[Risk]** The richer loop-trip message could itself leak a credential
  if `log_masking` misses a shape.
  → **Mitigation**: reuses the existing redaction surface, which is
  separately spec'd by `logging-and-masking` and unit-tested there. Any
  gap is a `logging-and-masking` bug, not a regression of this change.

- **[Risk]** Extracting `is_loop` changes the call site and could be
  reverted by a future merge.
  → **Mitigation**: the tests live at the helper's level and would fail
  if the helper is inlined back into `process_query` without preserving
  the (request, result) tuple logic.
