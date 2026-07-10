# Top-level `ai_model` provider/model Selector Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the top-level `ai_model` string act as a `provider-id/model` selector that chooses the active provider and supplies a fallback model.

**Architecture:** Two pure helpers in `settings.rs` (`parse_model_selector`, `resolve_active_selection`) plus a small helper for concrete-model selection, wired into `AiClient::from_settings`. No storage, schema, or UI changes.

**Tech Stack:** Rust (Tauri backend), `cargo test`, `cargo build --release`.

Spec: `docs/superpowers/specs/2026-07-10-provider-model-selector-design.md`

---

### Task 1: `parse_model_selector` helper

**Files:**
- Modify: `src-tauri/src/settings.rs` (add free function near `provider_caps`, ~line 163)
- Test: `src-tauri/src/settings.rs` (tests module, before closing `}` at line ~1792)

- [ ] **Step 1: Write the failing tests**

Add inside the `#[cfg(test)] mod tests { ... }` block (append before its final `}`):

```rust
    #[test]
    fn selector_recognizes_known_provider_prefix() {
        let ids = vec!["copilot".to_string(), "default".to_string()];
        assert_eq!(
            super::parse_model_selector("copilot/gpt-5.6-sol", &ids),
            (Some("copilot"), "gpt-5.6-sol")
        );
    }

    #[test]
    fn selector_keeps_slash_model_when_prefix_unknown() {
        let ids = vec!["copilot".to_string(), "default".to_string()];
        assert_eq!(
            super::parse_model_selector("jzhu/gpt-5.6-luna", &ids),
            (None, "jzhu/gpt-5.6-luna")
        );
    }

    #[test]
    fn selector_bare_model_and_empty() {
        let ids = vec!["copilot".to_string()];
        assert_eq!(super::parse_model_selector("gpt-5.6-sol", &ids), (None, "gpt-5.6-sol"));
        assert_eq!(super::parse_model_selector("", &ids), (None, ""));
    }

    #[test]
    fn selector_prefix_with_empty_model() {
        let ids = vec!["copilot".to_string()];
        assert_eq!(super::parse_model_selector("copilot/", &ids), (Some("copilot"), ""));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib parse_model_selector selector 2>&1 | tail -20`
Expected: FAIL — `cannot find function 'parse_model_selector'`.

- [ ] **Step 3: Implement `parse_model_selector`**

Insert after the `provider_caps` function (after line 163 in `settings.rs`):

```rust
/// Split a top-level `ai_model` string into an optional provider id and a
/// model string. The provider id is only recognized when the substring
/// before the FIRST '/' exactly matches one of `known_provider_ids`;
/// otherwise the whole string is returned as the model with no provider.
/// This keeps model names that themselves contain '/' (e.g.
/// "jzhu/gpt-5.6-luna") intact when no provider is named "jzhu".
pub fn parse_model_selector<'a>(
    ai_model: &'a str,
    known_provider_ids: &[String],
) -> (Option<&'a str>, &'a str) {
    if let Some((prefix, rest)) = ai_model.split_once('/') {
        if known_provider_ids.iter().any(|id| id == prefix) {
            return (Some(prefix), rest);
        }
    }
    (None, ai_model)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib parse_model_selector selector 2>&1 | tail -20`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/settings.rs
git commit --author="James Zhu <zhujian0805@gmail.com>" -m "feat(settings): add parse_model_selector for provider/model strings"
```

---

### Task 2: `resolve_active_selection` + concrete-model helper

**Files:**
- Modify: `src-tauri/src/settings.rs` (add `first_concrete_model` free fn near `parse_model_selector`; add `resolve_active_selection` method in `impl AppSettings`, after `active_provider()` at line 339)
- Test: `src-tauri/src/settings.rs` (tests module)

- [ ] **Step 1: Write the failing tests**

Add inside the tests module. The helper `provider_named` builds a minimal provider:

```rust
    fn provider_named(id: &str, model: &str) -> super::Provider {
        super::Provider {
            id: id.to_string(),
            name: id.to_string(),
            model: model.to_string(),
            ..super::Provider::default()
        }
    }

    fn settings_with(providers: Vec<super::Provider>, active: &str, ai_model: &str) -> super::AppSettings {
        let mut s = default_shaped_settings();
        s.providers = providers;
        s.active_provider_id = active.to_string();
        s.ai_model = ai_model.to_string();
        s
    }

    #[test]
    fn resolve_prefix_overrides_active_provider() {
        let s = settings_with(
            vec![provider_named("default", "m-default"), provider_named("copilot", "m-copilot")],
            "default",
            "copilot/gpt-5.6-sol",
        );
        let (p, model) = s.resolve_active_selection();
        assert_eq!(p.id, "copilot");
        // provider.model "m-copilot" is concrete, so it wins over selector model
        assert_eq!(model, "m-copilot");
    }

    #[test]
    fn resolve_falls_back_to_selector_model_when_provider_auto() {
        let s = settings_with(
            vec![provider_named("copilot", "auto")],
            "copilot",
            "copilot/gpt-5.6-sol",
        );
        let (p, model) = s.resolve_active_selection();
        assert_eq!(p.id, "copilot");
        assert_eq!(model, "gpt-5.6-sol");
    }

    #[test]
    fn resolve_bare_model_used_as_fallback_on_active_provider() {
        let s = settings_with(
            vec![provider_named("copilot", "")],
            "copilot",
            "gpt-5.6-sol",
        );
        let (p, model) = s.resolve_active_selection();
        assert_eq!(p.id, "copilot");
        assert_eq!(model, "gpt-5.6-sol");
    }

    #[test]
    fn resolve_defaults_to_auto_when_nothing_concrete() {
        let s = settings_with(vec![provider_named("copilot", "auto")], "copilot", "auto");
        let (_p, model) = s.resolve_active_selection();
        assert_eq!(model, "auto");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib resolve_ 2>&1 | tail -20`
Expected: FAIL — `no method named 'resolve_active_selection'`.

- [ ] **Step 3: Implement helper + method**

Add the free helper right after `parse_model_selector`:

```rust
/// Return the first "concrete" model from `candidates`, treating empty and
/// "auto" (case-insensitive, trimmed) as not concrete. Falls back to "auto".
fn first_concrete_model(candidates: &[&str]) -> String {
    for c in candidates {
        let t = c.trim();
        if !t.is_empty() && !t.eq_ignore_ascii_case("auto") {
            return t.to_string();
        }
    }
    "auto".to_string()
}
```

Add the method inside `impl AppSettings`, immediately after `active_provider()` (after line 339):

```rust
    /// Resolve the effective provider and concrete model to send, honoring a
    /// `provider-id/model` prefix in the top-level `ai_model`. A valid prefix
    /// overrides `active_provider_id`; the model falls back from the
    /// provider's own `model` to the selector model to "auto".
    pub fn resolve_active_selection(&self) -> (Provider, String) {
        let ids: Vec<String> = self.providers.iter().map(|p| p.id.clone()).collect();
        let (prefix, selector_model) = parse_model_selector(&self.ai_model, &ids);

        let provider = prefix
            .and_then(|id| self.providers.iter().find(|p| p.id == id).cloned())
            .unwrap_or_else(|| self.active_provider());

        let effective = first_concrete_model(&[&provider.model, selector_model]);
        (provider, effective)
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib resolve_ first_concrete 2>&1 | tail -20`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/settings.rs
git commit --author="James Zhu <zhujian0805@gmail.com>" -m "feat(settings): resolve_active_selection with provider prefix and model fallback"
```

---

### Task 3: Wire into `AiClient::from_settings`

**Files:**
- Modify: `src-tauri/src/ai/client.rs:212-239`

- [ ] **Step 1: Update `from_settings` to use the resolved selection**

Replace the body of `from_settings` (lines 212-239). The only change is deriving `(provider, effective_model)` from `resolve_active_selection()` and overriding the provider's `model` before resolving:

```rust
    pub fn from_settings(settings: &crate::AppSettings) -> Self {
        let (mut provider, effective_model) = settings.resolve_active_selection();
        provider.model = effective_model;
        match crate::ai::provider::resolve_provider(&provider) {
            Ok(resolved) => Self::with_resolved(
                provider.base_url,
                resolved.chat_url,
                resolved.headers,
                resolved.model,
                settings.ai_timeout_secs,
                settings.ai_max_retry_attempts,
                settings.ai_retry_base_delay_ms,
            ),
            Err(err) => {
                log::warn!(
                    "failed to resolve active provider '{}': {err}",
                    provider.name
                );
                Self::with_retry(
                    provider.base_url,
                    provider.api_key,
                    provider.model,
                    settings.ai_timeout_secs,
                    settings.ai_max_retry_attempts,
                    settings.ai_retry_base_delay_ms,
                )
            }
        }
    }
```

- [ ] **Step 2: Verify it compiles and existing client tests pass**

Run: `cd src-tauri && cargo test --lib client 2>&1 | tail -20`
Expected: PASS (existing `client_tests` unaffected — they construct via `with_*`, not `from_settings`).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/ai/client.rs
git commit --author="James Zhu <zhujian0805@gmail.com>" -m "feat(ai): honor ai_model provider/model selector in AiClient::from_settings"
```

---

### Task 4: Full build, reinstall, and manual verification

**Files:** none (build + runtime check)

- [ ] **Step 1: Full library test run**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -25`
Expected: all tests PASS.

- [ ] **Step 2: Release build (updates the binary behind `~/.local/bin/ol`)**

Run: `cd src-tauri && cargo build --release 2>&1 | tail -15`
Expected: `Finished \`release\` profile`.

- [ ] **Step 3: Verify the stale-binary usage error is gone**

Run: `ol providers --help 2>&1 | head; echo '---'; ol providers caps 2>&1 | head`
Expected: help text and the three provider kinds; no "usage:" failure.

- [ ] **Step 4: Verify model selection against live settings**

The live `settings.json` has `active_provider_id: "copilot"`, provider `copilot` model `gpt-5.6-sol`, and `ai_model: "gpt-5.6-sol"`. Confirm the active model resolves correctly:

Run: `ol providers active 2>&1 | tail`
Expected: shows the `copilot` provider as active. (No crash; model line consistent with `gpt-5.6-sol`.)

- [ ] **Step 5: Commit any incidental changes**

Only if `git status` shows tracked changes (e.g. Cargo.lock):

```bash
git add -A && git commit --author="James Zhu <zhujian0805@gmail.com>" -m "chore: rebuild after provider/model selector" || echo "nothing to commit"
```

---

## Notes for the implementer

- `Provider` and `AppSettings` are both in `src-tauri/src/settings.rs`; tests in that file reference items via `super::`.
- `default_shaped_settings()` already exists in the tests module (line ~1433) — reuse it, don't redefine.
- Do not touch `active_provider()`, `sync_legacy_ai_fields_from_active_provider`, or any setter — UI-driven writes must keep working unchanged.
