use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct TranslatePlugin;

#[async_trait]
impl Plugin for TranslatePlugin {
    fn name(&self) -> &str {
        "translate"
    }

    fn description(&self) -> &str {
        "Translate text to another language (AI-assisted)"
    }

    fn keyword(&self) -> Option<&str> {
        None
    }

    async fn query(&self, _q: &Query) -> Vec<QueryResult> {
        // This plugin only responds to AI tool calls, not direct queries
        vec![]
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "translate_text",
                "description": "Translate text from one language to another",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "The text to translate"
                        },
                        "target_language": {
                            "type": "string",
                            "description": "The target language (e.g. Spanish, French, Chinese, German)"
                        }
                    },
                    "required": ["text", "target_language"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let text = args["text"].as_str().unwrap_or("");
        let lang = args["target_language"].as_str().unwrap_or("");
        format!("Please translate the following to {}: {}", lang, text)
    }
}