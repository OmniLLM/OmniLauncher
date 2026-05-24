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

/// How a query should be dispatched.
#[derive(Debug, Clone, PartialEq)]
pub enum RouteDecision {
    /// Run local plugins only — instant, no AI.
    Local,
    /// User explicitly asked for AI (prefix `?` or `ai ` or `Ctrl+Enter`).
    Ai,
}

pub struct Router;

impl Router {
    /// Decide routing purely from the query text.
    ///
    /// Rules (in priority order):
    /// 1. Starts with `?` or `ai ` (case-insensitive) → AI
    /// 2. Everything else → Local
    ///
    /// This keeps the fast path (app launch, calculator, file search, shell,
    /// web search, clipboard) completely free of AI latency. Users who want
    /// AI assistance opt in explicitly.
    pub fn decide(input: &str) -> RouteDecision {
        let trimmed = input.trim();

        // Explicit AI prefix triggers
        if trimmed.starts_with('?')
            || trimmed.to_lowercase().starts_with("ai ")
        {
            return RouteDecision::Ai;
        }

        RouteDecision::Local
    }

    /// Strip the AI trigger prefix so the underlying prompt is clean.
    pub fn strip_ai_prefix(input: &str) -> &str {
        let trimmed = input.trim();
        if trimmed.starts_with('?') {
            trimmed[1..].trim()
        } else if trimmed.len() >= 3 && trimmed[..3].to_lowercase() == "ai " {
            trimmed[3..].trim()
        } else {
            trimmed
        }
    }

    /// Main entry-point: route a query and return a response.
    pub async fn route(
        input: &str,
        plugin_manager: &PluginManager,
        ai_client: &AiClient,
    ) -> AiResponse {
        match Self::decide(input) {
            RouteDecision::Local => {
                let results = plugin_manager.query_all(input).await;
                AiResponse {
                    content: String::new(),
                    tools_used: vec![],
                    results,
                    is_ai: false,
                }
            }
            RouteDecision::Ai => {
                let prompt = Self::strip_ai_prefix(input);
                Self::ai_route(prompt, plugin_manager, ai_client).await
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_routes() {
        assert_eq!(Router::decide("chrome"), RouteDecision::Local);
        assert_eq!(Router::decide("=2+2"), RouteDecision::Local);
        assert_eq!(Router::decide(">git status"), RouteDecision::Local);
        assert_eq!(Router::decide("find my notes"), RouteDecision::Local);
        assert_eq!(Router::decide("open firefox"), RouteDecision::Local);
        assert_eq!(Router::decide("what is rust"), RouteDecision::Local);
    }

    #[test]
    fn test_ai_routes() {
        assert_eq!(Router::decide("?what is the weather"), RouteDecision::Ai);
        assert_eq!(Router::decide("? summarize my clipboard"), RouteDecision::Ai);
        assert_eq!(Router::decide("ai help me write an email"), RouteDecision::Ai);
        assert_eq!(Router::decide("AI explain this error"), RouteDecision::Ai);
    }

    #[test]
    fn test_strip_prefix() {
        assert_eq!(Router::strip_ai_prefix("?hello"), "hello");
        assert_eq!(Router::strip_ai_prefix("? hello"), "hello");
        assert_eq!(Router::strip_ai_prefix("ai help me"), "help me");
        assert_eq!(Router::strip_ai_prefix("AI help me"), "help me");
        assert_eq!(Router::strip_ai_prefix("chrome"), "chrome");
    }
}
