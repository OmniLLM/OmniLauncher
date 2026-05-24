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

/// Multi-turn conversation context.
pub struct ConversationContext {
    pub messages: Vec<Message>,
    pub max_turns: usize,
}

impl Default for ConversationContext {
    fn default() -> Self {
        Self { messages: Vec::new(), max_turns: 10 }
    }
}

impl ConversationContext {
    pub fn new(max_turns: usize) -> Self {
        Self { messages: Vec::new(), max_turns }
    }

    pub fn add_user(&mut self, text: &str) {
        self.messages.push(Message { role: "user".to_string(), content: text.to_string() });
        self.trim_to_max();
    }

    pub fn add_assistant(&mut self, text: &str) {
        self.messages.push(Message { role: "assistant".to_string(), content: text.to_string() });
    }

    pub fn add_tool_result(&mut self, tool_name: &str, result: &str) {
        self.messages.push(Message {
            role: "tool".to_string(),
            content: format!("[{}]: {}", tool_name, result),
        });
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Keep last max_turns pairs (user + assistant = 1 turn).
    pub fn trim_to_max(&mut self) {
        let max_messages = self.max_turns * 2;
        if self.messages.len() > max_messages {
            let excess = self.messages.len() - max_messages;
            self.messages.drain(0..excess);
        }
    }

    pub fn get_messages_with_system(&self, system_prompt: &str) -> Vec<Message> {
        let mut msgs = vec![Message { role: "system".to_string(), content: system_prompt.to_string() }];
        msgs.extend(self.messages.clone());
        msgs
    }
}

pub struct Router;

impl Router {
    /// Improved heuristic: is this natural language query?
    pub fn is_natural_language(input: &str) -> bool {
        let words: Vec<&str> = input.split_whitespace().collect();
        if words.len() == 1 {
            return false;
        }

        // Contains question words or action words
        let question_words = ["what", "how", "why", "when", "where", "who",
                              "find", "show", "open", "search", "list", "get",
                              "帮", "找", "打开", "搜索", "显示", "什么", "怎么"];
        let lower = input.to_lowercase();
        if question_words.iter().any(|w| lower.contains(w)) {
            return true;
        }

        // Long enough to be sentence-like
        if words.len() >= 4 {
            return true;
        }

        // Punctuation typical of questions
        if input.contains('?') || input.contains('？') {
            return true;
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
