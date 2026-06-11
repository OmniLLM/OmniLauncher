## 1. Extract and test the loop-detector helper

- [x] 1.1 In `src-tauri/src/ai/router.rs`, extract a pure helper
      `fn is_loop(window: &VecDeque<(String, String)>) -> bool` that
      returns `true` only when the window has length 3 and all three
      `(request_fp, result_fp)` tuples are pairwise equal.
- [x] 1.2 Add `#[cfg(test)] mod loop_detector_tests` with three tests:
      `test_is_loop_window_smaller_than_three_returns_false`,
      `test_is_loop_negative_differing_results`,
      `test_is_loop_positive_identical_pairs`.
- [x] 1.3 Run `cargo test --lib --manifest-path src-tauri/Cargo.toml
      loop_detector_tests` and confirm all three pass.

## 2. Wire the helper into the agentic loop

- [x] 2.1 Replace the inline fingerprint comparison at
      `src-tauri/src/ai/router.rs:715–733` with:
      - Compute `request_fp` exactly as today.
      - Build `result_fp` by joining `"<tool_name>|<sha256(result_content)>"`
        across `tool_result_messages` for the iteration; truncate the
        joined string to a bounded length.
      - Push `(request_fp, result_fp)` into `recent_fingerprints`
        (now typed `VecDeque<(String, String)>`).
      - Call `is_loop(&recent_fingerprints)` to decide whether to halt.
- [x] 2.2 Build the richer loop-trip `final_content`:
      - Include the repeated tool name(s) from `tool_calls`.
      - Run the arguments JSON through the existing `log_masking`
        redactor and include a snippet (≤500 chars).
      - Include a truncated snippet (≤500 chars, mark elision) of the
        last tool result, also redacted.
      - Append a one-line hint pointing the user to re-prompt or to
        disable the detector in Settings.
- [x] 2.3 Add a `tracing::debug!` log line when the detector trips,
      logging the redacted tool name + result snippet.
- [x] 2.4 `cargo build --manifest-path src-tauri/Cargo.toml` succeeds.

## 3. Settings field and wiring

- [x] 3.1 In `src-tauri/src/settings.rs`, add field
      `ai_loop_detector_enabled: bool` to `AppSettings` with
      `#[serde(default = "default_ai_loop_detector_enabled")]` and
      `pub fn default_ai_loop_detector_enabled() -> bool { true }`.
- [x] 3.2 Update existing settings tests:
      `test_default_settings_values` asserts the new default is `true`;
      `test_deserializes_missing_ai_timeout_to_default` (or sibling)
      asserts a settings file missing this field deserializes with the
      new field at `true`.
- [x] 3.3 Add `test_preserves_custom_ai_loop_detector_field` that
      round-trips a JSON blob with `ai_loop_detector_enabled: false`.
- [x] 3.4 Thread the field from `AppSettings` through to the agentic
      loop the same way `ai_timeout_secs` flows today (via the
      `AiClient` construction call sites identified in the
      `ai-retry-settings` change).
- [x] 3.5 In `process_query`, when
      `settings.ai_loop_detector_enabled == false`, skip the
      `is_loop`-driven `break` (but still maintain the
      `recent_fingerprints` window so the debug log can record it).
- [x] 3.6 `cargo test --lib --manifest-path src-tauri/Cargo.toml settings`
      passes.

## 4. Frontend toggle

- [x] 4.1 In `src/types/app.ts`, add `ai_loop_detector_enabled: boolean`
      to the `AppSettings` interface and to the in-memory seed object
      used by `SettingsWindow.tsx`.
- [x] 4.2 In `src/components/SettingsWindow.tsx`, add a checkbox to the
      AI tab labelled "Enable AI loop detector" with a tooltip that
      explains the trade-off ("disable only when debugging long
      multi-step skills; the iteration cap still bounds runtime").
- [x] 4.3 `npm test -- --run` passes (no logic-test impact expected, but
      run as a smoke).

## 5. Validate and commit

- [x] 5.1 `openspec validate fix-tool-loop-detector-false-positive`
      passes.
- [x] 5.2 `cargo test --manifest-path src-tauri/Cargo.toml` passes
      end-to-end.
- [x] 5.3 Stage the change directory and the source / frontend edits,
      commit with `fix(ai): loop detector ignores tool results and
      hides context`, author per repo convention.
- [ ] 5.4 After merge: `openspec archive
      fix-tool-loop-detector-false-positive` to apply the delta to
      `openspec/specs/ai-chat/spec.md` and move the change into
      `openspec/changes/archive/`.
