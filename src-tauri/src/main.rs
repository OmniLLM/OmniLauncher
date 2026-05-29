use omnilauncher_lib::{
    ai::{
        client::AiClient,
        router::{ConversationContext, Router},
    },
    create_plugin_manager,
    live_server::{LiveResponse, LiveServer},
    load_settings, save_settings, AppSettings, QueryResult, SkillInfo, SkillManager,
};
mod python_installer;
use python_installer::{check_bundled_python, install_python_command};
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

fn window_pos_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let mut p = std::path::PathBuf::from(home);
    p.push(".config");
    p.push("omnilauncher");
    let _ = std::fs::create_dir_all(&p);
    p.push("window-pos.json");
    p
}

/// Returns true when the OS foreground window appears to belong to
/// OmniLauncher itself. Used to suppress selection capture so highlighted
/// text inside our own dashboard / settings windows doesn't bleed back into
/// the launcher input on the next hotkey press.
#[cfg(target_os = "windows")]
fn foreground_is_ours() -> bool {
    // Single PowerShell call: print "<fg_pid> <parent_pid>" so we can decide
    // ownership without spawning two separate processes per hotkey press.
    let script = r#"
Add-Type -Namespace W -Name U -MemberDefinition '
[DllImport("user32.dll")] public static extern System.IntPtr GetForegroundWindow();
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint procId);
' | Out-Null
$h = [W.U]::GetForegroundWindow()
$pid_ = 0
[void][W.U]::GetWindowThreadProcessId($h, [ref]$pid_)
$ppid = 0
try {
  $ppid = (Get-CimInstance Win32_Process -Filter "ProcessId=$pid_" -ErrorAction Stop).ParentProcessId
} catch {}
"$pid_ $ppid"
"#;
    let out = match std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
    {
        Ok(o) => o,
        Err(_) => return false,
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut parts = stdout.trim().split_whitespace();
    let fg_pid: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let parent_pid: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let our_pid = std::process::id();
    fg_pid == our_pid || parent_pid == our_pid
}

#[cfg(not(target_os = "windows"))]
fn foreground_is_ours() -> bool {
    // On Linux/macOS we don't currently inspect the foreground window owner;
    // err on the side of allowing capture (the setting still gates it).
    false
}

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
    pub ai_client: Arc<Mutex<AiClient>>,
    pub settings: Arc<Mutex<AppSettings>>,
    pub conversation: Arc<Mutex<ConversationContext>>,
    pub ai_in_flight: Arc<Semaphore>,
    /// Handle to the currently running AI agent task, if any.
    /// Used by `ai_cancel` to abort an in-flight request.
    pub current_ai_task: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
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
            .register_route("/dashboard", || async {
                LiveResponse::html(omnilauncher_lib::dashboard::index_html())
            })
            .await;
        live_server_task
            .register_route("/dashboard/data", || async {
                LiveResponse::json(omnilauncher_lib::dashboard::index_data_json())
            })
            .await;
        live_server_task
            .register_route("/dashboard/todos", || async {
                LiveResponse::html(omnilauncher_lib::dashboard::todos_html())
            })
            .await;
        live_server_task
            .register_route("/dashboard/todos/data", || async {
                LiveResponse::json(omnilauncher_lib::dashboard::todos_data_json())
            })
            .await;
        live_server_task
            .register_route("/dashboard/conversation", || async {
                LiveResponse::html(omnilauncher_lib::dashboard::conversation_html())
            })
            .await;
        live_server_task
            .register_route("/dashboard/conversation/data", || async {
                LiveResponse::json(omnilauncher_lib::dashboard::conversation_data_json())
            })
            .await;
        live_server_task
            .register_route("/dashboard/jobs", || async {
                LiveResponse::html(omnilauncher_lib::dashboard::jobs_html())
            })
            .await;
        live_server_task
            .register_route("/dashboard/jobs/data", || async {
                LiveResponse::json(omnilauncher_lib::dashboard::jobs_data_json())
            })
            .await;
        live_server_task
            .register_route("/dashboard/tables", || async {
                LiveResponse::html(omnilauncher_lib::dashboard::tables_html())
            })
            .await;
        live_server_task
            .register_route("/dashboard/tables/data", || async {
                LiveResponse::json(omnilauncher_lib::dashboard::tables_data_json())
            })
            .await;
        live_server_task
            .register_route("/dashboard/github", || async {
                LiveResponse::html(omnilauncher_lib::dashboard::github_html())
            })
            .await;
        live_server_task
            .register_route("/dashboard/github/data", || async {
                LiveResponse::json(omnilauncher_lib::dashboard::github_data_json().await)
            })
            .await;
        live_server_task
            .register_route_with_query("/dashboard/github/repo", |q| async move {
                LiveResponse::json(omnilauncher_lib::dashboard::github_repo_detail_json(q).await)
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
        ai_client: Arc::new(Mutex::new(ai_client)),
        settings: Arc::new(Mutex::new(settings)),
        conversation: Arc::new(Mutex::new({
            let mut ctx = ConversationContext::default();
            // Re-hydrate from SQLite so follow-up questions survive restarts.
            let sid = omnilauncher_lib::db::conversation::current_session_id();
            ctx.session_id = sid;
            ctx.messages = omnilauncher_lib::db::conversation::load_recent_for_session(sid, 20);
            ctx
        })),
        ai_in_flight: Arc::new(Semaphore::new(1)),
        current_ai_task: Arc::new(Mutex::new(None)),
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
            omnilauncher_lib::plugins::scheduler::migrate_inline_commands_to_files();
            omnilauncher_lib::plugins::scheduler::start_scheduler();

            let window = app.get_webview_window("main").unwrap();

            // Center the initial window before the frontend performs its first resize.
            let _ = window.center();

            // Restore saved window position
            let pos_path = window_pos_path();
            if let Ok(data) = std::fs::read_to_string(&pos_path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&data) {
                    if let (Some(x), Some(y)) = (val["x"].as_i64(), val["y"].as_i64()) {
                        let _ = window.set_position(tauri::Position::Physical(
                            tauri::PhysicalPosition {
                                x: x as i32,
                                y: y as i32,
                            },
                        ));
                    }
                }
            }

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
                            // Only capture the foreground selection when the
                            // user has opted in via settings, AND the
                            // foreground window isn't already one of ours
                            // (avoids text inside our dashboard bleeding into
                            // the launcher input on the next hotkey).
                            let cfg = omnilauncher_lib::settings::load_settings();
                            let selection =
                                if cfg.capture_selection_on_open && !foreground_is_ours() {
                                    omnilauncher_lib::plugins::selection::read_x11_selection()
                                        .unwrap_or_default()
                                } else {
                                    String::new()
                                };
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
            ai_cancel,
            execute_result,
            slash_preview,
            get_settings,
            save_settings_cmd,
            clear_conversation,
            list_ai_sessions,
            current_ai_session,
            switch_ai_session,
            delete_ai_session,
            execute_slash_command,
            list_models,
            list_skills,
            reload_skills,
            install_skill,
            delete_skill,
            update_skill,
            set_window_geometry,
            install_plugin,
            update_plugin,
            update_plugin_collection,
            list_plugins,
            remove_plugin,
            vision_analyze,
            save_window_position,
            install_python_command,
            check_bundled_python,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
async fn set_window_geometry(
    window: tauri::WebviewWindow,
    height: f64,
    ai_mode: bool,
    panel_mode: Option<bool>,
) -> Result<bool, String> {
    sync_window_geometry(&window, height, ai_mode, panel_mode.unwrap_or(false)).await
}

#[tauri::command]
async fn save_window_position(x: i32, y: i32) -> Result<(), String> {
    let path = window_pos_path();
    let json = format!("{{\"x\":{},\"y\":{}}}", x, y);
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

async fn sync_window_geometry(
    window: &tauri::WebviewWindow,
    height: f64,
    ai_mode: bool,
    panel_mode: bool,
) -> Result<bool, String> {
    let clamped_height = height.clamp(56.0, 1200.0);
    log::trace!("sync_window_geometry requested height={height}, clamped={clamped_height}");

    if let Some(monitor) = window.current_monitor().map_err(|e| e.to_string())? {
        let scale_factor = monitor.scale_factor();
        let monitor_size = monitor.size();
        let monitor_position = monitor.position();
        let monitor_width = monitor_size.width as f64 / scale_factor;
        let monitor_height = monitor_size.height as f64 / scale_factor;
        let monitor_x = monitor_position.x as f64 / scale_factor;
        let monitor_y = monitor_position.y as f64 / scale_factor;

        let window_width = if panel_mode || ai_mode {
            monitor_width * 0.5
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
        let fallback_width = if ai_mode { 768.0 } else { 640.0 };
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

#[tauri::command]
async fn ai_query(
    query: String,
    state: tauri::State<'_, AppState>,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    // Try to acquire permit (fail fast if AI already in flight)
    let permit = state
        .ai_in_flight
        .clone()
        .try_acquire_owned()
        .map_err(|_| "AI response is still in progress".to_string())?;

    log::debug!("ai_query invoked with {} characters", query.len());

    // Add user message to conversation context
    let session_id = {
        let mut ctx = state.conversation.lock().await;
        if ctx.session_id == 0 {
            ctx.session_id = omnilauncher_lib::db::conversation::current_session_id();
        }
        ctx.add_user(&query);
        ctx.session_id
    };
    omnilauncher_lib::db::conversation::save_turn(session_id, "user", &query);

    // Clone Arcs for the spawned task
    let pm = state.plugin_manager.clone();
    let ai_client = state.ai_client.clone();
    let conversation = state.conversation.clone();
    let skill_mgr = state.skill_manager.clone();

    let handle = tauri::async_runtime::spawn(async move {
        // Keep permit alive for duration of task
        let _permit = permit;

        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        // Spawn a task to forward tool-call events to the window
        let win_for_progress = window.clone();
        tauri::async_runtime::spawn(async move {
            let mut iteration = 0u32;
            while let Some(tool_name) = progress_rx.recv().await {
                iteration += 1;
                let _ = win_for_progress.emit(
                    "omnilauncher://ai-tool-call",
                    serde_json::json!({ "tool": tool_name, "iteration": iteration }),
                );
            }
        });

        // Run the agent loop and catch panics so the frontend always gets a
        // terminal event (ai-done or ai-error) instead of spinning forever.
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
                &mut *skill_lock,
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
                log::error!("ai_query task panicked: {msg}");
                let _ = window.emit("omnilauncher://ai-error", msg);
                return;
            }
        };

        // Add assistant response to context
        let sid = {
            let mut ctx = conversation.lock().await;
            ctx.add_assistant(&response.content);
            ctx.session_id
        };
        omnilauncher_lib::db::conversation::save_turn(sid, "assistant", &response.content);

        let _ = window.emit("omnilauncher://ai-done", &response);
    });

    // Track the handle so `ai_cancel` can abort an in-flight task.
    // Calling `.abort()` on a task that has already finished is a no-op,
    // so we can leave a stale handle until the next request overwrites it.
    {
        let mut slot = state.current_ai_task.lock().await;
        *slot = Some(handle);
    }

    Ok(())
}

#[tauri::command]
async fn ai_cancel(
    state: tauri::State<'_, AppState>,
    window: tauri::WebviewWindow,
) -> Result<bool, String> {
    log::debug!("ai_cancel invoked");
    let mut slot = state.current_ai_task.lock().await;
    if let Some(handle) = slot.take() {
        handle.abort();
        let _ = window.emit("omnilauncher://ai-error", "Cancelled by user".to_string());
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
async fn clear_conversation(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    log::debug!("clear_conversation invoked (starts a new session)");
    let new_id = omnilauncher_lib::db::conversation::start_new_session(None);
    let mut ctx = state.conversation.lock().await;
    ctx.clear();
    ctx.session_id = new_id;
    Ok(true)
}

#[tauri::command]
async fn list_ai_sessions() -> Result<Vec<omnilauncher_lib::db::conversation::SessionInfo>, String>
{
    Ok(omnilauncher_lib::db::conversation::list_sessions())
}

#[tauri::command]
async fn current_ai_session(state: tauri::State<'_, AppState>) -> Result<i64, String> {
    let ctx = state.conversation.lock().await;
    Ok(ctx.session_id)
}

#[tauri::command]
async fn switch_ai_session(
    session_id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    omnilauncher_lib::db::conversation::touch_for_switch(session_id);
    let msgs = omnilauncher_lib::db::conversation::load_recent_for_session(session_id, 200);
    let mut ctx = state.conversation.lock().await;
    ctx.clear();
    ctx.session_id = session_id;
    // Re-hydrate in-memory context with the most recent slice (bounded by max_turns).
    let take_n = ctx.max_turns * 2;
    let start = msgs.len().saturating_sub(take_n);
    ctx.messages = msgs[start..].to_vec();
    // Return the full session transcript so the UI can render it.
    let payload: Vec<serde_json::Value> = msgs
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content_str(),
            })
        })
        .collect();
    Ok(payload)
}

#[tauri::command]
async fn delete_ai_session(
    session_id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<i64, String> {
    let ok = omnilauncher_lib::db::conversation::delete_session(session_id);
    if !ok {
        return Err("Failed to delete session".to_string());
    }
    // If we just deleted the active session, fall back to a fresh one.
    let mut ctx = state.conversation.lock().await;
    if ctx.session_id == session_id {
        let new_id = omnilauncher_lib::db::conversation::current_session_id();
        ctx.clear();
        ctx.session_id = new_id;
        Ok(new_id)
    } else {
        Ok(ctx.session_id)
    }
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
        state
            .live_server
            .url(state.live_server_port, "/dashboard/todos")
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
        "plugin_execute" => {
            // Routing tag is encoded as `<plugin_name>::<original_id>` on the
            // result `id` by ExternalPlugin so we can call back into the right
            // plugin via op=execute.
            let (plugin_name, inner_id) = match result.id.split_once("::") {
                Some((name, id)) => (name.to_string(), id.to_string()),
                None => {
                    log::warn!(
                        "plugin_execute result id={} missing `<plugin>::` routing prefix",
                        result.id
                    );
                    return Ok(false);
                }
            };
            let pm = state.plugin_manager.lock().await;
            match pm
                .execute_action(&plugin_name, &inner_id, &action_data)
                .await
            {
                Some(output) => {
                    log::info!(
                        "plugin_execute '{}' (id={}) returned: {}",
                        plugin_name,
                        inner_id,
                        output
                    );
                    true
                }
                None => {
                    log::warn!(
                        "plugin_execute target plugin '{}' did not handle the request",
                        plugin_name
                    );
                    false
                }
            }
        }
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

#[tauri::command]
async fn update_skill(name: String, state: tauri::State<'_, AppState>) -> Result<String, String> {
    log::debug!("update_skill invoked with name={name}");
    let mut mgr = state.skill_manager.lock().await;
    mgr.update_skill(&name)
}

// ─── External plugin management commands ──────────────────────────────────────

/// Refresh the in-memory `PluginManager` so newly installed / updated /
/// removed external plugins (including their AI `tool_schema`s) become
/// visible immediately, without restarting the launcher.
async fn reload_external_plugins(state: &tauri::State<'_, AppState>) {
    let settings = omnilauncher_lib::load_settings();
    let mut pm = state.plugin_manager.lock().await;
    pm.reload_external_plugins(&settings.plugin_dirs);
}

#[tauri::command]
async fn install_plugin(
    source: String,
    target_dir: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    log::debug!("install_plugin invoked with source={source} target_dir={target_dir:?}");
    let result =
        omnilauncher_lib::plugins::plugin_manager_cmd::install_plugin(source, target_dir).await?;
    reload_external_plugins(&state).await;
    Ok(result)
}

#[tauri::command]
async fn update_plugin(name: String, state: tauri::State<'_, AppState>) -> Result<String, String> {
    log::debug!("update_plugin invoked with name={name}");
    let result = omnilauncher_lib::plugins::plugin_manager_cmd::update_plugin(name).await?;
    reload_external_plugins(&state).await;
    Ok(result)
}

#[tauri::command]
async fn update_plugin_collection(
    source: String,
    plugin_dirs: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    log::debug!(
        "update_plugin_collection invoked with source={source} plugin_dirs={plugin_dirs:?}"
    );
    let result = omnilauncher_lib::plugins::plugin_manager_cmd::update_plugin_collection(
        source,
        plugin_dirs,
    )
    .await?;
    reload_external_plugins(&state).await;
    Ok(result)
}

#[tauri::command]
fn list_plugins() -> Vec<serde_json::Value> {
    log::trace!("list_plugins invoked");
    omnilauncher_lib::plugins::plugin_manager_cmd::list_plugins()
}

#[tauri::command]
async fn remove_plugin(name: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    log::debug!("remove_plugin invoked with name={name}");
    omnilauncher_lib::plugins::plugin_manager_cmd::remove_plugin(name).await?;
    reload_external_plugins(&state).await;
    Ok(())
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
        let sem = Arc::new(Semaphore::new(1));
        let state = AppState {
            plugin_manager: Arc::new(Mutex::new(create_plugin_manager())),
            ai_client: Arc::new(Mutex::new(AiClient::new(
                String::new(),
                String::new(),
                String::new(),
            ))),
            settings: Arc::new(Mutex::new(AppSettings::default())),
            conversation: Arc::new(Mutex::new(ConversationContext::default())),
            ai_in_flight: sem.clone(),
            current_ai_task: Arc::new(Mutex::new(None)),
            skill_manager: Arc::new(Mutex::new(SkillManager::new())),
            live_server: LiveServer::new(),
            live_server_port: 0,
        };

        let first = state
            .ai_in_flight
            .clone()
            .try_acquire_owned()
            .expect("first request starts");
        let second = state.ai_in_flight.clone().try_acquire_owned();

        assert!(second.is_err(), "AI response is still in progress");
        drop(first);
        assert!(state.ai_in_flight.clone().try_acquire_owned().is_ok());
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
