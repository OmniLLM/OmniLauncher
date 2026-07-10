use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::ai::errors::{classify_ai_error, AiError, ErrorClass};

/// Mutable Copilot token state shared behind an `RwLock` so a long-lived
/// `AiClient` can refresh its short-lived Copilot token in place (e.g. after a
/// 401) without being reconstructed.
#[derive(Debug)]
struct CopilotTokenState {
    token: String,
    expiry: i64,
}

/// Everything an `AiClient` needs to mint a fresh Copilot token on its own.
///
/// The long-lived `GitHub` token exchanges for a short-lived Copilot token via
/// `copilot_auth::get_copilot_token`. `provider_id` lets us persist the refreshed
/// token back to on-disk settings so sibling clients reuse it.
#[derive(Debug)]
struct CopilotAuth {
    provider_id: String,
    github_token: String,
    enterprise_url: String,
    state: RwLock<CopilotTokenState>,
}

/// Persist a refreshed Copilot token back into on-disk settings by provider id.
fn persist_copilot_token_by_id(provider_id: &str, token: &str, expiry: i64) -> bool {
    let mut latest = crate::settings::load_settings();
    let Some(target) = latest.providers.iter_mut().find(|p| p.id == provider_id) else {
        return false;
    };
    target.copilot_token = token.to_string();
    target.copilot_token_expiry = expiry;
    crate::settings::save_settings(&latest)
}

/// Render an error together with its full `source` chain.
///
/// `reqwest::Error`'s `Display` only prints the top-level message
/// (e.g. "error sending request for url (...)") and drops the underlying
/// cause — the actual TLS / proxy CONNECT / connection-reset detail lives in
/// the source chain. Flatten it so logs show the real root cause.
fn full_error_chain(err: &dyn std::error::Error) -> String {
    let mut out = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        out.push_str(": ");
        out.push_str(&cause.to_string());
        source = cause.source();
    }
    out
}

/// Refresh a Copilot provider's short-lived token synchronously.
///
/// `AiClient::from_settings` is a sync function that may itself be called from
/// within a tokio runtime (main/server), so a nested `block_on` would panic.
/// We therefore run the async refresh on a dedicated OS thread that owns a
/// fresh current-thread runtime. Returns `Ok(true)` when the token changed.
fn refresh_copilot_token_blocking(
    provider: &mut crate::settings::Provider,
) -> Result<bool, String> {
    let mut p = provider.clone();
    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("failed to build runtime: {e}"))?;
        let changed = rt.block_on(crate::ai::copilot_auth::refresh_copilot_token_if_needed(
            &mut p,
        ))?;
        Ok::<_, String>((changed, p))
    });
    let (changed, refreshed) = handle
        .join()
        .map_err(|_| "copilot refresh thread panicked".to_string())??;
    if changed {
        *provider = refreshed;
    }
    Ok(changed)
}

/// Persist a refreshed Copilot token back into the on-disk settings for the
/// matching provider id. Returns the provider id on success.
fn persist_refreshed_copilot_token(provider: &crate::settings::Provider) -> Option<String> {
    let mut latest = crate::settings::load_settings();
    let target = latest.providers.iter_mut().find(|p| p.id == provider.id)?;
    target.copilot_token = provider.copilot_token.clone();
    target.copilot_token_expiry = provider.copilot_token_expiry;
    if crate::settings::save_settings(&latest) {
        Some(provider.id.clone())
    } else {
        None
    }
}

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
    /// When `Some`, this is a GitHub Copilot provider and requests are routed
    /// per-model between `/chat/completions` and `/responses`. The value is the
    /// Copilot API base URL (e.g. `https://api.githubcopilot.com`).
    copilot_base: Option<String>,
    /// When `Some`, holds the state needed to refresh this client's short-lived
    /// Copilot token in place on a 401 (see `refresh_copilot_and_headers`).
    copilot_auth: Option<Arc<CopilotAuth>>,
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
            copilot_base: None,
            copilot_auth: None,
        }
    }

    /// Mark this client as a GitHub Copilot provider whose requests are routed
    /// per-model between `/chat/completions` and `/responses`. `copilot_base`
    /// is the Copilot API base URL (e.g. `https://api.githubcopilot.com`).
    fn with_copilot_base(mut self, copilot_base: String) -> Self {
        self.copilot_base = Some(copilot_base);
        self
    }

    /// Attach the state required to refresh this client's Copilot token in place.
    fn with_copilot_auth(mut self, provider: &crate::settings::Provider) -> Self {
        self.copilot_auth = Some(Arc::new(CopilotAuth {
            provider_id: provider.id.clone(),
            github_token: provider.copilot_github_token.clone(),
            enterprise_url: provider.copilot_enterprise_url.clone(),
            state: RwLock::new(CopilotTokenState {
                token: provider.copilot_token.clone(),
                expiry: provider.copilot_token_expiry,
            }),
        }));
        self
    }

    pub fn from_settings(settings: &crate::AppSettings) -> Self {
        let (mut provider, effective_model) = settings.resolve_active_selection();
        provider.model = effective_model;

        // For GitHub Copilot, refresh the short-lived Copilot token on demand
        // (when missing or near expiry) before baking it into request headers.
        // If it changes, persist so other clients reuse it.
        if provider.kind == crate::settings::ProviderKind::GithubCopilot {
            match refresh_copilot_token_blocking(&mut provider) {
                Ok(true) => {
                    if let Some(saved) = persist_refreshed_copilot_token(&provider) {
                        log::info!("copilot: refreshed token for provider '{saved}'");
                    }
                }
                Ok(false) => {}
                Err(err) => log::warn!(
                    "copilot: failed to refresh token for provider '{}': {err}",
                    provider.name
                ),
            }
        }

        match crate::ai::provider::resolve_provider(&provider) {
            Ok(resolved) => {
                let client = Self::with_resolved(
                    provider.base_url.clone(),
                    resolved.chat_url,
                    resolved.headers,
                    resolved.model,
                    settings.ai_timeout_secs,
                    settings.ai_max_retry_attempts,
                    settings.ai_retry_base_delay_ms,
                );
                if provider.kind == crate::settings::ProviderKind::GithubCopilot {
                    client
                        .with_copilot_base(crate::ai::copilot_auth::copilot_base_url(
                            &provider.copilot_enterprise_url,
                        ))
                        .with_copilot_auth(&provider)
                } else {
                    client
                }
            }
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
    ///
    /// For non-Copilot providers this is a plain OpenAI chat-completions call.
    /// For GitHub Copilot providers (`copilot_base.is_some()`) it selects the
    /// per-model request shape (`/chat/completions` vs `/responses`) and, when a
    /// chat-completions request returns `unsupported_api_for_model`, transparently
    /// retries once on `/responses`.
    async fn chat_with_tools_once(
        &self,
        messages: Vec<Message>,
        tools: Vec<serde_json::Value>,
        tool_choice: ToolChoice,
    ) -> Result<ChatResponse, AiError> {
        let client = self.build_client()?;
        let api_messages = Self::build_api_messages(&messages);

        let Some(base) = &self.copilot_base else {
            // Standard OpenAI-compatible path (custom / azure-foundry).
            return self
                .execute_chat(&client, &self.chat_url, &api_messages, &tools, tool_choice)
                .await;
        };

        let base = base.trim_end_matches('/');
        match crate::ai::copilot_models::select_shape(&self.model, false) {
            crate::ai::copilot_models::CopilotShape::Responses => {
                let url = format!("{base}/responses");
                self.execute_responses(&client, &url, &messages, &tools, tool_choice)
                    .await
            }
            crate::ai::copilot_models::CopilotShape::Chat => {
                let result = self
                    .execute_chat(&client, &self.chat_url, &api_messages, &tools, tool_choice)
                    .await;
                match result {
                    Err(AiError::Api { status, body })
                        if crate::ai::copilot_models::is_unsupported_chat_completions_error(
                            status, &body,
                        ) =>
                    {
                        log::info!(
                            "copilot: model '{}' rejected /chat/completions ({}); retrying on /responses",
                            self.model,
                            status
                        );
                        let url = format!("{base}/responses");
                        self.execute_responses(&client, &url, &messages, &tools, tool_choice)
                            .await
                    }
                    other => other,
                }
            }
        }
    }

    /// Convert internal `Message`s into OpenAI chat-completions `messages`.
    fn build_api_messages(messages: &[Message]) -> Vec<serde_json::Value> {
        messages
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
            .collect()
    }

    /// Execute an OpenAI chat-completions request and parse the response.
    async fn execute_chat(
        &self,
        client: &reqwest::Client,
        url: &str,
        api_messages: &[serde_json::Value],
        tools: &[serde_json::Value],
        tool_choice: ToolChoice,
    ) -> Result<ChatResponse, AiError> {
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": api_messages,
        });
        if !tools.is_empty() {
            body["tools"] = serde_json::json!(tools);
            body["tool_choice"] = serde_json::json!(tool_choice.as_api_value());
        }

        let json = self
            .send_json(client, url, &body, api_messages.len(), tools.len())
            .await?;

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
            "AI response parsed (chat): finish_reason={:?} content_len={} tool_calls={}",
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

    /// Execute an OpenAI Responses request (`POST /responses`) and parse the
    /// response into the same `ChatResponse` shape the agent loop consumes.
    async fn execute_responses(
        &self,
        client: &reqwest::Client,
        url: &str,
        messages: &[Message],
        tools: &[serde_json::Value],
        tool_choice: ToolChoice,
    ) -> Result<ChatResponse, AiError> {
        let input = Self::build_responses_input(messages);
        let mut body = serde_json::json!({
            "model": self.model,
            "input": input,
            "stream": false,
        });
        if !tools.is_empty() {
            body["tools"] = serde_json::json!(Self::chat_tools_to_responses_tools(tools));
            body["tool_choice"] = serde_json::json!(tool_choice.as_api_value());
        }

        let json = self
            .send_json(client, url, &body, messages.len(), tools.len())
            .await?;

        Self::parse_responses_json(&json)
    }

    /// Convert internal `Message`s into the Responses API `input` array.
    fn build_responses_input(messages: &[Message]) -> Vec<serde_json::Value> {
        let mut input: Vec<serde_json::Value> = Vec::new();
        for m in messages {
            match m.role.as_str() {
                "system" | "user" => {
                    let text_type = "input_text";
                    input.push(serde_json::json!({
                        "type": "message",
                        "role": m.role,
                        "content": [{ "type": text_type, "text": m.content_str() }],
                    }));
                }
                "assistant" => {
                    if let Some(text) = m.content.as_ref().filter(|c| !c.is_empty()) {
                        input.push(serde_json::json!({
                            "type": "message",
                            "role": "assistant",
                            "content": [{ "type": "output_text", "text": text }],
                        }));
                    }
                    if let Some(tcs) = &m.tool_calls {
                        for tc in tcs {
                            input.push(serde_json::json!({
                                "type": "function_call",
                                "call_id": tc.id,
                                "name": tc.function.name,
                                "arguments": tc.function.arguments,
                            }));
                        }
                    }
                }
                "tool" => {
                    input.push(serde_json::json!({
                        "type": "function_call_output",
                        "call_id": m.tool_call_id.clone().unwrap_or_default(),
                        "output": m.content_str(),
                    }));
                }
                _ => {}
            }
        }
        input
    }

    /// Convert OpenAI chat-completions tool definitions
    /// (`{type:function, function:{name, parameters, description}}`) into the
    /// flattened Responses API form (`{type:function, name, parameters,
    /// description}`).
    fn chat_tools_to_responses_tools(tools: &[serde_json::Value]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|t| {
                let f = &t["function"];
                let mut out = serde_json::json!({ "type": "function" });
                if let Some(name) = f["name"].as_str() {
                    out["name"] = serde_json::json!(name);
                }
                if !f["parameters"].is_null() {
                    out["parameters"] = f["parameters"].clone();
                }
                if let Some(desc) = f["description"].as_str() {
                    out["description"] = serde_json::json!(desc);
                }
                out
            })
            .collect()
    }

    /// Parse a Responses API payload into a `ChatResponse`.
    fn parse_responses_json(json: &serde_json::Value) -> Result<ChatResponse, AiError> {
        let mut content = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        if let Some(output) = json["output"].as_array() {
            for item in output {
                match item["type"].as_str() {
                    Some("message") => {
                        if let Some(blocks) = item["content"].as_array() {
                            for block in blocks {
                                let bt = block["type"].as_str().unwrap_or("");
                                if bt == "output_text" || bt == "text" {
                                    if let Some(text) = block["text"].as_str() {
                                        content.push_str(text);
                                    }
                                }
                            }
                        }
                    }
                    Some("function_call") => {
                        let id = item["call_id"]
                            .as_str()
                            .or_else(|| item["id"].as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = item["name"].as_str().unwrap_or("").to_string();
                        let arguments = item["arguments"].as_str().unwrap_or("{}").to_string();
                        tool_calls.push(ToolCall {
                            id,
                            call_type: Some("function".to_string()),
                            function: FunctionCall { name, arguments },
                        });
                    }
                    _ => {}
                }
            }
        }

        // Map Responses status/output into an OpenAI-style finish_reason the
        // agent loop understands (`tool_calls` / `length` / `stop`).
        let finish_reason = if !tool_calls.is_empty() {
            Some("tool_calls".to_string())
        } else {
            match json["incomplete_details"]["reason"].as_str() {
                Some("max_output_tokens") => Some("length".to_string()),
                _ if json["status"].as_str() == Some("incomplete") => Some("length".to_string()),
                _ => Some("stop".to_string()),
            }
        };

        log::debug!(
            "AI response parsed (responses): finish_reason={:?} content_len={} tool_calls={}",
            finish_reason,
            content.len(),
            tool_calls.len(),
        );

        Ok(ChatResponse {
            content: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            finish_reason,
        })
    }

    /// The `Authorization` header value to send right now. For Copilot providers
    /// this reflects the live (possibly refreshed) token from `copilot_auth`
    /// rather than the value frozen into `self.headers` at construction.
    fn live_auth_header(&self) -> Option<String> {
        let auth = self.copilot_auth.as_ref()?;
        let state = auth.state.read().ok()?;
        Some(format!("Bearer {}", state.token))
    }

    /// Apply the client's headers to `req`, overriding `Authorization` with the
    /// live Copilot token when this is a Copilot client.
    fn apply_headers(&self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let live_auth = self.live_auth_header();
        for (name, value) in &self.headers {
            if live_auth.is_some() && name.eq_ignore_ascii_case("authorization") {
                continue; // replaced below with the live token
            }
            req = req.header(name, value);
        }
        if let Some(auth) = live_auth {
            req = req.header("Authorization", auth);
        }
        req
    }

    /// Refresh the Copilot token in place after an auth failure. Returns `true`
    /// when a *different* token was obtained (so the caller should retry). The
    /// new token is stored on `copilot_auth.state` and persisted to settings so
    /// sibling clients reuse it.
    async fn refresh_copilot_token(&self) -> bool {
        let Some(auth) = self.copilot_auth.as_ref() else {
            return false;
        };
        if auth.github_token.trim().is_empty() {
            log::warn!("copilot: cannot refresh token — no GitHub token stored");
            return false;
        }

        let current = auth
            .state
            .read()
            .ok()
            .map(|s| s.token.clone())
            .unwrap_or_default();

        let fresh = match crate::ai::copilot_auth::get_copilot_token(
            &auth.github_token,
            &auth.enterprise_url,
        )
        .await
        {
            Ok(t) => t,
            Err(e) => {
                log::warn!("copilot: token refresh failed: {e}");
                return false;
            }
        };

        if fresh.token == current {
            log::warn!("copilot: token refresh returned an unchanged token");
            return false;
        }

        if let Ok(mut state) = auth.state.write() {
            state.token = fresh.token.clone();
            state.expiry = fresh.expires_at;
        }
        if persist_copilot_token_by_id(&auth.provider_id, &fresh.token, fresh.expires_at) {
            log::info!(
                "copilot: refreshed token in place for provider '{}'",
                auth.provider_id
            );
        }
        true
    }

    /// POST a JSON body with the client's headers and return the parsed JSON,
    /// mapping transport/HTTP/JSON failures to `AiError`. Shared by the chat and
    /// responses execution paths.
    ///
    /// For Copilot providers, a `401` triggers a one-shot in-place token refresh
    /// and a single retry — this is what keeps a long-lived client working after
    /// its short-lived Copilot token expires mid-session.
    async fn send_json(
        &self,
        client: &reqwest::Client,
        url: &str,
        body: &serde_json::Value,
        message_count: usize,
        tool_count: usize,
    ) -> Result<serde_json::Value, AiError> {
        match self
            .send_json_once(client, url, body, message_count, tool_count)
            .await
        {
            Err(AiError::Api {
                status: 401,
                body: err_body,
            }) if self.copilot_auth.is_some() => {
                log::info!("copilot: got 401, attempting in-place token refresh and one retry");
                if self.refresh_copilot_token().await {
                    self.send_json_once(client, url, body, message_count, tool_count)
                        .await
                } else {
                    Err(AiError::Api {
                        status: 401,
                        body: err_body,
                    })
                }
            }
            other => other,
        }
    }

    /// Single POST attempt used by [`send_json`].
    async fn send_json_once(
        &self,
        client: &reqwest::Client,
        url: &str,
        body: &serde_json::Value,
        message_count: usize,
        tool_count: usize,
    ) -> Result<serde_json::Value, AiError> {
        log::info!(
            "AI request → endpoint={} model={} messages={} tools={} auth={}",
            url,
            self.model,
            message_count,
            tool_count,
            if self.live_auth_header().is_some()
                || self
                    .headers
                    .iter()
                    .any(|(k, _)| k.eq_ignore_ascii_case("authorization"))
            {
                "header"
            } else {
                "none"
            }
        );

        let req = self.apply_headers(client.post(url).json(body));

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
                let detail = full_error_chain(&e);
                log::warn!(
                    "AI request transport error (endpoint={} model={}): {}",
                    url,
                    self.model,
                    detail
                );
                AiError::Transport(detail)
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

        response
            .json()
            .await
            .map_err(|e| AiError::Json(e.to_string()))
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

    #[test]
    fn responses_input_maps_roles() {
        let messages = vec![
            Message::system("be helpful"),
            Message::user("hi"),
            Message::assistant_tool_calls(
                None,
                vec![ToolCall {
                    id: "call-1".into(),
                    call_type: Some("function".into()),
                    function: FunctionCall {
                        name: "get_time".into(),
                        arguments: "{}".into(),
                    },
                }],
            ),
            Message::tool_result("call-1", "get_time", "12:00"),
        ];
        let input = AiClient::build_responses_input(&messages);
        // system + user + function_call + function_call_output
        assert_eq!(input.len(), 4);
        assert_eq!(input[0]["type"], "message");
        assert_eq!(input[0]["role"], "system");
        assert_eq!(input[0]["content"][0]["type"], "input_text");
        assert_eq!(input[2]["type"], "function_call");
        assert_eq!(input[2]["call_id"], "call-1");
        assert_eq!(input[2]["name"], "get_time");
        assert_eq!(input[3]["type"], "function_call_output");
        assert_eq!(input[3]["call_id"], "call-1");
        assert_eq!(input[3]["output"], "12:00");
    }

    #[test]
    fn responses_parse_text_and_tool_calls() {
        let json = serde_json::json!({
            "status": "completed",
            "output": [
                { "type": "message", "content": [
                    { "type": "output_text", "text": "Hello " },
                    { "type": "output_text", "text": "world" }
                ]},
                { "type": "function_call", "call_id": "c1", "name": "do_it", "arguments": "{\"x\":1}" }
            ]
        });
        let resp = AiClient::parse_responses_json(&json).unwrap();
        assert_eq!(resp.content.as_deref(), Some("Hello world"));
        let tcs = resp.tool_calls.unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].id, "c1");
        assert_eq!(tcs[0].function.name, "do_it");
        assert_eq!(tcs[0].function.arguments, "{\"x\":1}");
        // Tool calls present → finish_reason maps to tool_calls.
        assert_eq!(resp.finish_reason.as_deref(), Some("tool_calls"));
    }

    #[test]
    fn responses_parse_incomplete_maps_to_length() {
        let json = serde_json::json!({
            "status": "incomplete",
            "incomplete_details": { "reason": "max_output_tokens" },
            "output": [ { "type": "message", "content": [
                { "type": "output_text", "text": "partial" }
            ]}]
        });
        let resp = AiClient::parse_responses_json(&json).unwrap();
        assert_eq!(resp.content.as_deref(), Some("partial"));
        assert!(resp.tool_calls.is_none());
        assert_eq!(resp.finish_reason.as_deref(), Some("length"));
    }

    #[test]
    fn responses_tools_are_flattened() {
        let chat_tools = vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "search",
                "description": "search the web",
                "parameters": { "type": "object", "properties": {} }
            }
        })];
        let out = AiClient::chat_tools_to_responses_tools(&chat_tools);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["type"], "function");
        assert_eq!(out[0]["name"], "search");
        assert_eq!(out[0]["description"], "search the web");
        assert_eq!(out[0]["parameters"]["type"], "object");
        // No nested "function" wrapper in the responses form.
        assert!(out[0]["function"].is_null());
    }

    fn copilot_client(token: &str) -> AiClient {
        let provider = crate::settings::Provider {
            id: "cop-1".into(),
            kind: crate::settings::ProviderKind::GithubCopilot,
            copilot_github_token: "gho_x".into(),
            copilot_token: token.into(),
            copilot_token_expiry: 0,
            ..crate::settings::Provider::default()
        };
        let resolved = crate::ai::provider::resolve_provider(&provider).unwrap();
        AiClient::with_resolved(
            provider.base_url.clone(),
            resolved.chat_url,
            resolved.headers,
            resolved.model,
            120,
            3,
            2_000,
        )
        .with_copilot_base("https://api.githubcopilot.com".into())
        .with_copilot_auth(&provider)
    }

    #[test]
    fn live_auth_header_reflects_stored_copilot_token() {
        let c = copilot_client("tok-a");
        assert_eq!(c.live_auth_header().as_deref(), Some("Bearer tok-a"));
    }

    #[test]
    fn live_auth_header_none_for_non_copilot_client() {
        let c = make_client();
        assert!(c.live_auth_header().is_none());
    }

    #[test]
    fn live_auth_header_tracks_in_place_token_update() {
        let c = copilot_client("tok-a");
        {
            let auth = c.copilot_auth.as_ref().unwrap();
            auth.state.write().unwrap().token = "tok-b".into();
        }
        assert_eq!(c.live_auth_header().as_deref(), Some("Bearer tok-b"));
    }
}
