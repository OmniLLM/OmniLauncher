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

// ─── Skills request payloads ────────────────────────────────────────────────
#[derive(Debug, Deserialize)]
struct SkillSourceRequest {
    source: String,
}

#[derive(Debug, Deserialize)]
struct SkillNameRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
struct SkillPinRequest {
    name: String,
    pinned: bool,
}

#[derive(Debug, Deserialize)]
struct ConsolidationApplyRequest {
    proposal: crate::skills::consolidate::Proposal,
}

// ─── Plugin request payloads ────────────────────────────────────────────────
#[derive(Debug, Deserialize)]
struct PluginInstallRequest {
    source: String,
    #[serde(default)]
    target_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PluginNameRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
struct CollectionUpdateRequest {
    #[serde(default)]
    collection_source: Option<String>,
    #[serde(default)]
    repo_dirs: Vec<String>,
    #[serde(default)]
    git_repo_dirs: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CollectionRemoveRequest {
    #[serde(default)]
    repo_dirs: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RuntimeDepInstallRequest {
    id: String,
}

// ─── Slash + vision request payloads ────────────────────────────────────────
#[derive(Debug, Deserialize)]
struct SlashRequest {
    query: String,
}

#[derive(Debug, Deserialize)]
struct VisionRequest {
    prompt: String,
    image_base64: String,
}

fn json_response<T: Serialize>(value: &T) -> LiveResponse {
    match serde_json::to_string(value) {
        Ok(body) => LiveResponse::json(body),
        Err(error) => LiveResponse::text("500 Internal Server Error", error.to_string()),
    }
}

fn parse_json<T: for<'de> Deserialize<'de>>(body: &str) -> Result<T, LiveResponse> {
    serde_json::from_str(body).map_err(|error| {
        log::warn!(
            "split backend JSON parse error: {} (body_bytes={})",
            error,
            body.len()
        );
        LiveResponse::text("400 Bad Request", format!("Invalid JSON: {error}"))
    })
}

async fn read_body(request: &str) -> String {
    request
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or_default()
        .to_string()
}

fn request_body_len(request: &str) -> usize {
    request
        .split("\r\n\r\n")
        .nth(1)
        .map(str::len)
        .unwrap_or_default()
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
            log::debug!(
                "split backend request from {}: method={} path={} query={} bytes={}",
                addr,
                method,
                path,
                query,
                read_len
            );

            if let Some(event_name) = event_name_from_path(&path) {
                log::debug!("split backend SSE subscribe from {}: {}", addr, event_name);
                let mut receiver = state.event_bus.subscribe(&event_name).await;
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
            log::debug!(
                "split backend response to {}: method={} path={} status={} body_bytes={}",
                addr,
                method,
                path,
                response.status,
                response.body.len()
            );
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
    if method != "GET" && method != "OPTIONS" {
        log::trace!(
            "split backend request body: method={} path={} body_bytes={}",
            method,
            path,
            request_body_len(request)
        );
    }
    match (method, path) {
        ("OPTIONS", _) => LiveResponse::text("204 No Content", String::new()),
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
            log::debug!(
                "split backend get settings: base_url={} model={} theme={} max_results={} background_url={}",
                settings.ai_base_url,
                settings.ai_model,
                settings.theme,
                settings.max_results,
                settings.background_url
            );
            json_response(&settings)
        }
        ("POST", "/api/settings") => {
            let body = read_body(request).await;
            match parse_json::<SaveSettingsRequest>(&body) {
                Ok(input) => {
                    log::debug!(
                        "split backend save settings request: base_url={} model={} theme={} max_results={} background_url={} api_key_present={}",
                        input.ai_base_url,
                        input.ai_model,
                        input.theme,
                        input.max_results,
                        input.background_url,
                        !input.ai_api_key.trim().is_empty()
                    );
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
                            updated.resolve_ai_api_key(),
                            updated.ai_model.clone(),
                        );
                    }
                    let ok = save_settings(&updated);
                    if ok {
                        log::info!("split backend saved settings successfully");
                    } else {
                        log::error!("split backend failed to save settings");
                    }
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
        // ─── Skills ─────────────────────────────────────────────────────────
        ("GET", "/api/skills") => {
            let mgr = state.skill_manager.lock().await;
            let metas: Vec<crate::SkillInfo> = mgr
                .list_meta()
                .into_iter()
                .map(crate::SkillInfo::from)
                .collect();
            json_response(&metas)
        }
        ("GET", "/api/skills/usage") => json_response(&crate::skills::curator::snapshot()),
        ("POST", "/api/skills/install") => {
            let body = read_body(request).await;
            match parse_json::<SkillSourceRequest>(&body) {
                Ok(input) => {
                    let mgr = state.skill_manager.clone();
                    let res = tokio::task::spawn_blocking(move || {
                        let mut mgr = mgr.blocking_lock();
                        if input.source.starts_with("http://")
                            || input.source.starts_with("https://")
                        {
                            mgr.install_from_url(&input.source)
                        } else {
                            mgr.install_from_path(&input.source)
                        }
                    })
                    .await
                    .map_err(|e| format!("install task failed: {e}"));
                    match res {
                        Ok(Ok(name)) => json_response(&name),
                        Ok(Err(e)) | Err(e) => LiveResponse::text("500 Internal Server Error", e),
                    }
                }
                Err(error) => error,
            }
        }
        ("POST", "/api/skills/update") => {
            let body = read_body(request).await;
            match parse_json::<SkillNameRequest>(&body) {
                Ok(input) => {
                    let mgr = state.skill_manager.clone();
                    let res = tokio::task::spawn_blocking(move || {
                        let mut mgr = mgr.blocking_lock();
                        mgr.update_skill(&input.name)
                    })
                    .await
                    .map_err(|e| format!("update task failed: {e}"));
                    match res {
                        Ok(Ok(msg)) => json_response(&msg),
                        Ok(Err(e)) | Err(e) => LiveResponse::text("500 Internal Server Error", e),
                    }
                }
                Err(error) => error,
            }
        }
        ("POST", "/api/skills/delete") => {
            let body = read_body(request).await;
            match parse_json::<SkillNameRequest>(&body) {
                Ok(input) => {
                    let mut mgr = state.skill_manager.lock().await;
                    match mgr.delete_skill(&input.name) {
                        Ok(msg) => json_response(&msg),
                        Err(e) => LiveResponse::text("500 Internal Server Error", e),
                    }
                }
                Err(error) => error,
            }
        }
        ("POST", "/api/skills/pin") => {
            let body = read_body(request).await;
            match parse_json::<SkillPinRequest>(&body) {
                Ok(input) => {
                    crate::skills::curator::set_pinned(&input.name, input.pinned);
                    json_response(&true)
                }
                Err(error) => error,
            }
        }
        ("POST", "/api/skills/curator/run") => {
            let names = {
                let mgr = state.skill_manager.lock().await;
                mgr.user_skill_names()
            };
            match tokio::task::spawn_blocking(move || crate::skills::curator::evaluate(&names))
                .await
            {
                Ok(report) => json_response(&serde_json::json!({
                    "marked_stale": report.marked_stale,
                    "marked_archived": report.marked_archived,
                    "seen_new": report.seen_new,
                    "total_tracked": report.total_tracked,
                })),
                Err(e) => LiveResponse::text(
                    "500 Internal Server Error",
                    format!("curator task failed: {e}"),
                ),
            }
        }
        ("POST", "/api/skills/consolidation/propose") => {
            let skills_clone: Vec<crate::skills::Skill> = {
                let mgr = state.skill_manager.lock().await;
                let user_names = mgr.user_skill_names();
                mgr.list_meta()
                    .iter()
                    .filter(|m| user_names.iter().any(|n| n == &m.name))
                    .filter_map(|m| mgr.get_by_name(&m.name).cloned())
                    .collect()
            };
            let ai = state.ai_client.lock().await;
            match crate::skills::consolidate::propose(&skills_clone, &ai).await {
                Ok(proposals) => json_response(&proposals),
                Err(e) => LiveResponse::text(
                    "500 Internal Server Error",
                    format!("LLM propose failed: {e}"),
                ),
            }
        }
        ("POST", "/api/skills/consolidation/apply") => {
            let body = read_body(request).await;
            match parse_json::<ConsolidationApplyRequest>(&body) {
                Ok(input) => {
                    let mut mgr = state.skill_manager.lock().await;
                    match crate::skills::consolidate::apply(&input.proposal, &mut mgr) {
                        Ok(outcome) => json_response(&outcome),
                        Err(e) => LiveResponse::text("500 Internal Server Error", e),
                    }
                }
                Err(error) => error,
            }
        }
        // ─── Plugins ────────────────────────────────────────────────────────
        ("GET", "/api/plugins/collections") => {
            json_response(&crate::plugins::plugin_manager_cmd::list_plugin_collections())
        }
        ("GET", "/api/plugins/runtime-deps") => {
            json_response(&crate::plugins::runtime_deps::list_runtime_dependencies())
        }
        ("POST", "/api/plugins/install") => {
            let body = read_body(request).await;
            match parse_json::<PluginInstallRequest>(&body) {
                Ok(input) => {
                    match crate::plugins::plugin_manager_cmd::install_plugin(
                        input.source,
                        input.target_dir,
                    )
                    .await
                    {
                        Ok(msg) => {
                            reload_external_plugins_state(state).await;
                            json_response(&msg)
                        }
                        Err(e) => LiveResponse::text("500 Internal Server Error", e),
                    }
                }
                Err(error) => error,
            }
        }
        ("POST", "/api/plugins/update") => {
            let body = read_body(request).await;
            match parse_json::<PluginNameRequest>(&body) {
                Ok(input) => {
                    match crate::plugins::plugin_manager_cmd::update_plugin(input.name).await {
                        Ok(msg) => {
                            reload_external_plugins_state(state).await;
                            json_response(&msg)
                        }
                        Err(e) => LiveResponse::text("500 Internal Server Error", e),
                    }
                }
                Err(error) => error,
            }
        }
        ("POST", "/api/plugins/collections/update") => {
            let body = read_body(request).await;
            match parse_json::<CollectionUpdateRequest>(&body) {
                Ok(input) => {
                    match crate::plugins::plugin_manager_cmd::update_plugin_collection_all(
                        input.collection_source,
                        input.repo_dirs,
                        input.git_repo_dirs,
                    )
                    .await
                    {
                        Ok(res) => {
                            reload_external_plugins_state(state).await;
                            json_response(&res)
                        }
                        Err(e) => LiveResponse::text("500 Internal Server Error", e),
                    }
                }
                Err(error) => error,
            }
        }
        ("POST", "/api/plugins/collections/remove") => {
            let body = read_body(request).await;
            match parse_json::<CollectionRemoveRequest>(&body) {
                Ok(input) => match crate::plugins::plugin_manager_cmd::remove_plugin_collection(
                    input.repo_dirs,
                )
                .await
                {
                    Ok(res) => {
                        reload_external_plugins_state(state).await;
                        json_response(&res)
                    }
                    Err(e) => LiveResponse::text("500 Internal Server Error", e),
                },
                Err(error) => error,
            }
        }
        ("POST", "/api/plugins/runtime-deps/install") => {
            let body = read_body(request).await;
            match parse_json::<RuntimeDepInstallRequest>(&body) {
                Ok(input) => match install_runtime_dep_backend(&input.id, state).await {
                    Ok(msg) => json_response(&msg),
                    Err(e) => LiveResponse::text("500 Internal Server Error", e),
                },
                Err(error) => error,
            }
        }
        // ─── Slash commands ─────────────────────────────────────────────────
        ("POST", "/api/slash/preview") => {
            let body = read_body(request).await;
            match parse_json::<SlashRequest>(&body) {
                Ok(input) => {
                    let pm = state.plugin_manager.lock().await;
                    json_response(&slash_preview_backend(&input.query, &pm).await)
                }
                Err(error) => error,
            }
        }
        ("POST", "/api/slash/execute") => {
            let body = read_body(request).await;
            match parse_json::<SlashRequest>(&body) {
                Ok(input) => {
                    let pm = state.plugin_manager.lock().await;
                    let mut skill_mgr = state.skill_manager.lock().await;
                    let resp =
                        crate::ai::router::Router::slash_command(&input.query, &pm, &mut skill_mgr)
                            .await;
                    json_response(&resp)
                }
                Err(error) => error,
            }
        }
        // ─── Vision ─────────────────────────────────────────────────────────
        ("POST", "/api/vision/analyze") => {
            let body = read_body(request).await;
            match parse_json::<VisionRequest>(&body) {
                Ok(input) => {
                    match vision_analyze_backend(&input.prompt, &input.image_base64, state).await {
                        Ok(text) => json_response(&text),
                        Err(e) => LiveResponse::text("500 Internal Server Error", e),
                    }
                }
                Err(error) => error,
            }
        }
        _ => LiveResponse::text("404 Not Found", "Not Found".to_string()),
    }
}

fn split_path_query(target: &str) -> (String, String) {
    match target.split_once('?') {
        Some((path, query)) => (normalize_path(path), query.to_string()),
        None => (normalize_path(target), String::new()),
    }
}

fn event_name_from_path(path: &str) -> Option<String> {
    path.strip_prefix("/api/events/")
        .map(percent_decode)
        .filter(|name| !name.is_empty())
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }

        out.push(bytes[i]);
        i += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
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
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nCache-Control: no-store, no-cache, must-revalidate\r\nPragma: no-cache\r\nExpires: 0\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    );
    [header.into_bytes(), response.body.into_bytes()].concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── encode_response tests ──────────────────────────────────────────

    #[test]
    fn encoded_response_includes_cors_preflight_headers() {
        let response = LiveResponse::text("204 No Content", String::new());
        let encoded = String::from_utf8(encode_response(response)).unwrap();

        assert!(encoded.starts_with("HTTP/1.1 204 No Content\r\n"));
        assert!(encoded.contains("Access-Control-Allow-Origin: *\r\n"));
        assert!(encoded.contains("Access-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\n"));
        assert!(encoded.contains("Access-Control-Allow-Headers: Content-Type\r\n"));
    }

    #[test]
    fn encoded_json_response_has_correct_content_type() {
        let response = LiveResponse::json(r#"{"ok":true}"#.to_string());
        let encoded = String::from_utf8(encode_response(response)).unwrap();

        assert!(encoded.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(encoded.contains("Content-Type: application/json; charset=utf-8\r\n"));
        assert!(encoded.contains(r#"{"ok":true}"#));
    }

    #[test]
    fn encoded_response_has_correct_content_length() {
        let body = "Hello, World!";
        let response = LiveResponse::text("200 OK", body.to_string());
        let encoded = String::from_utf8(encode_response(response)).unwrap();

        let expected_header = format!("Content-Length: {}\r\n", body.len());
        assert!(encoded.contains(&expected_header));
    }

    #[test]
    fn encoded_response_includes_cache_control_headers() {
        let response = LiveResponse::json("{}".to_string());
        let encoded = String::from_utf8(encode_response(response)).unwrap();

        assert!(encoded.contains("Cache-Control: no-store, no-cache, must-revalidate\r\n"));
        assert!(encoded.contains("Pragma: no-cache\r\n"));
        assert!(encoded.contains("Expires: 0\r\n"));
    }

    #[test]
    fn encoded_response_includes_connection_close() {
        let response = LiveResponse::text("200 OK", "test".to_string());
        let encoded = String::from_utf8(encode_response(response)).unwrap();

        assert!(encoded.contains("Connection: close\r\n"));
    }

    #[test]
    fn encoded_response_empty_body() {
        let response = LiveResponse::text("204 No Content", String::new());
        let encoded = String::from_utf8(encode_response(response)).unwrap();

        assert!(encoded.contains("Content-Length: 0\r\n"));
        // Headers end with \r\n\r\n, body is empty
        assert!(encoded.ends_with("\r\n\r\n"));
    }

    // ── split_path_query tests ─────────────────────────────────────────

    #[test]
    fn split_path_query_no_query_string() {
        let (path, query) = split_path_query("/api/health");
        assert_eq!(path, "/api/health");
        assert_eq!(query, "");
    }

    #[test]
    fn split_path_query_with_query_string() {
        let (path, query) = split_path_query("/api/search?q=hello&limit=10");
        assert_eq!(path, "/api/search");
        assert_eq!(query, "q=hello&limit=10");
    }

    #[test]
    fn split_path_query_root() {
        let (path, query) = split_path_query("/");
        assert_eq!(path, "/");
        assert_eq!(query, "");
    }

    #[test]
    fn split_path_query_empty() {
        let (path, query) = split_path_query("");
        assert_eq!(path, "/");
        assert_eq!(query, "");
    }

    #[test]
    fn split_path_query_with_empty_query() {
        let (path, query) = split_path_query("/api/test?");
        assert_eq!(path, "/api/test");
        assert_eq!(query, "");
    }

    // ── normalize_path tests ───────────────────────────────────────────

    #[test]
    fn normalize_path_strips_trailing_slash() {
        assert_eq!(normalize_path("/api/test/"), "/api/test");
    }

    #[test]
    fn normalize_path_handles_root() {
        assert_eq!(normalize_path("/"), "/");
    }

    #[test]
    fn normalize_path_handles_empty() {
        assert_eq!(normalize_path(""), "/");
    }

    #[test]
    fn normalize_path_preserves_normal_path() {
        assert_eq!(normalize_path("/api/ai/query"), "/api/ai/query");
    }

    #[test]
    fn normalize_path_trims_whitespace() {
        assert_eq!(normalize_path("  /api/test  "), "/api/test");
    }

    // ── read_body tests ────────────────────────────────────────────────

    #[tokio::test]
    async fn read_body_extracts_body_after_headers() {
        let raw_request = "POST /api/search HTTP/1.1\r\nContent-Type: application/json\r\n\r\n{\"query\":\"hello\"}";
        let body = read_body(raw_request).await;
        assert_eq!(body, "{\"query\":\"hello\"}");
    }

    #[tokio::test]
    async fn read_body_returns_empty_for_no_body() {
        let raw_request = "GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let body = read_body(raw_request).await;
        assert_eq!(body, "");
    }

    #[tokio::test]
    async fn read_body_returns_empty_for_malformed_request() {
        let raw_request = "GET /health HTTP/1.1";
        let body = read_body(raw_request).await;
        assert_eq!(body, "");
    }

    // ── request_body_len tests ─────────────────────────────────────────

    #[test]
    fn request_body_len_with_body() {
        let raw = "POST /api HTTP/1.1\r\n\r\n{\"a\":1}";
        assert_eq!(request_body_len(raw), 7);
    }

    #[test]
    fn request_body_len_no_body() {
        let raw = "GET / HTTP/1.1\r\n\r\n";
        assert_eq!(request_body_len(raw), 0);
    }

    #[test]
    fn request_body_len_no_separator() {
        let raw = "GET / HTTP/1.1";
        assert_eq!(request_body_len(raw), 0);
    }

    // ── parse_json tests ───────────────────────────────────────────────

    #[test]
    fn parse_json_valid() {
        let result: Result<SearchRequest, _> = parse_json(r#"{"query":"hello"}"#);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().query, "hello");
    }

    #[test]
    fn parse_json_invalid_returns_400() {
        let result: Result<SearchRequest, _> = parse_json("not json");
        assert!(result.is_err());
        let error_response = result.unwrap_err();
        assert_eq!(error_response.status, "400 Bad Request");
        assert!(error_response.body.contains("Invalid JSON"));
    }

    #[test]
    fn parse_json_missing_field_returns_400() {
        let result: Result<SearchRequest, _> = parse_json(r#"{"wrong_field":"hello"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn parse_json_empty_string_returns_400() {
        let result: Result<SearchRequest, _> = parse_json("");
        assert!(result.is_err());
    }

    // ── json_response tests ────────────────────────────────────────────

    #[test]
    fn json_response_serializes_value() {
        let response = json_response(&serde_json::json!({"ok": true}));
        assert_eq!(response.status, "200 OK");
        assert_eq!(response.content_type, "application/json; charset=utf-8");
        assert!(response.body.contains("\"ok\":true") || response.body.contains("\"ok\": true"));
    }

    #[test]
    fn json_response_serializes_string() {
        let response = json_response(&"hello world".to_string());
        assert_eq!(response.status, "200 OK");
        assert!(response.body.contains("hello world"));
    }

    #[test]
    fn json_response_serializes_bool() {
        let response = json_response(&true);
        assert_eq!(response.body, "true");
    }

    #[test]
    fn json_response_serializes_vec() {
        let items: Vec<String> = vec!["a".into(), "b".into()];
        let response = json_response(&items);
        assert!(response.body.contains("["));
        assert!(response.body.contains("\"a\""));
    }

    // ── EventBus tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn event_bus_emit_and_receive() {
        let bus = EventBus::default();
        let mut rx = bus.subscribe("test-event").await;
        bus.emit_json("test-event", &"hello").await;
        let msg = rx.recv().await.unwrap();
        assert!(msg.contains("hello"));
    }

    #[tokio::test]
    async fn event_bus_multiple_subscribers() {
        let bus = EventBus::default();
        let mut rx1 = bus.subscribe("multi").await;
        let mut rx2 = bus.subscribe("multi").await;
        bus.emit_json("multi", &42).await;
        assert_eq!(rx1.recv().await.unwrap(), "42");
        assert_eq!(rx2.recv().await.unwrap(), "42");
    }

    #[tokio::test]
    async fn event_bus_different_channels_are_isolated() {
        let bus = EventBus::default();
        let mut rx_a = bus.subscribe("channel-a").await;
        let _rx_b = bus.subscribe("channel-b").await;
        bus.emit_json("channel-a", &"only-a").await;
        let msg = rx_a.recv().await.unwrap();
        assert!(msg.contains("only-a"));
        // channel-b should not have received anything
    }

    #[tokio::test]
    async fn event_bus_json_serialization() {
        let bus = EventBus::default();
        let mut rx = bus.subscribe("json-test").await;

        #[derive(Serialize)]
        struct Payload {
            tool: String,
            iteration: u32,
        }
        let payload = Payload {
            tool: "calculator".to_string(),
            iteration: 3,
        };
        bus.emit_json("json-test", &payload).await;
        let msg = rx.recv().await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["tool"], "calculator");
        assert_eq!(parsed["iteration"], 3);
    }

    #[tokio::test]
    async fn event_bus_late_subscriber_misses_old_messages() {
        let bus = EventBus::default();
        // Emit before subscribing
        bus.emit_json("late", &"early-message").await;
        let mut rx = bus.subscribe("late").await;
        bus.emit_json("late", &"late-message").await;
        let msg = rx.recv().await.unwrap();
        assert!(msg.contains("late-message"));
    }

    #[test]
    fn event_stream_path_decodes_frontend_event_names() {
        assert_eq!(
            event_name_from_path("/api/events/omnilauncher%3A%2F%2Fai-done"),
            Some("omnilauncher://ai-done".to_string())
        );
    }

    // ── SelectionPayload tests ─────────────────────────────────────────

    #[test]
    fn selection_payload_serialization_roundtrip() {
        let payload = SelectionPayload {
            token: "tok-123".to_string(),
            selection: "selected text".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: SelectionPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.token, "tok-123");
        assert_eq!(deserialized.selection, "selected text");
    }

    // ── Request type deserialization tests ──────────────────────────────

    #[test]
    fn search_request_deserialize() {
        let json = r#"{"query":"hello world"}"#;
        let req: SearchRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.query, "hello world");
    }

    #[test]
    fn ai_query_request_deserialize() {
        let json = r#"{"query":"what is rust"}"#;
        let req: AiQueryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.query, "what is rust");
    }

    #[test]
    fn session_request_deserialize() {
        let json = r#"{"session_id":42}"#;
        let req: SessionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.session_id, 42);
    }

    #[test]
    fn models_request_deserialize() {
        let json = r#"{"base_url":"http://localhost:11434","api_key":"sk-test"}"#;
        let req: ModelsRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.base_url, "http://localhost:11434");
        assert_eq!(req.api_key, "sk-test");
    }

    #[test]
    fn vision_request_deserialize() {
        let json = r#"{"prompt":"describe this","image_base64":"abc123"}"#;
        let req: VisionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.prompt, "describe this");
        assert_eq!(req.image_base64, "abc123");
    }

    #[test]
    fn save_settings_request_deserialize() {
        let json = r#"{
            "ai_base_url":"http://localhost:11434",
            "ai_model":"gpt-4",
            "ai_api_key":"key",
            "theme":"dark",
            "hotkey":"Alt+Space",
            "max_results":10,
            "background_url":""
        }"#;
        let req: SaveSettingsRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.ai_base_url, "http://localhost:11434");
        assert_eq!(req.ai_model, "gpt-4");
        assert_eq!(req.theme, "dark");
        assert_eq!(req.max_results, 10);
    }
}

/// Vision analyze — AI half. The screenshot is captured locally by the desktop
/// shell (only it has a screen) and handed in as base64; this performs the
/// OpenAI-style chat-completion call with the image and returns the text.
pub async fn vision_analyze_backend(
    prompt: &str,
    image_base64: &str,
    state: &SplitServerState,
) -> Result<String, String> {
    let (base_url, api_key, model) = {
        let settings = state.settings.lock().await;
        (
            settings.ai_base_url.trim_end_matches('/').to_string(),
            settings.resolve_ai_api_key(),
            settings.ai_model.clone(),
        )
    };

    let user_prompt = if prompt.trim().is_empty() {
        "Please describe what you see in this image.".to_string()
    } else {
        prompt.to_string()
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let payload = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": format!("data:image/png;base64,{}", image_base64)
                        }
                    },
                    {
                        "type": "text",
                        "text": user_prompt
                    }
                ]
            }
        ],
        "max_tokens": 1024
    });

    let url = format!("{}/v1/chat/completions", base_url);
    let mut req = client.post(&url).json(&payload);
    if !api_key.is_empty() {
        req = req.bearer_auth(&api_key);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("API request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, text));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("(no response)")
        .to_string())
}

pub async fn search_backend(query: String, state: &SplitServerState) -> Vec<QueryResult> {
    let pm = state.plugin_manager.lock().await;
    pm.query_all(&query).await
}

/// Slash-command preview results. Shared by the split-backend
/// `/api/slash/preview` endpoint and the Tauri `slash_preview` command so both
/// surfaces behave identically. `pm` is passed in (already locked by the
/// caller) to keep this free of `SplitServerState` / `AppState` coupling.
pub async fn slash_preview_backend(query: &str, pm: &crate::PluginManager) -> Vec<QueryResult> {
    let lower = query.to_lowercase();

    // Parse command and argument
    let (cmd, arg) = match query.split_once(' ') {
        Some((c, a)) => (c.to_lowercase(), a.trim().to_string()),
        None => (lower.clone(), String::new()),
    };

    match cmd.as_str() {
        "/app" | "/a" => {
            if arg.is_empty() {
                vec![]
            } else {
                pm.query_all(&arg).await
            }
        }
        "/find" | "/f" => {
            if arg.is_empty() {
                vec![]
            } else {
                pm.query_all(&format!("f {}", arg)).await
            }
        }
        "/open" | "/o" => {
            if arg.is_empty() {
                vec![]
            } else {
                pm.query_all(&arg).await
            }
        }
        "/run" | "/r" => {
            if arg.is_empty() {
                vec![]
            } else {
                pm.query_all(&format!("> {}", arg)).await
            }
        }
        "/grep" | "/g" => vec![],
        "/web" | "/w" => {
            if arg.is_empty() {
                return vec![];
            }
            // Show web search targets as previews
            let encoded = arg.replace(' ', "+");
            vec![
                QueryResult {
                    id: "web-google".to_string(),
                    title: format!("Google: {}", arg),
                    subtitle: Some("Search with Google".to_string()),
                    icon: Some("🔍".to_string()),
                    score: 100,
                    action_type: "url".to_string(),
                    action_data: format!("https://www.google.com/search?q={}", encoded),
                    source: None,
                },
                QueryResult {
                    id: "web-youtube".to_string(),
                    title: format!("YouTube: {}", arg),
                    subtitle: Some("Search on YouTube".to_string()),
                    icon: Some("▶️".to_string()),
                    score: 90,
                    action_type: "url".to_string(),
                    action_data: format!(
                        "https://www.youtube.com/results?search_query={}",
                        encoded
                    ),
                    source: None,
                },
                QueryResult {
                    id: "web-github".to_string(),
                    title: format!("GitHub: {}", arg),
                    subtitle: Some("Search on GitHub".to_string()),
                    icon: Some("🐙".to_string()),
                    score: 80,
                    action_type: "url".to_string(),
                    action_data: format!("https://github.com/search?q={}", encoded),
                    source: None,
                },
            ]
        }
        "/kill" => {
            if arg.is_empty() {
                return vec![];
            }
            // SECURITY: use the in-process `sysinfo` crate — no shell, no string
            // interpolation, no possible injection.
            use sysinfo::System;
            let needle = arg.to_lowercase();
            let mut system = System::new_all();
            system.refresh_all();
            let mut matches: Vec<(u32, String, u64)> = system
                .processes()
                .iter()
                .filter_map(|(pid, proc)| {
                    let name = proc.name().to_string();
                    if name.to_lowercase().contains(&needle) {
                        Some((pid.as_u32(), name, proc.memory()))
                    } else {
                        None
                    }
                })
                .collect();
            matches.sort_by_key(|m| std::cmp::Reverse(m.2));
            matches.truncate(10);

            matches
                .into_iter()
                .enumerate()
                .map(|(i, (pid, name, mem_kb))| {
                    let mem_mb = mem_kb as f64 / 1024.0;
                    QueryResult {
                        id: format!("kill-{}", i),
                        title: format!("{} (PID: {})", name, pid),
                        subtitle: Some(format!("{:.1} MB", mem_mb)),
                        icon: Some("💀".to_string()),
                        score: 100 - i as i32,
                        action_type: "kill_pid".to_string(),
                        action_data: pid.to_string(),
                        source: None,
                    }
                })
                .collect()
        }
        "/clip" | "/cb" => pm.query_all(&format!("cb {}", arg)).await,
        "/calc" | "/c" => pm.query_all(&format!("= {}", arg)).await,
        "/todo" | "/t" => pm.query_all(query).await,
        "/env" => pm.query_all(&format!("env {}", arg)).await,
        "/color" => pm.query_all(&format!("color {}", arg)).await,
        "/sys" => pm.query_all(&format!("sys {}", arg)).await,
        "/ps" => pm.query_all("ps ").await,
        "/ip" => pm.query_all("net ip").await,
        "/ports" => pm.query_all("net ports").await,
        "/net" => pm.query_all(&format!("net {}", arg)).await,
        "/bm" | "/bookmarks" => pm.query_all(&format!("bm {}", arg)).await,
        "/git" => pm.query_all(&format!("git {}", arg)).await,
        "/hosts" => pm.query_all(&format!("hosts {}", arg)).await,
        "/timer" => pm.query_all(&format!("timer {}", arg)).await,
        "/emoji" => pm.query_all(&format!("emoji {}", arg)).await,
        "/cron" => pm.query_all(&format!("cron {}", arg)).await,
        "/pomo" => pm.query_all(&format!("pomo {}", arg)).await,
        "/sched" => {
            let sched_query = if arg.is_empty() {
                "sched".to_string()
            } else {
                format!("sched {}", arg)
            };
            pm.query_all(&sched_query).await
        }
        "/resize" => pm.query_all(&format!("resize {}", arg)).await,
        "/plugins" | "/pm" => vec![QueryResult {
            id: "builtin:plugin-manager".to_string(),
            title: "Manage Plugins".to_string(),
            subtitle: Some("Install, list, and remove external plugins".to_string()),
            icon: Some("🔌".to_string()),
            score: 100,
            action_type: "open_plugin_manager".to_string(),
            action_data: String::new(),
            source: None,
        }],
        _ => vec![],
    }
}

/// Refresh the in-memory PluginManager after an install/update/remove so newly
/// changed external plugins become visible without restarting the backend.
async fn reload_external_plugins_state(state: &SplitServerState) {
    let settings = crate::load_settings();
    let mut pm = state.plugin_manager.lock().await;
    pm.reload_external_plugins(&settings.plugin_dirs);
}

/// Install a plugin runtime dependency (python/node/dotnet), emitting progress
/// over the SSE bus so the desktop UI's `omnilauncher://plugin-runtime-progress`
/// listener updates live — mirroring the Tauri command path in `main.rs`.
async fn install_runtime_dep_backend(id: &str, state: &SplitServerState) -> Result<String, String> {
    use crate::plugins::runtime_deps::{runtime_install_plan, runtime_label};

    let emit = |message: String| {
        let bus = state.event_bus.clone();
        let id = id.to_string();
        async move {
            bus.emit_json(
                "omnilauncher://plugin-runtime-progress",
                &serde_json::json!({ "id": id, "label": runtime_label(&id), "message": message }),
            )
            .await;
        }
    };

    match id {
        "python" => {
            emit("Starting Python runtime install.".to_string()).await;
            let exe = crate::python_installer::install_bundled_python_with_progress(|_message| {
                // Per-line progress is best-effort; the start/end events below
                // are what the UI relies on. We avoid spawning from this sync
                // FnMut callback to keep the borrow checker and runtime simple.
            })
            .await?;
            emit("Python runtime installed.".to_string()).await;
            Ok(format!("Python installed at {}", exe.display()))
        }
        "node" | "dotnet" => {
            let (program, args, display) = runtime_install_plan(id)?;
            emit(format!("Running: {display}")).await;
            let output = tokio::process::Command::new(&program)
                .args(&args)
                .output()
                .await
                .map_err(|e| format!("Failed to run {display}: {e}"))?;
            if output.status.success() {
                emit(format!("{} installer completed.", runtime_label(id))).await;
                Ok(format!("Installed {}", runtime_label(id)))
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let detail = if stderr.is_empty() {
                    format!("installer exited with status {}", output.status)
                } else {
                    stderr
                };
                emit(format!("{} installer failed.", runtime_label(id))).await;
                Err(format!(
                    "Failed to install {}: {}",
                    runtime_label(id),
                    detail
                ))
            }
        }
        _ => Err(format!("Unknown plugin runtime dependency: {id}")),
    }
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
