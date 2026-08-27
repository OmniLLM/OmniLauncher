//! GitHub Copilot model → API shape mapping and per-model parameter quirks.
//!
//! Ported from the sibling projects `omni-pilot`
//! (`src/background/copilot-model-shapes.mjs`) and `omnillm`
//! (`internal/providers/copilot/{shape,payload}.go`).
//!
//! Copilot serves chat-style traffic through two request shapes:
//!   * OpenAI Chat Completions — `POST /chat/completions`
//!   * OpenAI Responses        — `POST /responses`
//!
//! Each model is served by one or both. Copilot's `GET /models` response
//! includes a `supported_endpoints` array that is the ground truth. Sending a
//! request to the wrong endpoint yields:
//!
//! ```text
//! HTTP 400 { "error": { "code": "unsupported_api_for_model",
//!            "message": "model \"gpt-5.5\" is not accessible via the
//!                        /chat/completions endpoint" } }
//! ```
//!
//! The static [`COPILOT_MODEL_SHAPES`] table is a snapshot of that metadata so
//! we route correctly on the first request. Unknown models fall back to a
//! family heuristic mirroring omnillm's `IsGPT5Family` rule.

use std::collections::HashMap;
use std::sync::LazyLock;

/// The request shape a Copilot model expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopilotShape {
    /// `POST /chat/completions` (OpenAI Chat Completions).
    Chat,
    /// `POST /responses` (OpenAI Responses).
    Responses,
}

/// Snapshot of Copilot's `/models` `supported_endpoints`, taken 2026-07-07.
/// Keys are lowercase model IDs. Models that support both endpoints are mapped
/// to `Chat` (preferred for request-builder compatibility); responses-only
/// models are mapped to `Responses`.
static COPILOT_MODEL_SHAPES: LazyLock<HashMap<&'static str, CopilotShape>> = LazyLock::new(|| {
    use CopilotShape::{Chat, Responses};
    HashMap::from([
        // Anthropic models on Copilot — chat-completions compatible
        ("claude-opus-4.6", Chat),
        ("claude-opus-4.7", Chat),
        ("claude-opus-4.8", Chat),
        ("claude-sonnet-4.5", Chat),
        ("claude-sonnet-4.6", Chat),
        ("claude-sonnet-5", Chat),
        ("claude-haiku-4.5", Chat),
        // Google models on Copilot
        ("gemini-2.5-pro", Chat),
        ("gemini-3-flash-preview", Chat),
        ("gemini-3.1-pro-preview", Chat),
        ("gemini-3.5-flash", Chat),
        // OpenAI GPT-5 family on Copilot
        ("gpt-5.4", Chat),    // supports both; prefer chat
        ("gpt-5-mini", Chat), // supports both; prefer chat
        ("gpt-5.3-codex", Responses),
        ("gpt-5.4-mini", Responses),
        ("gpt-5.5", Responses),
        // Microsoft AI models on Copilot — responses-only
        ("mai-code-1-flash-picker", Responses),
        // Classic OpenAI models — chat-completions
        ("gpt-3.5-turbo", Chat),
        ("gpt-3.5-turbo-0613", Chat),
        ("gpt-4", Chat),
        ("gpt-4-0125-preview", Chat),
        ("gpt-4-0613", Chat),
        ("gpt-4-o-preview", Chat),
        ("gpt-4.1", Chat),
        ("gpt-4.1-2025-04-14", Chat),
        ("gpt-41-copilot", Chat),
        ("gpt-4o", Chat),
        ("gpt-4o-2024-05-13", Chat),
        ("gpt-4o-2024-08-06", Chat),
        ("gpt-4o-2024-11-20", Chat),
        ("gpt-4o-mini", Chat),
        ("gpt-4o-mini-2024-07-18", Chat),
        // Utility
        ("trajectory-compaction", Chat),
    ])
});

/// True when the model is a member of the GPT-5 family based on Copilot's
/// naming scheme (`gpt-5`, `gpt-5.4`, `gpt-5-mini`, `gpt-5.3-codex`, `gpt-5o`).
/// Mirrors omnillm's `IsGPT5Family`.
pub fn is_gpt5_family(model: &str) -> bool {
    let m = model.trim().to_ascii_lowercase();
    // `gpt-5` optionally followed by `.`, `-`, `o`, or end-of-string.
    if let Some(rest) = m.strip_prefix("gpt-5") {
        rest.is_empty() || rest.starts_with('.') || rest.starts_with('-') || rest.starts_with('o')
    } else {
        false
    }
}

/// True for reasoning-family models (o1/o3/o4/gpt-5*) whose chat-completions
/// body needs `max_completion_tokens` instead of `max_tokens` and which reject
/// `temperature` / `top_p`.
pub fn is_reasoning_model(model: &str) -> bool {
    let lower = model.trim().to_ascii_lowercase();
    let o_series = ["o1", "o3", "o4"].iter().any(|p| {
        lower
            .strip_prefix(*p)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('-') || rest.starts_with('.'))
    });
    o_series || is_gpt5_family(&lower)
}

/// True when the chat-completions body should send `max_completion_tokens`
/// rather than `max_tokens`.
pub fn uses_max_completion_tokens(model: &str) -> bool {
    is_reasoning_model(model)
}

/// Choose the request shape for a Copilot model.
///
/// Priority:
///   1. `force_chat_completions` override → `Chat`.
///   2. Exact map lookup (case-insensitive) — ground truth from `/models`.
///   3. Family heuristic — GPT-5 family (except `-mini`) and `mai-code-*` →
///      `Responses`; everything else → `Chat`.
pub fn select_shape(model: &str, force_chat_completions: bool) -> CopilotShape {
    if force_chat_completions {
        return CopilotShape::Chat;
    }
    let key = model.trim().to_ascii_lowercase();
    if let Some(shape) = COPILOT_MODEL_SHAPES.get(key.as_str()) {
        return *shape;
    }
    if key.starts_with("mai-code-") {
        return CopilotShape::Responses;
    }
    if is_gpt5_family(&key) && !key.contains("-mini") {
        return CopilotShape::Responses;
    }
    CopilotShape::Chat
}

/// Detect Copilot's `model_not_supported` 400 — the model is not available on
/// this account/plan (or the id is stale), so it works on *neither*
/// `/chat/completions` nor `/responses`.
///
/// This is distinct from [`is_unsupported_chat_completions_error`]
/// (`unsupported_api_for_model`), which only means the *endpoint* is wrong and
/// a `/responses` retry will succeed. `model_not_supported` must NOT trigger an
/// endpoint reroute — that just produces a second identical 400. Instead the
/// caller surfaces an actionable "pick a model your plan supports" error.
///
/// Live example (`api.githubcopilot.com`, model `claude-opus-4.8`):
/// ```text
/// HTTP 400 { "error": { "message": "The requested model is not supported.",
///            "code": "model_not_supported", "param": "model",
///            "type": "invalid_request_error" } }
/// ```
pub fn is_model_not_supported_error(status: u16, body: &str) -> bool {
    if status != 400 {
        return false;
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if v["error"]["code"].as_str() == Some("model_not_supported") {
            return true;
        }
    }
    body.to_ascii_lowercase().contains("model_not_supported")
}

/// Detect Copilot's `unsupported_api_for_model` 400 so a `/chat/completions`
/// request can transparently fall back to `/responses`. Mirrors omnillm's
/// `isUnsupportedChatCompletionsModel`.
pub fn is_unsupported_chat_completions_error(status: u16, body: &str) -> bool {
    if status != 400 {
        return false;
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        let err = &v["error"];
        if err["code"].as_str() == Some("unsupported_api_for_model") {
            return true;
        }
        if let Some(msg) = err["message"].as_str() {
            if msg.to_ascii_lowercase().contains("/chat/completions") {
                return true;
            }
        }
    }
    body.to_ascii_lowercase()
        .contains("unsupported_api_for_model")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_map_hits() {
        assert_eq!(select_shape("gpt-4o", false), CopilotShape::Chat);
        assert_eq!(select_shape("GPT-4O", false), CopilotShape::Chat);
        assert_eq!(select_shape("gpt-5.5", false), CopilotShape::Responses);
        assert_eq!(select_shape("gpt-5.4-mini", false), CopilotShape::Responses);
        assert_eq!(select_shape("gpt-5.4", false), CopilotShape::Chat);
        assert_eq!(select_shape("gpt-5-mini", false), CopilotShape::Chat);
    }

    #[test]
    fn shape_heuristic_on_cache_miss() {
        // Unknown gpt-5 family → responses (except -mini).
        assert_eq!(select_shape("gpt-5-codex", false), CopilotShape::Responses);
        assert_eq!(
            select_shape("gpt-5.9-turbo", false),
            CopilotShape::Responses
        );
        assert_eq!(select_shape("gpt-5.9-mini", false), CopilotShape::Chat);
        // Unknown mai-code-* → responses.
        assert_eq!(
            select_shape("mai-code-anything", false),
            CopilotShape::Responses
        );
        // Unknown non-family → chat.
        assert_eq!(select_shape("claude-sonnet-9", false), CopilotShape::Chat);
        assert_eq!(select_shape("some-new-model", false), CopilotShape::Chat);
    }

    #[test]
    fn force_chat_overrides() {
        assert_eq!(select_shape("gpt-5.5", true), CopilotShape::Chat);
    }

    #[test]
    fn gpt5_family_detection() {
        for m in ["gpt-5", "gpt-5.4", "gpt-5-mini", "gpt-5.3-codex", "gpt-5o"] {
            assert!(is_gpt5_family(m), "{m} should be gpt-5 family");
        }
        for m in ["gpt-4o", "gpt-4.1", "gpt-50", "claude-sonnet-5"] {
            assert!(!is_gpt5_family(m), "{m} should NOT be gpt-5 family");
        }
    }

    #[test]
    fn reasoning_model_detection() {
        for m in ["o1", "o3-mini", "o4", "gpt-5", "gpt-5.4"] {
            assert!(is_reasoning_model(m), "{m} should be reasoning");
            assert!(uses_max_completion_tokens(m));
        }
        for m in ["gpt-4o", "claude-opus-4.6", "o2", "gemini-2.5-pro"] {
            assert!(!is_reasoning_model(m), "{m} should NOT be reasoning");
        }
    }

    #[test]
    fn model_not_supported_detection() {
        // The live shape from api.githubcopilot.com for an unavailable model.
        assert!(is_model_not_supported_error(
            400,
            r#"{"error":{"message":"The requested model is not supported.","code":"model_not_supported","param":"model","type":"invalid_request_error"}}"#
        ));
        // Raw substring fallback.
        assert!(is_model_not_supported_error(400, "model_not_supported"));
        // Wrong status → not this error.
        assert!(!is_model_not_supported_error(
            500,
            r#"{"error":{"code":"model_not_supported"}}"#
        ));
        // A different 400 (wrong endpoint) must NOT be misread as model_not_supported.
        assert!(!is_model_not_supported_error(
            400,
            r#"{"error":{"code":"unsupported_api_for_model"}}"#
        ));
    }

    #[test]
    fn model_not_supported_and_unsupported_api_are_disjoint() {
        let not_supported = r#"{"error":{"code":"model_not_supported","message":"nope"}}"#;
        let wrong_endpoint =
            r#"{"error":{"code":"unsupported_api_for_model","message":"use /responses"}}"#;
        assert!(is_model_not_supported_error(400, not_supported));
        assert!(!is_unsupported_chat_completions_error(400, not_supported));
        assert!(is_unsupported_chat_completions_error(400, wrong_endpoint));
        assert!(!is_model_not_supported_error(400, wrong_endpoint));
    }

    #[test]
    fn unsupported_error_detection() {
        assert!(is_unsupported_chat_completions_error(
            400,
            r#"{"error":{"code":"unsupported_api_for_model","message":"nope"}}"#
        ));
        assert!(is_unsupported_chat_completions_error(
            400,
            r#"{"error":{"message":"use the /chat/completions endpoint"}}"#
        ));
        assert!(is_unsupported_chat_completions_error(
            400,
            "raw unsupported_api_for_model text"
        ));
        assert!(!is_unsupported_chat_completions_error(
            400,
            "some other 400"
        ));
        assert!(!is_unsupported_chat_completions_error(
            500,
            r#"{"error":{"code":"unsupported_api_for_model"}}"#
        ));
    }
}
