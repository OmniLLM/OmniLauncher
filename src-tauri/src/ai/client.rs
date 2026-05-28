#[cfg(not(test))]
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
#[cfg(not(test))]
use tauri::Emitter;

use crate::ai::errors::{classify_ai_error, AiError, ErrorClass};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// For assistant messages that include tool calls
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// For tool result messages (role="tool")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool name for tool result messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    pub fn system(content: &str) -> Self {
        Self {
            role: "system".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }
    pub fn user(content: &str) -> Self {
        Self {
            role: "user".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }
    pub fn assistant(content: &str) -> Self {
        Self {
            role: "assistant".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }
    pub fn assistant_tool_calls(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".into(),
            content,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
            name: None,
        }
    }
    pub fn tool_result(call_id: &str, name: &str, result: &str) -> Self {
        Self {
            role: "tool".into(),
            content: Some(result.into()),
            tool_calls: None,
            tool_call_id: Some(call_id.into()),
            name: Some(name.into()),
        }
    }
    /// Helper to get content as &str
    pub fn content_str(&self) -> &str {
        self.content.as_deref().unwrap_or("")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub call_type: Option<String>,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

pub struct AiClient {
    base_url: String,
    api_key: String,
    model: String,
}

impl AiClient {
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        Self {
            base_url,
            api_key,
            model,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
    pub fn model(&self) -> &str {
        &self.model
    }

    fn build_client(&self) -> Result<reqwest::Client, AiError> {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| AiError::Transport(e.to_string()))
    }

    pub async fn chat(&self, messages: Vec<Message>) -> Result<String, AiError> {
        let resp = self.chat_with_tools(messages, vec![]).await?;
        Ok(resp.content.unwrap_or_default())
    }

    /// Wrapper with retry logic (max 3 attempts, 2 s base delay + XOR-shift jitter).
    ///
    /// Retries on: transient errors (timeout, transport, 429, 502, 503).
    /// Does NOT retry on permanent errors (auth, bad request, etc.).
    pub async fn chat_with_tools(
        &self,
        messages: Vec<Message>,
        tools: Vec<serde_json::Value>,
    ) -> Result<ChatResponse, AiError> {
        const MAX_ATTEMPTS: u32 = 3;
        const BASE_DELAY_MS: u64 = 2_000;

        let mut last_err: Option<AiError> = None;

        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                let backoff_ms = BASE_DELAY_MS * (1u64 << (attempt - 1));
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() as u64;
                let jitter_ms = seed.wrapping_mul(0x9e3779b97f4a7c15).wrapping_add(attempt as u64) % 1_000;
                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms + jitter_ms)).await;
            }

            match self
                .chat_with_tools_once(messages.clone(), tools.clone())
                .await
            {
                Ok(resp) => return Ok(resp),
                Err(e) => match classify_ai_error(&e) {
                    ErrorClass::Transient => {
                        last_err = Some(e);
                    }
                    _ => return Err(e),
                },
            }
        }

        Err(last_err.unwrap_or(AiError::Transport(
            "max retries exhausted".into(),
        )))
    }

    /// Single (non-retrying) API call — used internally by `chat_with_tools`.
    async fn chat_with_tools_once(
        &self,
        messages: Vec<Message>,
        tools: Vec<serde_json::Value>,
    ) -> Result<ChatResponse, AiError> {
        let client = self.build_client()?;

        let api_messages: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                let mut msg = serde_json::json!({ "role": m.role });

                // Content
                match &m.content {
                    Some(c) => msg["content"] = serde_json::json!(c),
                    None => msg["content"] = serde_json::Value::Null,
                }

                // Tool calls on assistant messages
                if let Some(ref tcs) = m.tool_calls {
                    let tc_json: Vec<serde_json::Value> = tcs
                        .iter()
                        .map(|tc| {
                            serde_json::json!({
                                "id": tc.id,
                                "type": "function",
                                "function": { "name": tc.function.name, "arguments": tc.function.arguments }
                            })
                        })
                        .collect();
                    msg["tool_calls"] = serde_json::json!(tc_json);
                }

                // Tool result fields
                if let Some(ref id) = m.tool_call_id {
                    msg["tool_call_id"] = serde_json::json!(id);
                }
                if let Some(ref name) = m.name {
                    msg["name"] = serde_json::json!(name);
                }

                msg
            })
            .collect();

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": api_messages,
        });

        if !tools.is_empty() {
            body["tools"] = serde_json::json!(tools);
            body["tool_choice"] = serde_json::json!("auto");
        }

        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );

        let mut req = client.post(&url).json(&body);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }

        let response = req
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AiError::Timeout
                } else {
                    AiError::Transport(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(AiError::Api { status, body });
        }

        let json: serde_json::Value =
            response.json().await.map_err(|e| AiError::Json(e.to_string()))?;

        let choice = &json["choices"][0]["message"];
        let content = choice["content"].as_str().map(|s| s.to_string());

        let tool_calls = choice["tool_calls"].as_array().map(|tcs| {
            tcs.iter()
                .filter_map(|tc| {
                    Some(ToolCall {
                        id: tc["id"].as_str()?.to_string(),
                        call_type: Some("function".to_string()),
                        function: FunctionCall {
                            name: tc["function"]["name"].as_str()?.to_string(),
                            arguments: tc["function"]["arguments"].as_str()?.to_string(),
                        },
                    })
                })
                .collect()
        });

        Ok(ChatResponse {
            content,
            tool_calls,
        })
    }

    /// Stream chat completions and emit Tauri events for each chunk.
    /// In test mode this is a no-op stub.
    #[cfg(not(test))]
    pub async fn chat_stream(
        &self,
        messages: Vec<Message>,
        window: tauri::WebviewWindow,
    ) -> Result<(), AiError> {
        let client = self.build_client()?;

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages.iter().map(|m| serde_json::json!({
                "role": m.role,
                "content": m.content_str()
            })).collect::<Vec<_>>(),
            "stream": true
        });

        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );

        let mut req = client.post(&url).json(&body);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }

        let response = req
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AiError::Timeout
                } else {
                    AiError::Transport(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(AiError::Api { status, body });
        }

        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| AiError::Transport(e.to_string()))?;
            let text = String::from_utf8_lossy(&bytes);
            for line in text.lines() {
                let line = line.trim();
                if line == "data: [DONE]" {
                    break;
                }
                if let Some(json_str) = line.strip_prefix("data: ") {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                        // Check for tool call
                        if let Some(tool_name) =
                            val["choices"][0]["delta"]["tool_calls"][0]["function"]["name"].as_str()
                        {
                            let _ = window.emit("ai-tool-call", tool_name.to_string());
                        }
                        // Text delta
                        if let Some(delta) = val["choices"][0]["delta"]["content"].as_str() {
                            if !delta.is_empty() {
                                let _ = window.emit("ai-stream", delta.to_string());
                            }
                        }
                    }
                }
            }
        }
        let _ = window.emit("ai-stream-done", "".to_string());
        Ok(())
    }

    #[cfg(test)]
    pub async fn chat_stream(
        &self,
        _messages: Vec<Message>,
        _window_placeholder: (),
    ) -> Result<(), AiError> {
        Ok(())
    }
}

#[cfg(test)]
mod client_tests {
    use super::*;
    use crate::ai::errors::AiError;

    fn make_client() -> AiClient {
        AiClient::new(
            "http://localhost:9999".into(),
            "test-key".into(),
            "test-model".into(),
        )
    }

    #[test]
    fn test_client_accessors_exist() {
        let c = make_client();
        assert_eq!(c.base_url(), "http://localhost:9999");
        assert_eq!(c.model(), "test-model");
    }

    #[tokio::test]
    async fn test_chat_returns_ai_error_on_connection_refused() {
        let c = make_client();
        let result = c.chat(vec![Message::user("hello")]).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AiError::Transport(_) | AiError::Timeout => {}
            other => panic!("Expected Transport or Timeout, got {:?}", other),
        }
    }
}