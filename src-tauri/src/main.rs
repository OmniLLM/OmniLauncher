use omnilauncher_lib::{
    ai::{
        client::AiClient,
        router::{ConversationContext, Router},
    },
    create_plugin_manager,
    live_server::{LiveResponse, LiveServer},
    load_settings, save_settings, AppSettings, QueryResult, SkillInfo, SkillManager,
};
use simplelog::{ColorChoice, ConfigBuilder, LevelFilter, TermLogger, TerminalMode, WriteLogger};
use std::{
    fs::{self, OpenOptions},
    path::PathBuf,
    sync::Arc,
};
use tauri::{
    image::Image,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, LogicalPosition, LogicalSize, Manager, Position, Size,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
use tokio::sync::{Mutex, Semaphore};

fn debug_log_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".omnilauncher")
        .join("omnilauncher.log")
}

fn init_debug_logging(enable_debug: bool) {
    if !enable_debug {
        return;
    }

    let path = debug_log_path();
    if let Some(parent) = path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            eprintln!(
                "Failed to create debug log directory {}: {err}",
                parent.display()
            );
            return;
        }
    }

    let config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .set_thread_level(LevelFilter::Debug)
        .build();

    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => {
            if WriteLogger::init(LevelFilter::Trace, config, file).is_err() {
                eprintln!("Failed to initialize debug logger at {}", path.display());
            } else {
                log::info!("Debug logging enabled at {}", path.display());
            }
        }
        Err(err) => {
            eprintln!("Failed to open debug log file {}: {err}", path.display());
            let _ = TermLogger::init(
                LevelFilter::Trace,
                ConfigBuilder::new().set_time_format_rfc3339().build(),
                TerminalMode::Stderr,
                ColorChoice::Never,
            );
        }
    }
}

pub struct AppState {
    pub plugin_manager: Arc<Mutex<omnilauncher_lib::PluginManager>>,
    pub ai_client: Mutex<AiClient>,
    pub settings: Arc<Mutex<AppSettings>>,
    pub conversation: Arc<Mutex<ConversationContext>>,
    pub ai_in_flight: Semaphore,
    pub skill_manager: Arc<Mutex<SkillManager>>,
    pub live_server: LiveServer,
    pub live_server_port: u16,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    log::info!("Starting OmniLauncher runtime");
    let settings = load_settings();
    log::debug!(
        "Loaded settings (base_url={}, model={}, max_results={})",
        settings.ai_base_url,
        settings.ai_model,
        settings.max_results
    );

    let ai_client = AiClient::new(
        settings.ai_base_url.clone(),
        settings.ai_api_key.clone(),
        settings.ai_model.clone(),
    );

    let mut skill_manager = SkillManager::new();
    skill_manager.load_all();
    log::debug!("Loaded skill manager");

    let live_server_port = 1421;
    let live_server = LiveServer::new();
    let live_server_task = live_server.clone();

    tauri::async_runtime::spawn(async move {
        log::info!(
            "registering live server routes on port {}",
            live_server_port
        );
        live_server_task
            .register_route("/todo", || {
                LiveResponse::html(omnilauncher_lib::plugins::todo::todo_live_html())
            })
            .await;
        live_server_task
            .register_route("/todo/data", || {
                LiveResponse::json(omnilauncher_lib::plugins::todo::todo_live_data_json())
            })
            .await;
        log::info!(
            "starting live server on http://127.0.0.1:{}",
            live_server_port
        );
        live_server_task.serve(live_server_port).await;
    });

    let state = AppState {
        plugin_manager: Arc::new(Mutex::new(create_plugin_manager())),
        ai_client: Mutex::new(ai_client),
        settings: Arc::new(Mutex::new(settings)),
        conversation: Arc::new(Mutex::new(ConversationContext::default())),
        ai_in_flight: Semaphore::new(1),
        skill_manager: Arc::new(Mutex::new(skill_manager)),
        live_server,
        live_server_port,
    };

    let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyO);

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(state)
        .setup(move |app| {
            log::debug!("Running Tauri setup");

            // Start background scheduler (must be inside setup — tokio runtime is live here)
            omnilauncher_lib::plugins::scheduler::start_scheduler();

            let window = app.get_webview_window("main").unwrap();

            // Center the initial window before the frontend performs its first resize.
            let _ = window.center();

            // ── System tray icon ──────────────────────────────────────────
            let icon = Image::from_path(
                app.path()
                    .resource_dir()
                    .unwrap_or_default()
                    .join("icons/32x32.png"),
            )
            .or_else(|_| {
                // Fallback: load from the src-tauri/icons directory during dev
                Image::from_path("icons/32x32.png")
            })
            .ok();

            let mut tray_builder =
                TrayIconBuilder::new().tooltip("OmniLauncher — Ctrl+Shift+O to toggle");

            if let Some(img) = icon {
                let _ = window.set_icon(img.clone());
                tray_builder = tray_builder.icon(img);
            }

            let tray_window = window.clone();
            tray_builder
                .on_tray_icon_event(move |_tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if tray_window.is_visible().unwrap_or(false) {
                            let _ = tray_window.hide();
                        } else {
                            let _ = tray_window.show();
                            let _ = tray_window.set_focus();
                            let _ = tray_window.emit("omnilauncher://shown", ());
                        }
                    }
                })
                .build(app)?;

            let global_shortcut = app.global_shortcut();

            global_shortcut
                .on_shortcut(shortcut, move |_app, _shortcut, event| {
                    if let tauri_plugin_global_shortcut::ShortcutState::Pressed = event.state() {
                        log::trace!("Global shortcut pressed; toggling main window visibility");
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                        } else {
                            // Read X11 PRIMARY selection before showing the window.
                            // This captures whatever text the user had highlighted in
                            // another app at the moment they triggered the hotkey.
                            let selection =
                                omnilauncher_lib::plugins::selection::read_x11_selection()
                                    .unwrap_or_default();
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = window.emit("omnilauncher://shown", selection);
                        }
                    }
                })
                .unwrap_or_else(|err| {
                    log::warn!("Failed to register global shortcut: {err}");
                    eprintln!("Failed to register global shortcut: {err}");
                });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            search,
            ai_query,
            execute_result,
            slash_preview,
            get_settings,
            save_settings_cmd,
            clear_conversation,
            execute_slash_command,
            list_models,
            list_skills,
            reload_skills,
            install_skill,
            delete_skill,
            set_window_geometry,
            install_plugin,
            update_plugin,
            update_plugin_collection,
            list_plugins,
            remove_plugin,
            vision_analyze,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
async fn set_window_geometry(
    window: tauri::WebviewWindow,
    height: f64,
    ai_mode: bool,
) -> Result<bool, String> {
    sync_window_geometry(&window, height, ai_mode).await
}

async fn sync_window_geometry(
    window: &tauri::WebviewWindow,
    height: f64,
    ai_mode: bool,
) -> Result<bool, String> {
    let clamped_height = height.clamp(56.0, 640.0);
    log::trace!("sync_window_geometry requested height={height}, clamped={clamped_height}");

    if let Some(monitor) = window.current_monitor().map_err(|e| e.to_string())? {
        let scale_factor = monitor.scale_factor();
        let monitor_size = monitor.size();
        let monitor_position = monitor.position();
        let monitor_width = monitor_size.width as f64 / scale_factor;
        let monitor_height = monitor_size.height as f64 / scale_factor;
        let monitor_x = monitor_position.x as f64 / scale_factor;
        let monitor_y = monitor_position.y as f64 / scale_factor;

        let window_width = if ai_mode {
            monitor_width * 0.4
        } else {
            monitor_width / 3.0
        };
        let window_x = monitor_x + (monitor_width - window_width) / 2.0;
        let window_y = monitor_y + (monitor_height - clamped_height) / 2.0;
        log::debug!(
            "Applying centered geometry width={window_width:.2}, height={clamped_height:.2}, x={window_x:.2}, y={window_y:.2}"
        );

        window
            .set_size(Size::Logical(LogicalSize::new(
                window_width,
                clamped_height,
            )))
            .map_err(|e| e.to_string())?;
        window
            .set_position(Position::Logical(LogicalPosition::new(window_x, window_y)))
            .map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        let fallback_width = if ai_mode { 768.0 } else { 680.0 };
        log::debug!(
            "No monitor info available; applying fallback geometry width={fallback_width:.2} height={clamped_height:.2}"
        );
        window
            .set_size(Size::Logical(LogicalSize::new(
                fallback_width,
                clamped_height,
            )))
            .map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[tauri::command]
async fn search(
    query: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<QueryResult>, String> {
    log::trace!("search invoked with query={query}");
    let pm = state.plugin_manager.lock().await;
    Ok(pm.query_all(&query).await)
}

async fn try_start_ai_request(
    state: &AppState,
) -> Result<tokio::sync::SemaphorePermit<'_>, String> {
    state
        .ai_in_flight
        .try_acquire()
        .map_err(|_| "AI response is still in progress".to_string())
}

#[tauri::command]
async fn ai_query(
    query: String,
    state: tauri::State<'_, AppState>,
) -> Result<omnilauncher_lib::AiResponse, String> {
    let _ai_request = try_start_ai_request(&state).await?;
    log::debug!("ai_query invoked with {} characters", query.len());
    // Add to conversation context
    {
        let mut ctx = state.conversation.lock().await;
        ctx.add_user(&query);
    }

    let pm = state.plugin_manager.lock().await;
    let client = state.ai_client.lock().await;
    let ctx = state.conversation.lock().await;
    let skill_mgr = state.skill_manager.lock().await;
    // Always use AI route in chat mode (bypass NL detection)
    let response = Router::ai_route(&query, &pm, &client, &ctx, &skill_mgr).await;
    drop(ctx);
    drop(skill_mgr);

    // Add assistant response to context
    {
        let mut ctx = state.conversation.lock().await;
        ctx.add_assistant(&response.content);
    }

    Ok(response)
}

#[tauri::command]
async fn clear_conversation(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    log::debug!("clear_conversation invoked");
    let mut ctx = state.conversation.lock().await;
    ctx.clear();
    Ok(true)
}

#[tauri::command]
async fn execute_slash_command(
    query: String,
    state: tauri::State<'_, AppState>,
) -> Result<omnilauncher_lib::AiResponse, String> {
    log::debug!("execute_slash_command invoked with query={query}");
    let pm = state.plugin_manager.lock().await;
    let mut skill_mgr = state.skill_manager.lock().await;
    let response = Router::slash_command(&query, &pm, &mut skill_mgr).await;
    Ok(response)
}

#[tauri::command]
async fn slash_preview(
    query: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<QueryResult>, String> {
    log::trace!("slash_preview invoked with query={query}");
    let pm = state.plugin_manager.lock().await;
    let lower = query.to_lowercase();

    // Parse command and argument
    let (cmd, arg) = match query.split_once(' ') {
        Some((c, a)) => (c.to_lowercase(), a.trim().to_string()),
        None => (lower.clone(), String::new()),
    };

    match cmd.as_str() {
        "/app" | "/a" => {
            if arg.is_empty() {
                Ok(vec![])
            } else {
                Ok(pm.query_all(&arg).await)
            }
        }
        "/find" | "/f" => {
            if arg.is_empty() {
                Ok(vec![])
            } else {
                Ok(pm.query_all(&format!("f {}", arg)).await)
            }
        }
        "/open" | "/o" => {
            if arg.is_empty() {
                Ok(vec![])
            } else {
                Ok(pm.query_all(&arg).await)
            }
        }
        "/run" | "/r" => {
            if arg.is_empty() {
                Ok(vec![])
            } else {
                Ok(pm.query_all(&format!("> {}", arg)).await)
            }
        }
        "/grep" | "/g" => Ok(vec![]),
        "/web" | "/w" => {
            if arg.is_empty() {
                return Ok(vec![]);
            }
            // Show web search targets as previews
            let encoded = arg.replace(' ', "+");
            Ok(vec![
                QueryResult {
                    id: "web-google".to_string(),
                    title: format!("Google: {}", arg),
                    subtitle: Some("Search with Google".to_string()),
                    icon: Some("🔍".to_string()),
                    score: 100,
                    action_type: "url".to_string(),
                    action_data: format!("https://www.google.com/search?q={}", encoded),
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
                },
                QueryResult {
                    id: "web-github".to_string(),
                    title: format!("GitHub: {}", arg),
                    subtitle: Some("Search on GitHub".to_string()),
                    icon: Some("🐙".to_string()),
                    score: 80,
                    action_type: "url".to_string(),
                    action_data: format!("https://github.com/search?q={}", encoded),
                },
            ])
        }
        "/kill" => {
            if arg.is_empty() {
                return Ok(vec![]);
            }
            // Show matching processes as previews
            let output = if cfg!(target_os = "windows") {
                std::process::Command::new("powershell")
                    .args(["-NoProfile", "-Command",
                        &format!("Get-Process | Where-Object {{ $_.Name -like '*{}*' }} | Select-Object -First 10 Id, Name, @{{N='MemMB';E={{[math]::Round($_.WorkingSet64/1MB,1)}}}} | ForEach-Object {{ \"$($_.Id)|$($_.Name)|$($_.MemMB)\" }}", arg)])
                    .output()
            } else {
                std::process::Command::new("sh")
                    .args([
                        "-c",
                        &format!("ps aux | grep -i '{}' | grep -v grep | head -10", arg),
                    ])
                    .output()
            };
            match output {
                Ok(out) => {
                    let text = String::from_utf8_lossy(&out.stdout);
                    let results: Vec<QueryResult> = text
                        .lines()
                        .filter(|l| !l.is_empty())
                        .enumerate()
                        .filter_map(|(i, line)| {
                            let parts: Vec<&str> = line.split('|').collect();
                            if parts.len() >= 2 {
                                Some(QueryResult {
                                    id: format!("kill-{}", i),
                                    title: format!("{} (PID: {})", parts[1], parts[0]),
                                    subtitle: parts.get(2).map(|m| format!("{} MB", m)),
                                    icon: Some("💀".to_string()),
                                    score: 100 - i as i32,
                                    action_type: "shell".to_string(),
                                    action_data: if cfg!(target_os = "windows") {
                                        format!("taskkill /F /PID {}", parts[0])
                                    } else {
                                        format!("kill -9 {}", parts[0])
                                    },
                                })
                            } else {
                                None
                            }
                        })
                        .collect();
                    Ok(results)
                }
                Err(_) => Ok(vec![]),
            }
        }
        "/clip" | "/cb" => {
            let results = pm.query_all(&format!("cb {}", arg)).await;
            Ok(results)
        }
        "/calc" | "/c" => {
            let results = pm.query_all(&format!("= {}", arg)).await;
            Ok(results)
        }
        "/todo" | "/t" => Ok(pm.query_all(&query).await),
        "/env" => Ok(pm.query_all(&format!("env {}", arg)).await),
        "/color" => Ok(pm.query_all(&format!("color {}", arg)).await),
        "/sys" => Ok(pm.query_all(&format!("sys {}", arg)).await),
        "/ps" => Ok(pm.query_all("ps ").await),
        "/ip" => Ok(pm.query_all("net ip").await),
        "/ports" => Ok(pm.query_all("net ports").await),
        "/net" => Ok(pm.query_all(&format!("net {}", arg)).await),
        "/bm" | "/bookmarks" => Ok(pm.query_all(&format!("bm {}", arg)).await),
        "/git" => Ok(pm.query_all(&format!("git {}", arg)).await),
        "/hosts" => Ok(pm.query_all(&format!("hosts {}", arg)).await),
        "/timer" => Ok(pm.query_all(&format!("timer {}", arg)).await),
        "/emoji" => Ok(pm.query_all(&format!("emoji {}", arg)).await),
        "/cron" => Ok(pm.query_all(&format!("cron {}", arg)).await),
        "/pomo" => Ok(pm.query_all(&format!("pomo {}", arg)).await),
        "/sched" => {
            let sched_query = if arg.is_empty() {
                "sched".to_string()
            } else {
                format!("sched {}", arg)
            };
            Ok(pm.query_all(&sched_query).await)
        }
        "/resize" => Ok(pm.query_all(&format!("resize {}", arg)).await),
        "/plugins" | "/pm" => Ok(vec![QueryResult {
            id: "builtin:plugin-manager".to_string(),
            title: "Manage Plugins".to_string(),
            subtitle: Some("Install, list, and remove external plugins".to_string()),
            icon: Some("🔌".to_string()),
            score: 100,
            action_type: "open_plugin_manager".to_string(),
            action_data: String::new(),
        }]),
        _ => Ok(vec![]),
    }
}

fn spawn_external_command(program: &str, args: &[&str], description: &str) -> bool {
    log::debug!("spawning external command for {description}: {program} {args:?}");
    match std::process::Command::new(program).args(args).spawn() {
        Ok(child) => {
            log::info!(
                "spawned external command for {description}: pid={}",
                child.id()
            );
            true
        }
        Err(err) => {
            log::error!(
                "failed to spawn external command for {description}: {program} {args:?}: {err}"
            );
            false
        }
    }
}

#[tauri::command]
async fn execute_result(
    result: QueryResult,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    log::debug!(
        "execute_result invoked action_type={} id={} title={}",
        result.action_type,
        result.id,
        result.title
    );
    let action_data = if result.id == "todo:view" {
        state.live_server.url(state.live_server_port, "/todo")
    } else {
        result.action_data.clone()
    };
    if action_data != result.action_data {
        log::debug!(
            "resolved action_data for id={}: {} -> {}",
            result.id,
            result.action_data,
            action_data
        );
    }

    let success = match result.action_type.as_str() {
        "url" | "open_url" => {
            log::info!("opening url: {}", action_data);
            #[cfg(target_os = "linux")]
            {
                spawn_external_command("xdg-open", &[&action_data], "open url")
            }
            #[cfg(target_os = "macos")]
            {
                spawn_external_command("open", &[&action_data], "open url")
            }
            #[cfg(target_os = "windows")]
            {
                spawn_external_command("cmd", &["/C", "start", "", &action_data], "open url")
            }
        }
        "shell" | "open_app" => {
            #[cfg(target_os = "windows")]
            {
                spawn_external_command("cmd", &["/C", "start", "", &result.action_data], "open app")
            }
            #[cfg(target_os = "macos")]
            {
                spawn_external_command("open", &[&result.action_data], "open app")
            }
            #[cfg(target_os = "linux")]
            {
                spawn_external_command("sh", &["-c", &result.action_data], "open app")
            }
        }
        "open" => {
            #[cfg(target_os = "linux")]
            {
                spawn_external_command("xdg-open", &[&result.action_data], "open path")
            }
            #[cfg(target_os = "macos")]
            {
                spawn_external_command("open", &[&result.action_data], "open path")
            }
            #[cfg(target_os = "windows")]
            {
                spawn_external_command("explorer", &[&result.action_data], "open path")
            }
        }
        "copy" => {
            // Just a copy action — frontend handles clipboard
            true
        }
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
        "sched_add" => {
            // action_data format: "label|||schedule|||command"
            let parts: Vec<&str> = result.action_data.splitn(3, "|||").collect();
            if parts.len() == 3 {
                let label = parts[0];
                let sched_str = parts[1];
                let cmd = parts[2];
                if let Some(sched) =
                    omnilauncher_lib::plugins::scheduler::Schedule::from_stored(sched_str)
                {
                    omnilauncher_lib::plugins::scheduler::add_job(label, &sched, cmd)
                        .map(|_| true)
                        .unwrap_or(false)
                } else {
                    false
                }
            } else {
                false
            }
        }
        "sched_del" => {
            if let Ok(id) = result.action_data.parse::<i64>() {
                omnilauncher_lib::plugins::scheduler::delete_job(id)
            } else {
                false
            }
        }
        "sched_on" => {
            if let Ok(id) = result.action_data.parse::<i64>() {
                omnilauncher_lib::plugins::scheduler::toggle_job(id, true)
            } else {
                false
            }
        }
        "sched_off" => {
            if let Ok(id) = result.action_data.parse::<i64>() {
                omnilauncher_lib::plugins::scheduler::toggle_job(id, false)
            } else {
                false
            }
        }
        _ => false,
    };
    Ok(success)
}

#[tauri::command]
async fn list_models(base_url: String, api_key: String) -> Result<Vec<String>, String> {
    log::debug!(
        "list_models invoked for base_url={} (api_key_present={})",
        base_url,
        !api_key.is_empty()
    );
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
    let models = json["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok(models)
}

#[tauri::command]
async fn get_settings(state: tauri::State<'_, AppState>) -> Result<AppSettings, String> {
    log::trace!("get_settings invoked");
    let settings = state.settings.lock().await;
    Ok(settings.clone())
}

#[tauri::command]
async fn save_settings_cmd(
    settings: AppSettings,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    log::debug!(
        "save_settings_cmd invoked (base_url={}, model={}, max_results={})",
        settings.ai_base_url,
        settings.ai_model,
        settings.max_results
    );
    let mut current = state.settings.lock().await;
    *current = settings.clone();
    // Recreate AiClient with new settings
    let mut client = state.ai_client.lock().await;
    *client = AiClient::new(
        settings.ai_base_url.clone(),
        settings.ai_api_key.clone(),
        settings.ai_model.clone(),
    );
    Ok(save_settings(&settings))
}

// ─── Skill commands ───────────────────────────────────────────────────────────

#[tauri::command]
async fn list_skills(state: tauri::State<'_, AppState>) -> Result<Vec<SkillInfo>, String> {
    log::trace!("list_skills invoked");
    let mgr = state.skill_manager.lock().await;
    Ok(mgr.list_meta().into_iter().map(SkillInfo::from).collect())
}

#[tauri::command]
async fn reload_skills(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    log::debug!("reload_skills invoked");
    let mut mgr = state.skill_manager.lock().await;
    mgr.reload();
    Ok(true)
}

#[tauri::command]
async fn install_skill(
    source: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    log::debug!("install_skill invoked with source={source}");
    let mut mgr = state.skill_manager.lock().await;
    if source.starts_with("http://") || source.starts_with("https://") {
        mgr.install_from_url(&source)
    } else {
        mgr.install_from_path(&source)
    }
}

#[tauri::command]
async fn delete_skill(name: String, state: tauri::State<'_, AppState>) -> Result<String, String> {
    log::debug!("delete_skill invoked with name={name}");
    let mut mgr = state.skill_manager.lock().await;
    mgr.delete_skill(&name)
}

// ─── External plugin management commands ──────────────────────────────────────

#[tauri::command]
async fn install_plugin(source: String, target_dir: Option<String>) -> Result<String, String> {
    log::debug!("install_plugin invoked with source={source} target_dir={target_dir:?}");
    omnilauncher_lib::plugins::plugin_manager_cmd::install_plugin(source, target_dir).await
}

#[tauri::command]
async fn update_plugin(name: String) -> Result<String, String> {
    log::debug!("update_plugin invoked with name={name}");
    omnilauncher_lib::plugins::plugin_manager_cmd::update_plugin(name).await
}

#[tauri::command]
async fn update_plugin_collection(
    source: String,
    plugin_dirs: Vec<String>,
) -> Result<String, String> {
    log::debug!(
        "update_plugin_collection invoked with source={source} plugin_dirs={plugin_dirs:?}"
    );
    omnilauncher_lib::plugins::plugin_manager_cmd::update_plugin_collection(source, plugin_dirs)
        .await
}

#[tauri::command]
fn list_plugins() -> Vec<serde_json::Value> {
    log::trace!("list_plugins invoked");
    omnilauncher_lib::plugins::plugin_manager_cmd::list_plugins()
}

#[tauri::command]
async fn remove_plugin(name: String) -> Result<(), String> {
    log::debug!("remove_plugin invoked with name={name}");
    omnilauncher_lib::plugins::plugin_manager_cmd::remove_plugin(name).await
}

/// Vision analyze command:
/// 1. Hides the launcher window
/// 2. Runs `scrot -s` (interactive region select) to save a screenshot
/// 3. Base64-encodes the image
/// 4. Calls the configured vision model via OpenAI chat completions with image_url
/// 5. Returns the AI response as a plain string
#[tauri::command]
async fn vision_analyze(
    prompt: String,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    use std::io::Read;
    log::debug!("vision_analyze invoked, prompt={:?}", prompt);

    // Hide the launcher so it doesn't appear in the screenshot
    let _ = window.hide();
    // Brief pause to let the window disappear before the user selects a region
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    // Generate a temp file path for the screenshot
    let tmp_path = std::env::temp_dir().join("omnilauncher_vision.png");
    let tmp_str = tmp_path.to_string_lossy().to_string();

    // Run scrot with interactive selection (-s flag)
    let output = tokio::process::Command::new("scrot")
        .args(["-s", "--overwrite", &tmp_str])
        .output()
        .await
        .map_err(|e| format!("scrot failed: {e}. Is scrot installed?"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("scrot exited with error: {}", stderr));
    }

    // Read and base64-encode the screenshot
    let mut file =
        std::fs::File::open(&tmp_path).map_err(|e| format!("Failed to open screenshot: {e}"))?;
    let mut img_bytes = Vec::new();
    file.read_to_end(&mut img_bytes)
        .map_err(|e| format!("Failed to read screenshot: {e}"))?;

    use std::io::Write;
    let mut enc =
        base64::write::EncoderStringWriter::new(&base64::engine::general_purpose::STANDARD);
    enc.write_all(&img_bytes)
        .map_err(|e| format!("Base64 encode error: {e}"))?;
    let b64 = enc.into_inner();

    // Clean up temp file (best-effort)
    let _ = std::fs::remove_file(&tmp_path);

    // Build the vision prompt
    let user_prompt = if prompt.trim().is_empty() {
        "Please describe what you see in this image.".to_string()
    } else {
        prompt.clone()
    };

    // Call the AI API with the image
    let settings = state.settings.lock().await;
    let base_url = settings.ai_base_url.trim_end_matches('/').to_string();
    let api_key = settings.ai_api_key.clone();
    let model = settings.ai_model.clone();
    drop(settings);

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
                            "url": format!("data:image/png;base64,{}", b64)
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
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("(no response)")
        .to_string();

    // Show the window again with results
    let _ = window.show();
    let _ = window.set_focus();

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_external_command_reports_missing_command_failure() {
        assert!(!super::spawn_external_command(
            "omnilauncher-command-that-does-not-exist",
            &[],
            "test missing command",
        ));
    }

    #[tokio::test]
    async fn rejects_second_ai_request_while_one_is_in_progress() {
        let state = AppState {
            plugin_manager: Arc::new(Mutex::new(create_plugin_manager())),
            ai_client: Mutex::new(AiClient::new(String::new(), String::new(), String::new())),
            settings: Arc::new(Mutex::new(AppSettings::default())),
            conversation: Arc::new(Mutex::new(ConversationContext::default())),
            ai_in_flight: Semaphore::new(1),
            skill_manager: Arc::new(Mutex::new(SkillManager::new())),
            live_server: LiveServer::new(),
            live_server_port: 0,
        };

        let first = try_start_ai_request(&state)
            .await
            .expect("first request starts");
        let second = try_start_ai_request(&state).await;

        assert_eq!(second.unwrap_err(), "AI response is still in progress");
        drop(first);
        assert!(try_start_ai_request(&state).await.is_ok());
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let debug_enabled = args.iter().any(|arg| arg == "--debug");
    init_debug_logging(debug_enabled);

    if debug_enabled {
        log::info!("Running with --debug");
        log::debug!("CLI args: {:?}", args);
    } else if TermLogger::init(
        LevelFilter::Info,
        ConfigBuilder::new().build(),
        TerminalMode::Stderr,
        ColorChoice::Never,
    )
    .is_ok()
    {
        log::info!("Running without debug file logging");
    }

    run();
}
