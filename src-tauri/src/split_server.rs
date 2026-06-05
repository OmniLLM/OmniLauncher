use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{broadcast, Mutex, RwLock},
};

use crate::{
    ai::{client::AiClient, router::ConversationContext},
    launcher_config::LauncherConfig,
    live_server::LiveResponse,
    plugins::QueryResult,
    save_settings, AppSettings, SkillManager,
};

#[derive(Clone)]
pub struct SplitServerState {
    pub plugin_manager: Arc<Mutex<crate::PluginManager>>,
    pub ai_client: Arc<Mutex<AiClient>>,
    pub settings: Arc<Mutex<AppSettings>>,
    pub conversation: Arc<Mutex<ConversationContext>>,
    pub ai_in_flight: Arc<tokio::sync::Semaphore>,
    pub current_ai_task: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    pub skill_manager: Arc<Mutex<SkillManager>>,
    pub event_bus: EventBus,
    pub latest_selection: Arc<Mutex<Option<SelectionPayload>>>,
}

#[derive(Clone, Default)]
pub struct EventBus {
    inner: Arc<RwLock<HashMap<String, broadcast::Sender<String>>>>,
}

impl EventBus {
    async fn sender(&self, name: &str) -> broadcast::Sender<String> {
        let mut guard = self.inner.write().await;
        guard
            .entry(name.to_string())
            .or_insert_with(|| broadcast::channel(64).0)
            .clone()
    }

    pub async fn emit_json<T: Serialize>(&self, name: &str, payload: &T) {
        if let Ok(json) = serde_json::to_string(payload) {
            let sender = self.sender(name).await;
            let _ = sender.send(json);
        }
    }

    pub async fn subscribe(&self, name: &str) -> broadcast::Receiver<String> {
        self.sender(name).await.subscribe()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionPayload {
    pub token: String,
    pub selection: String,
}

#[derive(Debug, Deserialize)]
struct SearchRequest {
    query: String,
}

#[derive(Debug, Deserialize)]
struct ModelsRequest {
    base_url: String,
    api_key: String,
}

#[derive(Debug, Deserialize)]
struct SessionRequest {
    session_id: i64,
}

#[derive(Debug, Deserialize)]
struct FavoriteRequest {
    result: QueryResult,
}

#[derive(Debug, Deserialize)]
struct SaveSettingsRequest {
    ai_base_url: String,
    ai_model: String,
    ai_api_key: String,
    theme: String,
    hotkey: String,
    max_results: usize,
    background_url: String,
}

#[derive(Debug, Deserialize)]
struct AiQueryRequest {
    query: String,
}

#[derive(Debug, Deserialize)]
struct ExecuteResultRequest {
    result: QueryResult,
}

fn json_response<T: Serialize>(value: &T) -> LiveResponse {
    match serde_json::to_string(value) {
        Ok(body) => LiveResponse::json(body),
        Err(error) => LiveResponse::text("500 Internal Server Error", error.to_string()),
    }
}

fn parse_json<T: for<'de> Deserialize<'de>>(body: &str) -> Result<T, LiveResponse> {
    serde_json::from_str(body)
        .map_err(|error| LiveResponse::text("400 Bad Request", format!("Invalid JSON: {error}")))
}

async fn read_body(request: &str) -> String {
    request
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or_default()
        .to_string()
}

pub async fn spawn_split_server(state: SplitServerState, host: String, port: u16) {
    let listener = match TcpListener::bind((host.as_str(), port)).await {
        Ok(listener) => listener,
        Err(error) => {
            log::error!(
                "failed to bind split backend on {}:{}: {}",
                host,
                port,
                error
            );
            return;
        }
    };

    log::info!("split backend listening on http://{}:{}", host, port);

    loop {
        let (mut stream, addr) = match listener.accept().await {
            Ok(parts) => parts,
            Err(error) => {
                log::warn!("split backend accept error: {}", error);
                continue;
            }
        };
        let state = state.clone();
        tokio::spawn(async move {
            let mut buf = vec![0_u8; 1024 * 1024];
            let read_len = match stream.read(&mut buf).await {
                Ok(size) => size,
                Err(error) => {
                    log::debug!("split backend read error from {}: {}", addr, error);
                    return;
                }
            };
            if read_len == 0 {
                return;
            }
            let request = String::from_utf8_lossy(&buf[..read_len]).to_string();
            let first_line = request.lines().next().unwrap_or_default();
            let mut parts = first_line.split_whitespace();
            let method = parts.next().unwrap_or("GET");
            let target = parts.next().unwrap_or("/");
            let (path, query) = split_path_query(target);

            if let Some(event_name) = path.strip_prefix("/api/events/") {
                let mut receiver = state.event_bus.subscribe(event_name).await;
                let headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nAccess-Control-Allow-Origin: *\r\n\r\n".to_string();
                if stream.write_all(headers.as_bytes()).await.is_err() {
                    return;
                }
                while let Ok(message) = receiver.recv().await {
                    let payload = format!("data: {}\n\n", message);
                    if stream.write_all(payload.as_bytes()).await.is_err() {
                        break;
                    }
                }
                let _ = stream.shutdown().await;
                return;
            }

            let response = handle_request(&state, method, &path, &query, &request).await;
            let bytes = encode_response(response);
            let _ = stream.write_all(&bytes).await;
            let _ = stream.shutdown().await;
        });
    }
}

async fn handle_request(
    state: &SplitServerState,
    method: &str,
    path: &str,
    _query: &str,
    request: &str,
) -> LiveResponse {
    match (method, path) {
        ("GET", "/health") => LiveResponse::json("{\"ok\":true}".to_string()),
        ("POST", "/api/search") => {
            let body = read_body(request).await;
            match parse_json::<SearchRequest>(&body) {
                Ok(input) => json_response(&search_backend(input.query, state).await),
                Err(error) => error,
            }
        }
        ("GET", "/api/settings") => {
            let settings = state.settings.lock().await.clone();
            json_response(&settings)
        }
        ("POST", "/api/settings") => {
            let body = read_body(request).await;
            match parse_json::<SaveSettingsRequest>(&body) {
                Ok(input) => {
                    let updated = AppSettings {
                        ai_base_url: input.ai_base_url,
                        ai_model: input.ai_model,
                        ai_api_key: input.ai_api_key,
                        theme: input.theme,
                        hotkey: input.hotkey,
                        max_results: input.max_results,
                        background_url: input.background_url,
                        ..state.settings.lock().await.clone()
                    };
                    {
                        let mut settings = state.settings.lock().await;
                        *settings = updated.clone();
                    }
                    {
                        let mut client = state.ai_client.lock().await;
                        *client = AiClient::new(
                            updated.ai_base_url.clone(),
                            updated.ai_api_key.clone(),
                            updated.ai_model.clone(),
                        );
                    }
                    let ok = save_settings(&updated);
                    state
                        .event_bus
                        .emit_json("omnilauncher://settings-saved", &updated)
                        .await;
                    json_response(&ok)
                }
                Err(error) => error,
            }
        }
        ("POST", "/api/models") => {
            let body = read_body(request).await;
            match parse_json::<ModelsRequest>(&body) {
                Ok(input) => match list_models_backend(input.base_url, input.api_key).await {
                    Ok(models) => json_response(&models),
                    Err(error) => LiveResponse::text("500 Internal Server Error", error),
                },
                Err(error) => error,
            }
        }
        ("GET", "/api/launcher-config") => json_response(&LauncherConfig::current()),
        ("GET", "/api/favorites") => json_response(&crate::db::favorites::list_favorites()),
        ("POST", "/api/favorites") => {
            let body = read_body(request).await;
            match parse_json::<FavoriteRequest>(&body) {
                Ok(input) => match crate::db::favorites::add_favorite(&input.result) {
                    Ok(()) => json_response(&true),
                    Err(error) => LiveResponse::text("500 Internal Server Error", error),
                },
                Err(error) => error,
            }
        }
        ("DELETE", path) if path.starts_with("/api/favorites/") => {
            let id = path.trim_start_matches("/api/favorites/");
            match crate::db::favorites::remove_favorite(id) {
                Ok(()) => json_response(&true),
                Err(error) => LiveResponse::text("500 Internal Server Error", error),
            }
        }
        ("GET", "/api/sessions") => json_response(&crate::db::conversation::list_sessions()),
        ("GET", "/api/sessions/current") => {
            let ctx = state.conversation.lock().await;
            json_response(&ctx.session_id)
        }
        ("POST", "/api/sessions/switch") => {
            let body = read_body(request).await;
            match parse_json::<SessionRequest>(&body) {
                Ok(input) => match switch_session_backend(input.session_id, state).await {
                    Ok(payload) => json_response(&payload),
                    Err(error) => LiveResponse::text("500 Internal Server Error", error),
                },
                Err(error) => error,
            }
        }
        ("POST", "/api/sessions/delete") => {
            let body = read_body(request).await;
            match parse_json::<SessionRequest>(&body) {
                Ok(input) => match delete_session_backend(input.session_id, state).await {
                    Ok(id) => json_response(&id),
                    Err(error) => LiveResponse::text("500 Internal Server Error", error),
                },
                Err(error) => error,
            }
        }
        ("POST", "/api/sessions/clear") => match clear_conversation_backend(state).await {
            Ok(ok) => json_response(&ok),
            Err(error) => LiveResponse::text("500 Internal Server Error", error),
        },
        ("POST", "/api/ai/query") => {
            let body = read_body(request).await;
            match parse_json::<AiQueryRequest>(&body) {
                Ok(input) => match ai_query_backend(input.query, state.clone()).await {
                    Ok(()) => json_response(&true),
                    Err(error) => LiveResponse::text("500 Internal Server Error", error),
                },
                Err(error) => error,
            }
        }
        ("POST", "/api/ai/cancel") => match ai_cancel_backend(state).await {
            Ok(ok) => json_response(&ok),
            Err(error) => LiveResponse::text("500 Internal Server Error", error),
        },
        ("POST", "/api/execute-result") => {
            let body = read_body(request).await;
            match parse_json::<ExecuteResultRequest>(&body) {
                Ok(input) => match execute_result_backend(input.result, state).await {
                    Ok(ok) => json_response(&ok),
                    Err(error) => LiveResponse::text("500 Internal Server Error", error),
                },
                Err(error) => error,
            }
        }
        ("GET", "/api/selection/latest") => {
            let payload = state.latest_selection.lock().await.clone();
            json_response(&payload)
        }
        ("POST", "/api/window/hide") => json_response(&true),
        _ => LiveResponse::text("404 Not Found", "Not Found".to_string()),
    }
}

fn split_path_query(target: &str) -> (String, String) {
    match target.split_once('?') {
        Some((path, query)) => (normalize_path(path), query.to_string()),
        None => (normalize_path(target), String::new()),
    }
}

fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        "/".to_string()
    } else {
        format!("/{}", trimmed.trim_matches('/'))
    }
}

fn encode_response(response: LiveResponse) -> Vec<u8> {
    let header = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nCache-Control: no-store, no-cache, must-revalidate\r\nPragma: no-cache\r\nExpires: 0\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    );
    [header.into_bytes(), response.body.into_bytes()].concat()
}

pub async fn search_backend(query: String, state: &SplitServerState) -> Vec<QueryResult> {
    let pm = state.plugin_manager.lock().await;
    pm.query_all(&query).await
}

pub async fn list_models_backend(base_url: String, api_key: String) -> Result<Vec<String>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let mut req = client.get(&url);
    if !api_key.is_empty() {
        req = req.bearer_auth(&api_key);
    }

    let response = req.send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, text));
    }

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    Ok(json["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default())
}

pub async fn clear_conversation_backend(state: &SplitServerState) -> Result<bool, String> {
    let new_id = crate::db::conversation::start_new_session(None);
    let mut ctx = state.conversation.lock().await;
    ctx.clear();
    ctx.session_id = new_id;
    Ok(true)
}

pub async fn switch_session_backend(
    session_id: i64,
    state: &SplitServerState,
) -> Result<Vec<serde_json::Value>, String> {
    crate::db::conversation::touch_for_switch(session_id);
    let msgs = crate::db::conversation::load_recent_for_session(session_id, 200);
    let mut ctx = state.conversation.lock().await;
    ctx.clear();
    ctx.session_id = session_id;
    let take_n = ctx.max_turns * 2;
    let start = msgs.len().saturating_sub(take_n);
    ctx.messages = msgs[start..].to_vec();
    Ok(msgs
        .iter()
        .map(|m| serde_json::json!({ "role": m.role, "content": m.content_str() }))
        .collect())
}

pub async fn delete_session_backend(
    session_id: i64,
    state: &SplitServerState,
) -> Result<i64, String> {
    let ok = crate::db::conversation::delete_session(session_id);
    if !ok {
        return Err("Failed to delete session".to_string());
    }
    let mut ctx = state.conversation.lock().await;
    if ctx.session_id == session_id {
        let new_id = crate::db::conversation::current_session_id();
        ctx.clear();
        ctx.session_id = new_id;
        Ok(new_id)
    } else {
        Ok(ctx.session_id)
    }
}

pub async fn ai_cancel_backend(state: &SplitServerState) -> Result<bool, String> {
    let handle = {
        let mut slot = state.current_ai_task.lock().await;
        slot.take()
    };
    if let Some(handle) = handle {
        handle.abort();
        let _ = handle.await;
        state
            .event_bus
            .emit_json("omnilauncher://ai-error", &"Cancelled by user".to_string())
            .await;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub async fn ai_query_backend(query: String, state: SplitServerState) -> Result<(), String> {
    use crate::ai::router::Router;

    let permit = state
        .ai_in_flight
        .clone()
        .try_acquire_owned()
        .map_err(|_| "AI response is still in progress".to_string())?;

    let session_id = {
        let mut ctx = state.conversation.lock().await;
        if ctx.session_id == 0 {
            ctx.session_id = crate::db::conversation::current_session_id();
        }
        ctx.add_user(&query);
        ctx.session_id
    };
    crate::db::conversation::save_turn(session_id, "user", &query);

    let pm = state.plugin_manager.clone();
    let ai_client = state.ai_client.clone();
    let conversation = state.conversation.clone();
    let skill_mgr = state.skill_manager.clone();
    let event_bus = state.event_bus.clone();

    let handle = tauri::async_runtime::spawn(async move {
        let _permit = permit;
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<String>(64);
        let progress_bus = event_bus.clone();
        tauri::async_runtime::spawn(async move {
            let mut iteration = 0u32;
            while let Some(tool_name) = progress_rx.recv().await {
                iteration += 1;
                progress_bus
                    .emit_json(
                        "omnilauncher://ai-tool-call",
                        &serde_json::json!({ "tool": tool_name, "iteration": iteration }),
                    )
                    .await;
            }
        });

        let routed = std::panic::AssertUnwindSafe(async {
            let pm_lock = pm.lock().await;
            let client = ai_client.lock().await;
            let ctx = conversation.lock().await;
            let mut skill_lock = skill_mgr.lock().await;
            Router::ai_route(
                &query,
                &pm_lock,
                &client,
                &ctx,
                &mut skill_lock,
                Some(progress_tx),
            )
            .await
        });
        let response = match futures_util::FutureExt::catch_unwind(routed).await {
            Ok(resp) => resp,
            Err(panic) => {
                let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = panic.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "AI task panicked".to_string()
                };
                event_bus.emit_json("omnilauncher://ai-error", &msg).await;
                return;
            }
        };

        let sid = {
            let mut ctx = conversation.lock().await;
            ctx.add_assistant(&response.content);
            ctx.session_id
        };
        crate::db::conversation::save_turn(sid, "assistant", &response.content);
        event_bus
            .emit_json("omnilauncher://ai-done", &response)
            .await;
    });

    {
        let mut slot = state.current_ai_task.lock().await;
        *slot = Some(handle);
    }

    Ok(())
}

pub async fn execute_result_backend(
    result: QueryResult,
    state: &SplitServerState,
) -> Result<bool, String> {
    let action_data = result.action_data.clone();

    let success = match result.action_type.as_str() {
        "plugin_execute" => {
            let (plugin_name, inner_id) = match result.id.split_once("::") {
                Some((name, id)) => (name.to_string(), id.to_string()),
                None => return Ok(false),
            };
            let pm = state.plugin_manager.lock().await;
            pm.execute_action(&plugin_name, &inner_id, &action_data)
                .await
                .is_some()
        }
        "copy" => true,
        "todo_add" => {
            let pm = state.plugin_manager.lock().await;
            pm.execute_tool(
                "todo_memory",
                serde_json::json!({ "action": "add", "text": result.action_data }),
            )
            .await;
            true
        }
        "todo_remove" => {
            let pm = state.plugin_manager.lock().await;
            pm.execute_tool(
                "todo_memory",
                serde_json::json!({ "action": "remove", "text": result.action_data }),
            )
            .await;
            true
        }
        "todo_done" => {
            let pm = state.plugin_manager.lock().await;
            pm.execute_tool(
                "todo_memory",
                serde_json::json!({ "action": "done", "text": result.action_data }),
            )
            .await;
            true
        }
        "todo_undone" => {
            let pm = state.plugin_manager.lock().await;
            pm.execute_tool(
                "todo_memory",
                serde_json::json!({ "action": "undone", "text": result.action_data }),
            )
            .await;
            true
        }
        _ => false,
    };

    Ok(success)
}
