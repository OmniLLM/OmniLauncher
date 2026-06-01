/// Vision Analyze Plugin
///
/// Capture a screen region (or full screen) and send the image to a vision-capable
/// LLM for analysis.
///
/// Commands:
///   `vq <prompt>`          — capture region, ask AI the question
///   `vision <prompt>`      — same
///   `vq`                   — capture region, ask "Describe what you see"
///
/// On execute:
///   action_type = "vision_analyze"
///   action_data = "<prompt>" (empty → default describe prompt)
///
/// The actual screenshot + API call is handled in main.rs `vision_analyze` command
/// because it needs access to AppState (ai_client + settings).
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct VisionAnalyzePlugin;

#[async_trait]
impl Plugin for VisionAnalyzePlugin {
    fn name(&self) -> &str {
        "vision_analyze"
    }

    fn description(&self) -> &str {
        "Capture a screen region and analyze it with AI (type 'vq <question>')"
    }

    fn keyword(&self) -> Option<&str> {
        None
    }

    fn cheap_prefix_match(&self, raw: &str) -> bool {
        let lower = raw.trim().to_lowercase();
        lower.starts_with("vision ")
            || lower.starts_with("vq ")
            || lower == "vq"
            || lower == "vision"
            || lower == "视觉"
            || lower == "截图分析"
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = q.raw.trim();
        let lower = raw.to_lowercase();

        // Match: "vq", "vq <prompt>", "vision", "vision <prompt>"
        let prompt = if let Some(rest) = lower
            .strip_prefix("vision ")
            .or_else(|| lower.strip_prefix("vq "))
        {
            rest.trim().to_string()
        } else if lower == "vq" || lower == "vision" || lower == "视觉" || lower == "截图分析"
        {
            String::new()
        } else {
            return vec![];
        };

        // Preserve original-case prompt from raw input
        let original_prompt = if prompt.is_empty() {
            String::new()
        } else {
            let prefix_len = if raw.to_lowercase().starts_with("vision ") {
                7
            } else {
                3 // "vq "
            };
            raw.chars()
                .skip(prefix_len)
                .collect::<String>()
                .trim()
                .to_string()
        };

        let display_prompt = if original_prompt.is_empty() {
            "Describe what you see".to_string()
        } else {
            original_prompt.clone()
        };

        vec![QueryResult {
            id: "vision:analyze".to_string(),
            title: format!("👁 Vision: {}", truncate(&display_prompt, 50)),
            subtitle: Some("Select a screen region → AI analysis".to_string()),
            icon: Some("👁".to_string()),
            score: 100,
            action_type: "vision_analyze".to_string(),
            action_data: original_prompt,
            source: None,
        }]
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{}…", t)
    }
}
