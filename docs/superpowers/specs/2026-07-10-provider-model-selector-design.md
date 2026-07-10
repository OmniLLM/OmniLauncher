# Top-level `ai_model` as `provider/model` Selector

Date: 2026-07-10
Status: Approved

## Problem

After the provider-picker work (`2026-07-09-provider-picker-model-selection-design.md`),
`AppSettings` holds a list of `providers` plus an `active_provider_id`, and
each `Provider` has its own `model`. The legacy top-level `ai_model` field is
kept as a compatibility shim, synchronized *from* the active provider by
`sync_legacy_ai_fields_from_active_provider`.

Today the top-level `ai_model` is purely derived output — it never drives
selection. A user editing `settings.json` by hand cannot use it to say "use
this provider and this model" in one place, and a provider whose `model` is
empty or `"auto"` gives the endpoint no concrete model to run.

This design makes the top-level `ai_model` an **input selector**: it can name
a provider *and* a model, and it acts as the fallback model when a provider's
own `model` is unset.

## Goals

- Let top-level `ai_model` optionally encode `provider-id/model`.
  - `ai_model: "copilot/gpt-5.6-sol"` → select provider `copilot`, model
    `gpt-5.6-sol`.
  - `ai_model: "gpt-5.6-sol"` (no known provider prefix) → bare model on the
    provider chosen by `active_provider_id`.
- When the selected provider's own `model` is empty or `"auto"`, fall back to
  the selector model derived from `ai_model`.
- A valid `provider-id/` prefix **overrides** `active_provider_id` for the
  purpose of choosing which provider is used.

## Non-goals

- No change to how providers are stored, added, or authenticated.
- No new settings field. The selector rides entirely on the existing
  top-level `ai_model` string.
- No frontend change. This is a backend resolution refinement; the desktop
  UI continues to drive selection through `active_provider_id` +
  per-provider `model`, and writing those keeps working because the legacy
  sync still runs.

## Key ambiguity: slashes inside model names

Model names legitimately contain `/` (the live install has a Custom provider
whose model is `jzhu/gpt-5.6-luna`). Naive slash-splitting would
misinterpret `jzhu/gpt-5.6-luna` as "provider `jzhu`, model
`gpt-5.6-luna`".

**Resolution rule:** split on the *first* `/` only, and treat the left part
as a provider prefix **only if it exactly matches a known provider id**.
Since no provider has id `jzhu`, `jzhu/gpt-5.6-luna` is correctly treated as
a bare model string. This keeps existing configs working with zero change.

## Design

Two small, pure, independently-testable pieces in `settings.rs`.

### 1. `parse_model_selector`

```rust
/// Split a top-level `ai_model` string into an optional provider id and a
/// model string. The provider id is only recognized when the substring
/// before the FIRST '/' exactly matches one of `known_provider_ids`;
/// otherwise the whole string is returned as the model with no provider.
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

Edge cases:
- Empty `ai_model` → `(None, "")`.
- `"copilot/"` (prefix match, empty model) → `(Some("copilot"), "")` — the
  empty model then triggers the fallback ladder below.

### 2. `resolve_active_selection`

```rust
/// Resolve the effective provider and the concrete model to send, honoring
/// a `provider-id/model` prefix in the top-level `ai_model`.
pub fn resolve_active_selection(&self) -> (Provider, String) {
    let ids: Vec<String> = self.providers.iter().map(|p| p.id.clone()).collect();
    let (prefix, selector_model) = parse_model_selector(&self.ai_model, &ids);

    // Provider: a valid prefix wins over active_provider_id.
    let provider = match prefix {
        Some(id) => self.providers.iter().find(|p| p.id == id).cloned(),
        None => None,
    }
    .unwrap_or_else(|| self.active_provider());

    // Effective model: provider.model unless empty/"auto", else the selector
    // model unless empty/"auto", else "auto".
    let effective = first_concrete_model(&[&provider.model, selector_model]);
    (provider, effective)
}
```

where a small helper treats `""` and `"auto"` (case-insensitive, trimmed) as
"not concrete" and returns the first concrete candidate, defaulting to
`"auto"`.

### Wiring

- `AiClient::from_settings` (client.rs:212) calls
  `settings.resolve_active_selection()` instead of `settings.active_provider()`.
  It builds a `Provider` value whose `model` is replaced by the resolved
  effective model, then passes that to `resolve_provider`. All downstream
  code (`resolve_provider`, header building, chat URL) is unchanged because
  it still receives a `Provider` — only its `model` field is now the resolved
  value.

The existing `active_provider()`, `sync_legacy_ai_fields_from_active_provider`,
and all setters are left intact so UI-driven writes keep behaving as before.

## Precedence summary

1. Valid `provider-id/` prefix in `ai_model` → that provider.
2. Otherwise `active_provider_id`.
3. Otherwise first provider, then legacy `ai_*` fallback (unchanged
   `active_provider()` behavior).

For the model:
1. Provider's own `model` if concrete (not empty/`auto`).
2. Otherwise the selector model from `ai_model`.
3. Otherwise `"auto"`.

## Rebuild

The `~/.local/bin/ol` symlink points at
`src-tauri/target/release/omnilauncher`. The currently-installed binary
predates commit `6c4c03c` (interactive `providers add`), which is why
`ol providers add` prints the old usage string. Rebuilding `--release` after
this change ships both the interactive-add command and this selector feature.

## Testing

- `parse_model_selector`:
  - prefix matches a known id → `(Some(id), model)`.
  - prefix does not match → `(None, whole_string)`.
  - model containing `/` but no matching prefix (`jzhu/gpt-5.6-luna`) →
    `(None, "jzhu/gpt-5.6-luna")`.
  - empty string → `(None, "")`.
  - `"copilot/"` → `(Some("copilot"), "")`.
- `resolve_active_selection`:
  - prefix overrides `active_provider_id`.
  - provider with concrete `model` ignores the selector model.
  - provider with empty/`auto` `model` falls back to the selector model.
  - both unset → `"auto"`.

## References

- Prior design: `docs/superpowers/specs/2026-07-09-provider-picker-model-selection-design.md`.
- Code: `src-tauri/src/settings.rs` (`active_provider`,
  `sync_legacy_ai_fields_from_active_provider`),
  `src-tauri/src/ai/client.rs` (`from_settings`),
  `src-tauri/src/ai/provider.rs` (`resolve_provider`).
- Sibling reference: `~/repos/omni-pilot` provider handling.
