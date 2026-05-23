use crate::ai::client::{AiClient, Message};
use crate::plugins::{PluginManager, QueryResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResponse {
    pub content: String,
    pub tools_used: Vec<String>,
    pub results: Vec<QueryResult>,
    pub is_ai: bool,
}

pub struct Router;

impl Router {
    /// Heuristic: is this natural language query?
    pub fn is_natural_language(input: &str) -> bool {
        let lower = input.to_lowercase();
        if input.len() > 20 {
            return true;
        }
        // Check for NL verbs/words combined with spaces
        if input.contains(' ') {
            let nl_words = ["find", "show", "open", "search", "what", "how", "why", "who",
                            "when", "where", "help", "get", "list", "create", "make",
                            "tell", "explain", "translate", "calculate", "convert",
                            "找", "帮", "搜", "查", "打开"];
            return nl_words.iter().any(|w| lower.contains(w));
        }
        false
    }

    pub async fn route(
        input: &str,
        plugin_manager: &PluginManager,
        ai_client: &AiClient,
    ) -> AiResponse {
        if Self::is_natural_language(input) {
            Self::ai_route(input, plugin_manager, ai_client).await
        } else {
            let results = plugin_manager.query_all(input).await;
            AiResponse {
                content: String::new(),
                tools_used: vec![],
                results,
                is_ai: false,
            }
        }
    }

    async fn ai_route(
        input: &str,
        plugin_manager: &PluginManager,
        ai_client: &AiClient,
    ) -> AiResponse {
        let tools = plugin_manager.all_tool_schemas();
        let messages = vec![
            Message {
                role: "system".to_string(),
                content: "You are OmniLauncher, an AI-powered application launcher. \
                          Help the user find files, search the web, launch apps, or answer questions. \
                          Use the available tools when appropriate. Be concise.".to_string(),
            },
            Message {
                role: "user".to_string(),
                content: input.to_string(),
            },
        ];

        match ai_client.chat_with_tools(messages, tools).await {
            Ok(resp) => {
                let mut tools_used = vec![];
                let mut tool_results = vec![];
                let mut final_content = resp.content.clone().unwrap_or_default();

                if let Some(tool_calls) = resp.tool_calls {
                    for tc in &tool_calls {
                        tools_used.push(tc.function.name.clone());
                        let args: serde_json::Value =
                            serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                        let result = plugin_manager
                            .execute_tool(&tc.function.name, args)
                            .await;
                        tool_results.push(result);
                    }

                    // If we have tool results but no content, summarize
                    if final_content.is_empty() && !tool_results.is_empty() {
                        final_content = tool_results.join("\n\n");
                    }
                }

                AiResponse {
                    content: final_content,
                    tools_used,
                    results: vec![],
                    is_ai: true,
                }
            }
            Err(e) => AiResponse {
                content: format!("AI error: {}", e),
                tools_used: vec![],
                results: vec![],
                is_ai: true,
            },
        }
    }
}
