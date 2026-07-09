use serde::{Deserialize, Serialize};

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
    /// OpenAI / vLLM / OpenLLM finish_reason for the assistant turn.
    /// Used by the agent loop to detect hard truncation (`"length"`),
    /// the explicit `"tool_calls"` stop, or a normal `"stop"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// OpenAI-style `tool_choice` directive. Mirrors the spec values the model
/// providers support (`auto` / `none` / `required`); see the OpenAI Chat
/// Completions reference. The agent loop normally uses `Auto`; it escalates
/// to `Required` once per turn when the model returned text-only despite
/// the task being mid-flight — a principled alternative to inspecting the
/// model's text for "preamble" phrases.
#[derive(Debug, Clone, Copy)]
pub enum ToolChoice {
    Auto,
    None,
    Required,
}

impl ToolChoice {
    fn as_api_value(self) -> &'static str {
        match self {
            ToolChoice::Auto => "auto",
            ToolChoice::None => "none",
            ToolChoice::Required => "required",
        }
    }
}

const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 120;
const DEFAULT_MAX_RETRY_ATTEMPTS: u32 = 3;
const DEFAULT_RETRY_BASE_DELAY_MS: u64 = 2_000;
const MAX_ALLOWED_RETRY_ATTEMPTS: u32 = 30;

pub struct AiClient {
    base_url: String,
    chat_url: String,
    headers: Vec<(String, String)>,
    model: String,
    request_timeout_secs: u64,
    max_retry_attempts: u32,
    retry_base_delay_ms: u64,
}

impl AiClient {
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        Self::with_timeout(base_url, api_key, model, DEFAULT_REQUEST_TIMEOUT_SECS)
    }

    pub fn with_timeout(
        base_url: String,
        api_key: String,
        model: String,
        request_timeout_secs: u64,
    ) -> Self {
        Self::with_retry(
            base_url,
            api_key,
            model,
            request_timeout_secs,
            DEFAULT_MAX_RETRY_ATTEMPTS,
            DEFAULT_RETRY_BASE_DELAY_MS,
        )
    }

    /// Full builder: explicit request timeout AND retry budget.
    ///
    /// `max_retry_attempts` is clamped to `[1, 30]`:
    ///   * `1` floor so the original request always runs.
    ///   * `30` ceiling so the per-retry shift `1u64 << (attempt - 1)`
    ///     cannot overflow.
    pub fn with_retry(
        base_url: String,
        api_key: String,
        model: String,
        request_timeout_secs: u64,
        max_retry_attempts: u32,
        retry_base_delay_ms: u64,
    ) -> Self {
        let chat_url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
        let headers = if api_key.trim().is_empty() {
            vec![]
        } else {
            vec![(
                "Authorization".to_string(),
                format!("Bearer {}", api_key.trim()),
            )]
        };
        Self::with_resolved(
            base_url,
            chat_url,
            headers,
            model,
            request_timeout_secs,
            max_retry_attempts,
            retry_base_delay_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_resolved(
        base_url: String,
        chat_url: String,
        headers: Vec<(String, String)>,
        model: String,
        request_timeout_secs: u64,
        max_retry_attempts: u32,
        retry_base_delay_ms: u64,
    ) -> Self {
        Self {
            base_url,
            chat_url,
            headers,
            model,
            request_timeout_secs: request_timeout_secs.max(1),
            max_retry_attempts: max_retry_attempts.clamp(1, MAX_ALLOWED_RETRY_ATTEMPTS),
            retry_base_delay_ms,
        }
    }

    pub fn from_settings(settings: &crate::AppSettings) -> Self {
        let provider = settings.active_provider();
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

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn request_timeout_secs(&self) -> u64 {
        self.request_timeout_secs
    }
    pub fn max_retry_attempts(&self) -> u32 {
        self.max_retry_attempts
    }
    pub fn retry_base_delay_ms(&self) -> u64 {
        self.retry_base_delay_ms
    }

    fn build_client(&self) -> Result<reqwest::Client, AiError> {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(self.request_timeout_secs))
            .build()
            .map_err(|e| AiError::Transport(e.to_string()))
    }

    pub async fn chat(&self, messages: Vec<Message>) -> Result<String, AiError> {
        let resp = self.chat_with_tools(messages, vec![]).await?;
        Ok(resp.content.unwrap_or_default())
    }

    /// Wrapper with retry logic. The attempt cap and base delay come from the
    /// client's configured `max_retry_attempts` / `retry_base_delay_ms`
    /// (defaults match the historical hardcoded values: 3 attempts, 2 s base).
    ///
    /// Retries on: transient errors (timeout, transport, 429, 502, 503).
    /// Does NOT retry on permanent errors (auth, bad request, etc.).
    pub async fn chat_with_tools(
        &self,
        messages: Vec<Message>,
        tools: Vec<serde_json::Value>,
    ) -> Result<ChatResponse, AiError> {
        self.chat_with_tools_choice(messages, tools, ToolChoice::Auto)
            .await
    }

    /// Same as [`chat_with_tools`] but lets the caller force the model to
    /// emit a tool call. Used by the agentic loop as a one-shot escalation
    /// when the model returned text only but the task is mid-flight (no
    /// tool was called all turn) — borrowed from the OpenAI / LangChain
    /// "required" tool-choice pattern instead of inspecting model text.
    pub async fn chat_with_tools_choice(
        &self,
        messages: Vec<Message>,
        tools: Vec<serde_json::Value>,
        tool_choice: ToolChoice,
    ) -> Result<ChatResponse, AiError> {
        let max_attempts = self.max_retry_attempts;
        let base_delay_ms = self.retry_base_delay_ms;

        let mut last_err: Option<AiError> = None;

        for attempt in 0..max_attempts {
            if attempt > 0 {
                let backoff_ms = base_delay_ms * (1u64 << (attempt - 1));
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() as u64;
                let jitter_ms = seed
                    .wrapping_mul(0x9e3779b97f4a7c15)
                    .wrapping_add(attempt as u64)
                    % 1_000;
                log::debug!(
                    "AI retry attempt {}/{} after {} ms (model={})",
                    attempt + 1,
                    max_attempts,
                    backoff_ms + jitter_ms,
                    self.model
                );
                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms + jitter_ms)).await;
            }

            match self
                .chat_with_tools_once(messages.clone(), tools.clone(), tool_choice)
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

        Err(last_err.unwrap_or(AiError::Transport("max retries exhausted".into())))
    }

    /// Same as [`chat_with_tools_choice`] but performs exactly ONE attempt
    /// — no retry backoff. Used for the agent loop's one-shot
    /// `tool_choice="required"` escalation: when the proxy doesn't
    /// support that mode (e.g. returns 502 "All providers failed"),
    /// retrying 30× with exponential backoff just wedges the agent for
    /// many minutes. A single attempt fails fast and lets the caller
    /// gracefully fall back to the original text response.
    pub async fn chat_with_tools_choice_once(
        &self,
        messages: Vec<Message>,
        tools: Vec<serde_json::Value>,
        tool_choice: ToolChoice,
    ) -> Result<ChatResponse, AiError> {
        self.chat_with_tools_once(messages, tools, tool_choice)
            .await
    }

    /// Single (non-retrying) API call — used internally by `chat_with_tools_choice`.
    async fn chat_with_tools_once(
        &self,
        messages: Vec<Message>,
        tools: Vec<serde_json::Value>,
        tool_choice: ToolChoice,
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
            body["tool_choice"] = serde_json::json!(tool_choice.as_api_value());
        }

        let url = self.chat_url.clone();

        log::info!(
            "AI request → endpoint={} model={} messages={} tools={} auth={}",
            url,
            self.model,
            api_messages.len(),
            tools.len(),
            if self
                .headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("authorization"))
            {
                "header"
            } else {
                "none"
            }
        );

        let mut req = client.post(&url).json(&body);
        for (name, value) in &self.headers {
            req = req.header(name, value);
        }

        let started = std::time::Instant::now();
        let response = req.send().await.map_err(|e| {
            if e.is_timeout() {
                log::warn!(
                    "AI request timed out after {} ms (endpoint={} model={})",
                    started.elapsed().as_millis(),
                    url,
                    self.model
                );
                AiError::Timeout
            } else {
                log::warn!(
                    "AI request transport error (endpoint={} model={}): {}",
                    url,
                    self.model,
                    e
                );
                AiError::Transport(e.to_string())
            }
        })?;

        let status = response.status();
        let elapsed_ms = started.elapsed().as_millis();
        if !status.is_success() {
            let status = status.as_u16();
            let body = response.text().await.unwrap_or_default();
            log::warn!(
                "AI response ← status={} in {} ms (endpoint={} model={}): {}",
                status,
                elapsed_ms,
                url,
                self.model,
                body.chars().take(500).collect::<String>()
            );
            return Err(AiError::Api { status, body });
        }

        log::info!(
            "AI response ← status={} in {} ms (model={})",
            status.as_u16(),
            elapsed_ms,
            self.model
        );

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AiError::Json(e.to_string()))?;

        let choice = &json["choices"][0];
        let message = &choice["message"];
        let content = message["content"].as_str().map(|s| s.to_string());
        let finish_reason = choice["finish_reason"].as_str().map(|s| s.to_string());

        let tool_calls = message["tool_calls"].as_array().map(|tcs| {
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

        log::debug!(
            "AI response parsed: finish_reason={:?} content_len={} tool_calls={}",
            finish_reason,
            content.as_ref().map(|c| c.len()).unwrap_or(0),
            tool_calls
                .as_ref()
                .map(|tcs: &Vec<ToolCall>| tcs.len())
                .unwrap_or(0),
        );

        Ok(ChatResponse {
            content,
            tool_calls,
            finish_reason,
        })
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
    fn test_tool_choice_api_values() {
        // Lock the on-the-wire strings — must match the OpenAI /
        // vLLM / OpenLLM chat-completions spec, which several
        // proxies our users hit are strict about. Changing these
        // is a wire-format break.
        assert_eq!(ToolChoice::Auto.as_api_value(), "auto");
        assert_eq!(ToolChoice::None.as_api_value(), "none");
        assert_eq!(ToolChoice::Required.as_api_value(), "required");
    }

    #[test]
    fn test_client_default_timeout_is_120_seconds() {
        let c = make_client();
        assert_eq!(c.request_timeout_secs(), 120);
    }

    #[test]
    fn test_client_accepts_custom_timeout() {
        let c = AiClient::with_timeout(
            "http://localhost:9999".into(),
            "test-key".into(),
            "test-model".into(),
            300,
        );
        assert_eq!(c.request_timeout_secs(), 300);
    }

    #[test]
    fn test_client_accessors_exist() {
        let c = make_client();
        assert_eq!(c.base_url(), "http://localhost:9999");
        assert_eq!(c.model(), "test-model");
    }

    #[test]
    fn test_default_retry_budget_matches_legacy_constants() {
        let c = make_client();
        assert_eq!(c.max_retry_attempts(), 3);
        assert_eq!(c.retry_base_delay_ms(), 2_000);
    }

    #[test]
    fn test_with_retry_clamps_max_attempts_to_one() {
        let c = AiClient::with_retry(
            "http://localhost:9999".into(),
            "test-key".into(),
            "test-model".into(),
            120,
            0,
            500,
        );
        assert_eq!(c.max_retry_attempts(), 1, "0 must clamp to 1");
    }

    #[test]
    fn test_with_retry_clamps_max_attempts_to_thirty() {
        let c = AiClient::with_retry(
            "http://localhost:9999".into(),
            "test-key".into(),
            "test-model".into(),
            120,
            9_999,
            500,
        );
        assert_eq!(c.max_retry_attempts(), 30, "absurd value must clamp to 30");
    }

    #[test]
    fn test_with_retry_preserves_in_range_values() {
        let c = AiClient::with_retry(
            "http://localhost:9999".into(),
            "test-key".into(),
            "test-model".into(),
            120,
            5,
            750,
        );
        assert_eq!(c.max_retry_attempts(), 5);
        assert_eq!(c.retry_base_delay_ms(), 750);
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

    #[test]
    fn test_client_with_empty_api_key() {
        let c = AiClient::new(
            "http://localhost:9999".into(),
            "".into(),
            "test-model".into(),
        );
        assert_eq!(c.base_url(), "http://localhost:9999");
        assert_eq!(c.model(), "test-model");
    }

    #[test]
    fn test_client_trims_trailing_slash_in_url() {
        // The URL trimming happens in chat_with_tools_once, not in new()
        let c = AiClient::new(
            "http://localhost:9999/".into(),
            "key".into(),
            "model".into(),
        );
        // base_url stores it as-is, trimming happens at call time
        assert_eq!(c.base_url(), "http://localhost:9999/");
    }

    #[tokio::test]
    async fn test_chat_with_tools_returns_error_on_connection_refused() {
        let c = make_client();
        let result = c
            .chat_with_tools(vec![Message::user("hello")], vec![])
            .await;
        assert!(result.is_err());
    }

    // ── Message construction tests ─────────────────────────────────────

    #[test]
    fn test_message_system() {
        let msg = Message::system("You are a helpful assistant.");
        assert_eq!(msg.role, "system");
        assert_eq!(msg.content_str(), "You are a helpful assistant.");
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
    }

    #[test]
    fn test_message_user() {
        let msg = Message::user("Hello!");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content_str(), "Hello!");
    }

    #[test]
    fn test_message_assistant() {
        let msg = Message::assistant("Hi there!");
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content_str(), "Hi there!");
    }

    #[test]
    fn test_message_content_str_with_none() {
        let msg = Message {
            role: "assistant".into(),
            content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        };
        assert_eq!(msg.content_str(), "");
    }

    #[test]
    fn test_message_assistant_tool_calls() {
        let tc = ToolCall {
            id: "call-1".to_string(),
            call_type: Some("function".to_string()),
            function: FunctionCall {
                name: "calculator".to_string(),
                arguments: r#"{"expr":"2+2"}"#.to_string(),
            },
        };
        let msg = Message::assistant_tool_calls(Some("Let me calculate.".into()), vec![tc]);
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content_str(), "Let me calculate.");
        assert!(msg.tool_calls.is_some());
        let tcs = msg.tool_calls.unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].function.name, "calculator");
    }

    #[test]
    fn test_message_tool_result() {
        let msg = Message::tool_result("call-1", "calculator", "4");
        assert_eq!(msg.role, "tool");
        assert_eq!(msg.content_str(), "4");
        assert_eq!(msg.tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(msg.name.as_deref(), Some("calculator"));
    }

    // ── Serialization roundtrip tests ──────────────────────────────────

    #[test]
    fn test_message_serialization_roundtrip() {
        let msg = Message::user("test message");
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, "user");
        assert_eq!(deserialized.content_str(), "test message");
    }

    #[test]
    fn test_chat_response_serialization() {
        let resp = ChatResponse {
            content: Some("Hello!".to_string()),
            tool_calls: None,
            finish_reason: Some("stop".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("Hello!"));
        let deserialized: ChatResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.content.as_deref(), Some("Hello!"));
        assert!(deserialized.tool_calls.is_none());
        assert_eq!(deserialized.finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn test_chat_response_with_tool_calls() {
        let resp = ChatResponse {
            content: None,
            tool_calls: Some(vec![ToolCall {
                id: "tc-1".to_string(),
                call_type: Some("function".to_string()),
                function: FunctionCall {
                    name: "search".to_string(),
                    arguments: r#"{"q":"rust"}"#.to_string(),
                },
            }]),
            finish_reason: Some("tool_calls".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: ChatResponse = serde_json::from_str(&json).unwrap();
        assert!(deserialized.content.is_none());
        let tcs = deserialized.tool_calls.unwrap();
        assert_eq!(tcs[0].function.name, "search");
        assert_eq!(deserialized.finish_reason.as_deref(), Some("tool_calls"));
    }

    #[test]
    fn test_tool_call_serialization_skips_none_type() {
        let tc = ToolCall {
            id: "tc-1".to_string(),
            call_type: None,
            function: FunctionCall {
                name: "test".to_string(),
                arguments: "{}".to_string(),
            },
        };
        let json = serde_json::to_string(&tc).unwrap();
        // "type" field should be skipped when None
        assert!(!json.contains("\"type\""));
    }
}
