//! AI tool-call simulation tests
//!
//! Spins up a tiny in-process HTTP server that **acts like an OpenAI-style
//! Chat Completions endpoint** and exercises `AiClient.chat_with_tools()`
//! end-to-end. No network, no API key, no real LLM — but every byte that
//! goes onto the wire is the same as what would hit a real provider.
//!
//! Tests cover:
//!   1. happy-path text reply (no tools)
//!   2. tool-call reply — single tool requested
//!   3. tool-call reply — multiple tools requested in one turn
//!   4. multi-turn round trip: model asks for tool, we send back result,
//!      model finalizes with content
//!   5. transient 429 → automatic retry → success
//!   6. permanent 401 → no retry, returns AiError::Api
//!   7. request body shape — model, messages, tools, tool_choice
//!   8. bearer auth header presence + absence
//!
//! Why not wiremock / httpmock? They aren't in the dev-deps and the project
//! already builds an HTTP server with raw tokio::net::TcpListener. Keeping
//! that pattern minimizes the dep surface.

use omnilauncher_lib::ai::client::{AiClient, FunctionCall, Message, ToolCall};
use omnilauncher_lib::ai::router::{ConversationContext, Router};
use omnilauncher_lib::plugins::Plugin;
use omnilauncher_lib::{PluginManager, QueryResult, SkillManager};
use serde_json::{json, Value};
use std::sync::Arc;
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

/// One scripted response from the mock server.
#[derive(Clone)]
struct ScriptedResponse {
    status: u16,
    body: String,
}

impl ScriptedResponse {
    fn ok(body: Value) -> Self {
        Self {
            status: 200,
            body: body.to_string(),
        }
    }
    fn http(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.to_string(),
        }
    }
}

/// In-memory log of one inbound request the mock saw.
#[derive(Debug, Clone)]
struct CapturedRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Value,
}

impl CapturedRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// A scripted mock LLM server. Hands out responses from `script` in FIFO
/// order. Each test gets its own instance bound to a fresh port.
struct MockLlm {
    base_url: String,
    captures: Arc<Mutex<Vec<CapturedRequest>>>,
}

impl MockLlm {
    async fn start(script: Vec<ScriptedResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let base_url = format!("http://127.0.0.1:{}", port);
        let captures: Arc<Mutex<Vec<CapturedRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let captures_for_task = Arc::clone(&captures);
        let script = Arc::new(Mutex::new(script.into_iter()));

        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(v) => v,
                    Err(_) => break,
                };
                let captures = Arc::clone(&captures_for_task);
                let script = Arc::clone(&script);

                tokio::spawn(async move {
                    // ── Read request line + headers ────────────────────────
                    let (read_half, mut write_half) = socket.split();
                    let mut reader = BufReader::new(read_half);

                    let mut request_line = String::new();
                    if reader.read_line(&mut request_line).await.unwrap_or(0) == 0 {
                        return;
                    }
                    let mut parts = request_line.split_whitespace();
                    let method = parts.next().unwrap_or("").to_string();
                    let path = parts.next().unwrap_or("").to_string();

                    let mut headers: Vec<(String, String)> = Vec::new();
                    let mut content_length: usize = 0;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                            break;
                        }
                        let trimmed = line.trim_end_matches(['\r', '\n']);
                        if trimmed.is_empty() {
                            break;
                        }
                        if let Some((k, v)) = trimmed.split_once(':') {
                            let k = k.trim().to_string();
                            let v = v.trim().to_string();
                            if k.eq_ignore_ascii_case("content-length") {
                                content_length = v.parse().unwrap_or(0);
                            }
                            headers.push((k, v));
                        }
                    }

                    // ── Read body ──────────────────────────────────────────
                    let mut buf = vec![0u8; content_length];
                    if content_length > 0 {
                        let _ = reader.read_exact(&mut buf).await;
                    }
                    let body_str = String::from_utf8_lossy(&buf).to_string();
                    let body_json: Value = serde_json::from_str(&body_str).unwrap_or(Value::Null);

                    captures.lock().unwrap().push(CapturedRequest {
                        method,
                        path,
                        headers,
                        body: body_json,
                    });

                    // ── Hand out the next scripted response ────────────────
                    let resp = script.lock().unwrap().next().unwrap_or_else(|| {
                        ScriptedResponse::http(500, "no more scripted responses")
                    });

                    let response = format!(
                        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        resp.status,
                        status_text(resp.status),
                        resp.body.len(),
                        resp.body
                    );
                    let _ = write_half.write_all(response.as_bytes()).await;
                    let _ = write_half.shutdown().await;
                });
            }
        });

        MockLlm { base_url, captures }
    }

    fn requests(&self) -> Vec<CapturedRequest> {
        self.captures.lock().unwrap().clone()
    }
}

fn status_text(code: u16) -> &'static str {
    match code {
        200 => "OK",
        401 => "Unauthorized",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Status",
    }
}

// ---------------------------------------------------------------------------
// Helpers: build mock LLM response payloads in OpenAI Chat Completions shape.
// ---------------------------------------------------------------------------

fn text_response(content: &str) -> Value {
    json!({
        "id": "chatcmpl-mock-1",
        "object": "chat.completion",
        "created": 1_700_000_000,
        "model": "mock-model",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": content },
            "finish_reason": "stop"
        }]
    })
}

fn tool_call_response(calls: Vec<(&str, &str, Value)>) -> Value {
    let tool_calls: Vec<Value> = calls
        .into_iter()
        .map(|(id, name, args)| {
            json!({
                "id": id,
                "type": "function",
                "function": { "name": name, "arguments": args.to_string() }
            })
        })
        .collect();
    json!({
        "id": "chatcmpl-mock-tc",
        "object": "chat.completion",
        "created": 1_700_000_000,
        "model": "mock-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": tool_calls
            },
            "finish_reason": "tool_calls"
        }]
    })
}

fn one_tool(name: &str, description: &str) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": {
                "type": "object",
                "properties": { "q": { "type": "string" } },
                "required": ["q"]
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn happy_path_text_reply_no_tools() {
    let mock = MockLlm::start(vec![ScriptedResponse::ok(text_response("Hello from mock"))]).await;
    let client = AiClient::new(
        mock.base_url.clone(),
        "".to_string(),
        "mock-model".to_string(),
    );

    let resp = client
        .chat_with_tools(vec![Message::user("hi")], vec![])
        .await
        .expect("chat ok");

    assert_eq!(resp.content.as_deref(), Some("Hello from mock"));
    assert!(resp.tool_calls.is_none() || resp.tool_calls.as_ref().unwrap().is_empty());

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1, "exactly one HTTP call");
    assert_eq!(reqs[0].method, "POST");
    assert_eq!(reqs[0].path, "/v1/chat/completions");
}

#[tokio::test]
async fn request_body_shape_has_model_messages_tools_and_tool_choice() {
    let mock = MockLlm::start(vec![ScriptedResponse::ok(text_response("ok"))]).await;
    let client = AiClient::new(
        mock.base_url.clone(),
        "".to_string(),
        "my-model".to_string(),
    );

    let _ = client
        .chat_with_tools(
            vec![Message::system("be brief"), Message::user("hi")],
            vec![one_tool("search", "search the web")],
        )
        .await
        .expect("chat ok");

    let reqs = mock.requests();
    let body = &reqs[0].body;
    assert_eq!(body["model"], "my-model");
    assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(body["messages"][1]["content"], "hi");
    assert_eq!(body["tools"].as_array().unwrap().len(), 1);
    assert_eq!(body["tools"][0]["function"]["name"], "search");
    assert_eq!(body["tool_choice"], "auto");
}

#[tokio::test]
async fn no_tools_means_no_tool_choice_field() {
    let mock = MockLlm::start(vec![ScriptedResponse::ok(text_response("ok"))]).await;
    let client = AiClient::new(mock.base_url.clone(), "".to_string(), "m".to_string());

    let _ = client
        .chat_with_tools(vec![Message::user("hi")], vec![])
        .await
        .expect("chat ok");

    let body = &mock.requests()[0].body;
    assert!(
        body.get("tools").is_none(),
        "tools should be absent when none given"
    );
    assert!(
        body.get("tool_choice").is_none(),
        "tool_choice should be absent when no tools"
    );
}

#[tokio::test]
async fn bearer_auth_header_present_when_key_set() {
    let mock = MockLlm::start(vec![ScriptedResponse::ok(text_response("ok"))]).await;
    let client = AiClient::new(
        mock.base_url.clone(),
        "sk-secret".to_string(),
        "m".to_string(),
    );

    let _ = client
        .chat_with_tools(vec![Message::user("hi")], vec![])
        .await
        .expect("chat ok");

    let reqs = mock.requests();
    let auth = reqs[0].header("authorization").unwrap_or("");
    assert_eq!(auth, "Bearer sk-secret");
}

#[tokio::test]
async fn bearer_auth_header_absent_when_key_empty() {
    let mock = MockLlm::start(vec![ScriptedResponse::ok(text_response("ok"))]).await;
    let client = AiClient::new(mock.base_url.clone(), "".to_string(), "m".to_string());

    let _ = client
        .chat_with_tools(vec![Message::user("hi")], vec![])
        .await
        .expect("chat ok");

    assert!(mock.requests()[0].header("authorization").is_none());
}

#[tokio::test]
async fn single_tool_call_response_parses_correctly() {
    let mock = MockLlm::start(vec![ScriptedResponse::ok(tool_call_response(vec![(
        "call_1",
        "search",
        json!({ "q": "rust async" }),
    )]))])
    .await;
    let client = AiClient::new(mock.base_url.clone(), "".to_string(), "m".to_string());

    let resp = client
        .chat_with_tools(
            vec![Message::user("find rust async docs")],
            vec![one_tool("search", "search the web")],
        )
        .await
        .expect("chat ok");

    assert!(
        resp.content.is_none() || resp.content.as_deref() == Some(""),
        "content should be null when tool_calls is set, got: {:?}",
        resp.content
    );
    let tcs = resp.tool_calls.expect("tool_calls present");
    assert_eq!(tcs.len(), 1);
    assert_eq!(tcs[0].id, "call_1");
    assert_eq!(tcs[0].function.name, "search");
    let parsed_args: Value = serde_json::from_str(&tcs[0].function.arguments).unwrap();
    assert_eq!(parsed_args["q"], "rust async");
}

#[tokio::test]
async fn parallel_tool_calls_in_one_response_all_returned() {
    let mock = MockLlm::start(vec![ScriptedResponse::ok(tool_call_response(vec![
        ("call_a", "search", json!({ "q": "alpha" })),
        ("call_b", "search", json!({ "q": "beta" })),
        ("call_c", "calc", json!({ "expr": "2+2" })),
    ]))])
    .await;
    let client = AiClient::new(mock.base_url.clone(), "".to_string(), "m".to_string());

    let resp = client
        .chat_with_tools(vec![Message::user("do many things")], vec![])
        .await
        .expect("chat ok");

    let tcs = resp.tool_calls.expect("tool_calls present");
    assert_eq!(tcs.len(), 3);
    let names: Vec<&str> = tcs.iter().map(|tc| tc.function.name.as_str()).collect();
    assert_eq!(names, vec!["search", "search", "calc"]);
    let ids: Vec<&str> = tcs.iter().map(|tc| tc.id.as_str()).collect();
    assert_eq!(ids, vec!["call_a", "call_b", "call_c"]);
}

#[tokio::test]
async fn multi_turn_round_trip_tool_use_then_final_answer() {
    // Turn 1: model requests `search`.
    // Turn 2 (after we echo back the tool result): model returns the answer.
    let mock = MockLlm::start(vec![
        ScriptedResponse::ok(tool_call_response(vec![(
            "call_1",
            "search",
            json!({ "q": "capital of france" }),
        )])),
        ScriptedResponse::ok(text_response("The capital of France is Paris.")),
    ])
    .await;
    let client = AiClient::new(mock.base_url.clone(), "".to_string(), "m".to_string());

    // ── Turn 1: ask, get tool call ────────────────────────────────────────
    let mut history = vec![Message::user("what is the capital of france?")];
    let tools = vec![one_tool("search", "search the web")];

    let r1 = client
        .chat_with_tools(history.clone(), tools.clone())
        .await
        .expect("turn 1");
    let tcs = r1.tool_calls.expect("turn 1 should have tool_calls");
    assert_eq!(tcs.len(), 1);
    let call = &tcs[0];

    // Persist the assistant tool-call message into history.
    history.push(Message::assistant_tool_calls(
        None,
        vec![ToolCall {
            id: call.id.clone(),
            call_type: Some("function".to_string()),
            function: FunctionCall {
                name: call.function.name.clone(),
                arguments: call.function.arguments.clone(),
            },
        }],
    ));

    // Simulate executing the tool locally and append the result.
    history.push(Message::tool_result(
        &call.id,
        &call.function.name,
        "Paris, the capital of France.",
    ));

    // ── Turn 2: model finalizes ───────────────────────────────────────────
    let r2 = client
        .chat_with_tools(history.clone(), tools.clone())
        .await
        .expect("turn 2");

    assert_eq!(
        r2.content.as_deref(),
        Some("The capital of France is Paris.")
    );
    assert!(r2.tool_calls.is_none() || r2.tool_calls.as_ref().unwrap().is_empty());

    // ── Verify request shape on turn 2 ────────────────────────────────────
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2);
    let t2_msgs = reqs[1].body["messages"].as_array().unwrap();
    assert_eq!(t2_msgs.len(), 3, "user + assistant(tool_calls) + tool");
    assert_eq!(t2_msgs[1]["role"], "assistant");
    assert!(t2_msgs[1]["tool_calls"].is_array());
    assert_eq!(t2_msgs[2]["role"], "tool");
    assert_eq!(t2_msgs[2]["tool_call_id"], call.id);
    assert_eq!(t2_msgs[2]["content"], "Paris, the capital of France.");
}

struct TestToolPlugin;

#[async_trait::async_trait]
impl Plugin for TestToolPlugin {
    fn name(&self) -> &str {
        "Test Calculator"
    }

    fn description(&self) -> &str {
        "Test calculator tool"
    }

    fn keyword(&self) -> Option<&str> {
        None
    }

    async fn query(&self, _q: &omnilauncher_lib::plugins::Query) -> Vec<QueryResult> {
        vec![]
    }

    fn tool_schema(&self) -> Option<Value> {
        Some(one_tool("calculator", "calculate a simple expression"))
    }

    async fn execute_tool(&self, _args: Value) -> String {
        "4".to_string()
    }
}

#[tokio::test]
async fn router_accepts_stop_text_after_tool_result_as_final_answer() {
    // Regression for GPT-5.5-style strict finalization: after the model has
    // called a tool and then returns content with finish_reason="stop", the
    // router must accept that text as final. It must NOT append a continuation
    // nudge and make another LLM request.
    let mock = MockLlm::start(vec![
        ScriptedResponse::ok(tool_call_response(vec![(
            "call_1",
            "calculator",
            json!({ "q": "2+2" }),
        )])),
        ScriptedResponse::ok(text_response("The answer is 4.")),
        ScriptedResponse::ok(text_response("unexpected extra request")),
    ])
    .await;
    let client = AiClient::new(mock.base_url.clone(), "".to_string(), "gpt-5.5".to_string());
    let mut plugin_manager = PluginManager::new();
    plugin_manager.register(Box::new(TestToolPlugin));
    let mut context = ConversationContext::default();
    context.add_user("Use calculator to compute 2+2, then answer.");
    let mut skill_manager = SkillManager::new();

    let response = Router::ai_route(
        "Use calculator to compute 2+2, then answer.",
        &plugin_manager,
        &client,
        &context,
        &mut skill_manager,
        None,
        8,
        true,
    )
    .await;

    assert_eq!(response.content, "The answer is 4.");
    assert_eq!(
        mock.requests().len(),
        2,
        "final stop text after a tool result must not trigger an extra LLM request"
    );
}
#[tokio::test]
async fn permanent_401_error_returns_api_error_without_retry() {
    // Three responses queued — but on 401 the client should not retry, so
    // only the first should be consumed.
    let mock = MockLlm::start(vec![
        ScriptedResponse::http(401, r#"{"error":{"message":"bad key"}}"#),
        ScriptedResponse::ok(text_response("should not be reached")),
        ScriptedResponse::ok(text_response("should not be reached")),
    ])
    .await;
    let client = AiClient::new(
        mock.base_url.clone(),
        "bad-key".to_string(),
        "m".to_string(),
    );

    let err = client
        .chat_with_tools(vec![Message::user("hi")], vec![])
        .await
        .expect_err("should error");

    let msg = format!("{:?}", err);
    assert!(
        msg.contains("401") || msg.to_lowercase().contains("bad key"),
        "expected 401 payload, got {}",
        msg
    );

    // Critical: only ONE attempt, no retries on auth failure.
    assert_eq!(mock.requests().len(), 1, "401 must not retry");
}

#[tokio::test]
async fn transient_429_triggers_retry_then_success() {
    // First 429 retries, second succeeds. The backoff is 2 s minimum which
    // would slow tests; we accept the wait for a single retry.
    let mock = MockLlm::start(vec![
        ScriptedResponse::http(429, r#"{"error":{"message":"rate limited"}}"#),
        ScriptedResponse::ok(text_response("recovered")),
    ])
    .await;
    let client = AiClient::new(mock.base_url.clone(), "".to_string(), "m".to_string());

    let resp = client
        .chat_with_tools(vec![Message::user("hi")], vec![])
        .await
        .expect("retry should succeed");

    assert_eq!(resp.content.as_deref(), Some("recovered"));
    assert_eq!(mock.requests().len(), 2, "exactly one retry");
}

#[tokio::test]
async fn persistent_5xx_eventually_returns_error_after_retries() {
    // 503 IS in the transient list (along with 429, 502). 500 is NOT — the
    // server already chose to return its body, so the client treats it as
    // permanent. Use 503 here so the retry path actually fires.
    let mock = MockLlm::start(vec![
        ScriptedResponse::http(503, "server unavailable"),
        ScriptedResponse::http(503, "server unavailable"),
        ScriptedResponse::http(503, "server unavailable"),
    ])
    .await;
    let client = AiClient::new(mock.base_url.clone(), "".to_string(), "m".to_string());

    let err = client
        .chat_with_tools(vec![Message::user("hi")], vec![])
        .await
        .expect_err("should error after retries");

    let msg = format!("{:?}", err);
    assert!(
        msg.contains("503") || msg.to_lowercase().contains("unavailable"),
        "expected 503-derived error, got {}",
        msg
    );
    assert_eq!(
        mock.requests().len(),
        3,
        "should have attempted MAX_ATTEMPTS=3 times"
    );
}

#[tokio::test]
async fn five_hundred_is_treated_as_permanent_and_not_retried() {
    // Document the (perhaps surprising) policy: bare HTTP 500 is classified
    // as Permanent. This guards against accidentally changing that behavior.
    let mock = MockLlm::start(vec![
        ScriptedResponse::http(500, "server down"),
        ScriptedResponse::ok(text_response("would never be reached")),
    ])
    .await;
    let client = AiClient::new(mock.base_url.clone(), "".to_string(), "m".to_string());

    let err = client
        .chat_with_tools(vec![Message::user("hi")], vec![])
        .await
        .expect_err("500 should hard-fail");

    let msg = format!("{:?}", err);
    assert!(
        msg.contains("500"),
        "expected 500-derived error, got {}",
        msg
    );
    assert_eq!(mock.requests().len(), 1, "500 must not retry");
}
