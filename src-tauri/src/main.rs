use omnilauncher_lib::{
    ai::{
        client::AiClient,
        router::{ConversationContext, Router},
    },
    create_plugin_manager_builtin_only,
    live_server::LiveServer,
    load_settings, save_settings, server, AppSettings, QueryResult, SkillInfo, SkillManager,
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

fn window_pos_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let mut p = std::path::PathBuf::from(home);
    p.push(".config");
    p.push("omnilauncher");
    let _ = std::fs::create_dir_all(&p);
    p.push("window-pos.json");
    p
}

/// Path where the API server writes its per-launch auth token so the Tauri
/// shell can read it back via the `get_server_token` command.
fn server_token_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let mut p = std::path::PathBuf::from(home);
    p.push(".config");
    p.push("omnilauncher");
    let _ = std::fs::create_dir_all(&p);
    p.push("server-token");
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

/// Resolve the backend URL for the desktop shell:
/// 1. `OMNILAUNCHER_BACKEND_URL` env override
/// 2. `settings.backend_url`
/// 3. default `http://127.0.0.1:1422`
fn resolve_backend_url(settings: &AppSettings) -> String {
    if let Ok(url) = std::env::var("OMNILAUNCHER_BACKEND_URL") {
        if !url.trim().is_empty() {
            log::info!(
                "backend URL resolved from OMNILAUNCHER_BACKEND_URL={}",
                url.trim()
            );
            return url.trim().to_string();
        }
        log::debug!("OMNILAUNCHER_BACKEND_URL is set but empty; falling back");
    }
    if !settings.backend_url.trim().is_empty() {
        log::info!(
            "backend URL resolved from settings.backend_url={}",
            settings.backend_url.trim()
        );
        return settings.backend_url.trim().to_string();
    }
    log::info!("backend URL resolved from built-in default http://127.0.0.1:1422");
    "http://127.0.0.1:1422".to_string()
}

/// Resolve the auth token the desktop shell should send to the separated
/// backend. Mirrors `resolve_backend_url`'s precedence and is shared between
/// the Tauri startup window-injection path and the `get_server_token` Tauri
/// command.
///
/// Precedence:
///   1. `OMNILAUNCHER_AUTH_TOKEN` env override
///   2. `settings.backend_token`
///   3. `~/.config/omnilauncher/server-token` (same-machine fallback, written
///      by the `--server` process at startup)
///
/// Returns an empty string when none of the three is present — callers either
/// already noticed the empty token (logged) or are running in browser/mock
/// mode where token-less requests are expected.
fn resolve_auth_token(settings: &AppSettings) -> String {
    omnilauncher_lib::settings::resolve_backend_auth_token(settings)
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

/// Emit a single info-level banner so debug logs and stderr always start
/// with a clear "what process, in what mode" line. Useful when sifting
/// through ~/.omnilauncher/omnilauncher.log with multiple sessions appended.
fn log_startup_banner(role: &str, debug_enabled: bool) {
    let version = env!("CARGO_PKG_VERSION");
    let log_target = if debug_enabled {
        debug_log_path().display().to_string()
    } else {
        "stderr".to_string()
    };
    log::info!(
        "OmniLauncher starting role={role} version={version} debug={debug_enabled} log={log_target}"
    );
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
/// Parse a hotkey spec like `"Ctrl+Shift+O"`, `"Alt+Space"`, `"Cmd+K"`, `"F12"`
/// into a `Shortcut`. Tokens are separated by `+` and case-insensitive.
/// Modifier tokens: `ctrl`/`control`, `shift`, `alt`/`option`,
/// `cmd`/`command`/`super`/`meta`. The final token is the key.
///
/// Returns `None` if the spec is empty, malformed, or names an unknown key —
/// callers are expected to fall back to a hard-coded default and surface the
/// failure to the user.
fn parse_shortcut(spec: &str) -> Option<Shortcut> {
    let parts: Vec<&str> = spec
        .split('+')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return None;
    }
    let (key_token, mod_tokens) = parts.split_last().unwrap();
    let mut mods = Modifiers::empty();
    for m in mod_tokens {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "shift" => mods |= Modifiers::SHIFT,
            "alt" | "option" => mods |= Modifiers::ALT,
            "cmd" | "command" | "super" | "meta" | "win" => mods |= Modifiers::SUPER,
            _ => return None,
        }
    }
    let key = parse_key_code(key_token)?;
    let mods_opt = if mods.is_empty() { None } else { Some(mods) };
    Some(Shortcut::new(mods_opt, key))
}

fn parse_key_code(token: &str) -> Option<Code> {
    let t = token.trim();
    if t.is_empty() {
        return None;
    }
    // Single-character keys: letters and digits.
    if t.len() == 1 {
        let c = t.chars().next().unwrap().to_ascii_uppercase();
        return match c {
            'A' => Some(Code::KeyA),
            'B' => Some(Code::KeyB),
            'C' => Some(Code::KeyC),
            'D' => Some(Code::KeyD),
            'E' => Some(Code::KeyE),
            'F' => Some(Code::KeyF),
            'G' => Some(Code::KeyG),
            'H' => Some(Code::KeyH),
            'I' => Some(Code::KeyI),
            'J' => Some(Code::KeyJ),
            'K' => Some(Code::KeyK),
            'L' => Some(Code::KeyL),
            'M' => Some(Code::KeyM),
            'N' => Some(Code::KeyN),
            'O' => Some(Code::KeyO),
            'P' => Some(Code::KeyP),
            'Q' => Some(Code::KeyQ),
            'R' => Some(Code::KeyR),
            'S' => Some(Code::KeyS),
            'T' => Some(Code::KeyT),
            'U' => Some(Code::KeyU),
            'V' => Some(Code::KeyV),
            'W' => Some(Code::KeyW),
            'X' => Some(Code::KeyX),
            'Y' => Some(Code::KeyY),
            'Z' => Some(Code::KeyZ),
            '0' => Some(Code::Digit0),
            '1' => Some(Code::Digit1),
            '2' => Some(Code::Digit2),
            '3' => Some(Code::Digit3),
            '4' => Some(Code::Digit4),
            '5' => Some(Code::Digit5),
            '6' => Some(Code::Digit6),
            '7' => Some(Code::Digit7),
            '8' => Some(Code::Digit8),
            '9' => Some(Code::Digit9),
            _ => None,
        };
    }
    match t.to_ascii_lowercase().as_str() {
        "space" => Some(Code::Space),
        "enter" | "return" => Some(Code::Enter),
        "escape" | "esc" => Some(Code::Escape),
        "tab" => Some(Code::Tab),
        "backspace" => Some(Code::Backspace),
        "f1" => Some(Code::F1),
        "f2" => Some(Code::F2),
        "f3" => Some(Code::F3),
        "f4" => Some(Code::F4),
        "f5" => Some(Code::F5),
        "f6" => Some(Code::F6),
        "f7" => Some(Code::F7),
        "f8" => Some(Code::F8),
        "f9" => Some(Code::F9),
        "f10" => Some(Code::F10),
        "f11" => Some(Code::F11),
        "f12" => Some(Code::F12),
        _ => None,
    }
}

pub fn run() {
    log::info!("Starting OmniLauncher runtime");
    let settings = load_settings();
    log::debug!(
        "Loaded settings (base_url={}, model={}, max_results={})",
        settings.ai_base_url,
        settings.ai_model,
        settings.max_results
    );

    let ai_client = AiClient::with_timeout(
        settings.ai_base_url.clone(),
        settings.resolve_ai_api_key(),
        settings.ai_model.clone(),
        settings.ai_timeout_secs,
    );

    let mut skill_manager = SkillManager::new();
    skill_manager.load_all();
    log::debug!("Loaded skill manager");

    let live_server_port = 1421;
    let live_server = LiveServer::new();

    // NOTE (backend/UI split): the desktop app is now a thin UI shell. It no
    // longer serves the in-process dashboard live server — that responsibility
    // moves to the separated backend. The `live_server` handle is retained only
    // so the (now dormant, HTTP-routed) Tauri commands still type-check; nothing
    // is served on port 1421 from the shell.

    // Bring up the launcher with built-in plugins only — external plugin
    // discovery (manifest reads, JSON parses, raycast/flow shim refresh)
    // runs in a background task spawned from `setup` below. This shaves
    // a noticeable chunk off cold-start; the `cheap_prefix_match` gate in
    // PluginManager protects against early keystrokes hitting an
    // un-loaded plugin.
    let state = AppState {
        plugin_manager: Arc::new(Mutex::new(create_plugin_manager_builtin_only())),
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

    // Parse the configured hotkey from settings (default "Alt+Space"). On parse
    // failure we log + fall back to Ctrl+Shift+O so the launcher is always
    // reachable.
    let settings_for_hotkey = omnilauncher_lib::settings::load_settings();
    let shortcut = parse_shortcut(&settings_for_hotkey.hotkey).unwrap_or_else(|| {
        log::warn!(
            "settings.hotkey '{}' did not parse — falling back to Ctrl+Shift+O",
            settings_for_hotkey.hotkey
        );
        Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyO)
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(state)
        .setup(move |app| {
            log::debug!("Running Tauri setup");

            // Backend/UI split: the desktop shell no longer discovers external
            // plugins, runs the skill curator, or serves a dashboard — all of
            // that now lives in the separated backend. The shell keeps only
            // window/hotkey/tray/selection concerns. We DO keep the local
            // scheduler so OS-level scheduled jobs still fire on this machine.
            omnilauncher_lib::plugins::scheduler::migrate_inline_commands_to_files();
            omnilauncher_lib::plugins::scheduler::start_scheduler();

            let window = app.get_webview_window("main").unwrap();

            // Inject the backend URL the frontend should talk to BEFORE it makes
            // any request. `runtime.ts` reads `window.__OMNILAUNCHER_BACKEND_URL__`
            // lazily (at first `invoke`), so this eval wins the race.
            let settings_for_url = omnilauncher_lib::load_settings();
            let backend_url = resolve_backend_url(&settings_for_url);
            log::info!("desktop shell will use backend at {backend_url}");
            let _ = window.eval(format!(
                "window.__OMNILAUNCHER_BACKEND_URL__ = {};",
                serde_json::to_string(&backend_url).unwrap_or_else(|_| "\"\"".to_string())
            ));

            // Inject the auth token the frontend should send with every backend
            // request. Same lazy-read race as backend_url — `runtime.ts` reads
            // `window.__OMNILAUNCHER_TOKEN__` at first `httpJson` call. Lets the
            // Windows shell + WSL backend topology work: the shell never reads
            // the WSL-side `~/.config/omnilauncher/server-token` (different
            // filesystem), it just trusts what the user pinned via env or
            // settings. When the token is empty we still inject the empty
            // string so the frontend's `tauriInvoke` fallback path can detect
            // "shell tried but had nothing" vs. "running in a browser".
            let auth_token = resolve_auth_token(&settings_for_url);
            if auth_token.is_empty() {
                log::warn!("desktop shell has no auth token — backend requests will be unauthenticated");
            } else {
                log::info!("desktop shell will send X-OmniLauncher-Token header (len={})", auth_token.len());
            }
            let _ = window.eval(format!(
                "window.__OMNILAUNCHER_TOKEN__ = {};",
                serde_json::to_string(&auth_token).unwrap_or_else(|_| "\"\"".to_string())
            ));

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
            // Embed the icon at compile time so it is always available
            // regardless of the working directory or whether the build was
            // bundled (e.g. `tauri build --no-bundle`). Falling back to a
            // filesystem path breaks in prod, where the cwd is not src-tauri
            // and the icon is not copied next to the binary.
            const ICON_BYTES: &[u8] = include_bytes!("../icons/32x32.png");
            let icon = Image::from_bytes(ICON_BYTES).ok();

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
                        // FIX (Windows): a *minimized* window still reports
                        // `is_visible() == true`, and `show()` does NOT restore
                        // a minimized window. Treat "minimized" as not-visible
                        // and always `unminimize()` before showing so the UI
                        // actually appears instead of only the taskbar icon.
                        let shown = tray_window.is_visible().unwrap_or(false)
                            && !tray_window.is_minimized().unwrap_or(false);
                        if shown {
                            let _ = tray_window.hide();
                        } else {
                            let _ = tray_window.unminimize();
                            let _ = tray_window.show();
                            let _ = tray_window.set_focus();
                            // FIX: emit a String payload so the frontend
                            // listener (which deserializes selection as a
                            // string) handles tray clicks identically to the
                            // hotkey path. Previously this sent `()`, which
                            // caused a runtime deserialize error in the
                            // webview's `omnilauncher://shown` listener.
                            let _ = tray_window.emit("omnilauncher://shown", String::new());
                        }
                    }
                })
                .build(app)?;

            let global_shortcut = app.global_shortcut();

            // FIX: surface registration failures to the frontend so the user
            // sees a banner instead of a silently-dead hotkey. We still allow
            // the app to start (the tray click is a working fallback).
            let shortcut_window = window.clone();
            let register_result =
                global_shortcut.on_shortcut(shortcut, move |_app, _shortcut, event| {
                    if let tauri_plugin_global_shortcut::ShortcutState::Pressed = event.state() {
                        log::trace!("Global shortcut pressed; toggling main window visibility");
                        // FIX (Windows): a *minimized* window still reports
                        // `is_visible() == true`, and `show()` does NOT restore
                        // a minimized window. Treat "minimized" as not-visible
                        // so the toggle restores the UI instead of only the
                        // taskbar icon.
                        let shown = shortcut_window.is_visible().unwrap_or(false)
                            && !shortcut_window.is_minimized().unwrap_or(false);
                        if shown {
                            let _ = shortcut_window.hide();
                            return;
                        }
                        // FIX: previously this called `foreground_is_ours()`
                        // (synchronous PowerShell on Windows) and a blocking
                        // X11 selection read directly on the global-hotkey
                        // dispatcher thread, freezing the GUI for ~200-500ms
                        // on every press. Move the capture work off-thread
                        // and show the window immediately so the launcher
                        // appears instantly; the selection is delivered as a
                        // follow-up event when ready.
                        let _ = shortcut_window.unminimize();
                        let _ = shortcut_window.show();
                        let _ = shortcut_window.set_focus();
                        let _ = shortcut_window.emit("omnilauncher://shown", String::new());

                        let win_for_selection = shortcut_window.clone();
                        std::thread::spawn(move || {
                            let cfg = omnilauncher_lib::settings::load_settings();
                            if !cfg.capture_selection_on_open {
                                return;
                            }
                            if foreground_is_ours() {
                                return;
                            }
                            let selection =
                                omnilauncher_lib::plugins::selection::read_x11_selection()
                                    .unwrap_or_default();
                            if !selection.is_empty() {
                                let _ =
                                    win_for_selection.emit("omnilauncher://selection", selection);
                            }
                        });
                    }
                });

            if let Err(err) = register_result {
                log::warn!("Failed to register global shortcut: {err}");
                eprintln!("Failed to register global shortcut: {err}");
                // Emit on the main window so the UI can surface a toast.
                let _ = window.emit(
                    "omnilauncher://hotkey-error",
                    format!("Failed to register Ctrl+Shift+O: {err}"),
                );
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            search,
            ai_query,
            ai_cancel,
            execute_result,
            slash_preview,
            get_settings,
            get_launcher_config,
            list_favorites,
            add_favorite,
            remove_favorite,
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
            list_skill_usage,
            pin_skill,
            run_curator_now,
            propose_skill_consolidation,
            apply_skill_consolidation,
            set_window_geometry,
            save_window_position,
            install_plugin,
            update_plugin,
            update_plugin_collection,
            list_plugin_collections,
            update_plugin_collection_all,
            remove_plugin_collection,
            list_plugins,
            list_quarantined_plugins,
            remove_plugin,
            capture_vision_screenshot,
            set_window_size_centered,
            frontend_log,
            list_plugin_runtime_dependencies,
            install_plugin_runtime_dependency,
            omnilauncher_lib::python_installer::install_python_command,
            omnilauncher_lib::python_installer::check_bundled_python,
            get_server_token,
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

#[tauri::command]
fn frontend_log(level: String, message: String) -> Result<(), String> {
    match level.as_str() {
        "error" => log::error!("[frontend] {message}"),
        "warn" => log::warn!("[frontend] {message}"),
        "debug" => log::debug!("[frontend] {message}"),
        "trace" => log::trace!("[frontend] {message}"),
        _ => log::info!("[frontend] {message}"),
    }
    Ok(())
}

/// Return the auth token the desktop shell should pass to the separated
/// backend.
///
/// Resolution mirrors `resolve_auth_token`:
///   1. `OMNILAUNCHER_AUTH_TOKEN` env override
///   2. `settings.backend_token` (lets the user pin a stable token via the
///      Preferences UI — required when shell and backend live on different
///      machines, e.g. Windows shell + WSL backend)
///   3. `~/.config/omnilauncher/server-token` (same-machine fallback the
///      `--server` process writes at startup)
///
/// We intentionally accept the synchronous `tauri::State` getter cost here so
/// the frontend's `serverTokenPromise` stays a one-call resolution path.
#[tauri::command]
fn get_server_token(state: tauri::State<'_, AppState>) -> String {
    let settings = state.settings.blocking_lock().clone();
    resolve_auth_token(&settings)
}

/// Resize the window to an explicit logical size while keeping it centered on
/// the current monitor. Driven by the front-end corner resize grip.
#[tauri::command]
async fn set_window_size_centered(
    window: tauri::WebviewWindow,
    width: f64,
    height: f64,
) -> Result<(), String> {
    if let Some(monitor) = window.current_monitor().map_err(|e| e.to_string())? {
        let scale_factor = monitor.scale_factor();
        let monitor_size = monitor.size();
        let monitor_position = monitor.position();
        let monitor_width = monitor_size.width as f64 / scale_factor;
        let monitor_height = monitor_size.height as f64 / scale_factor;
        let monitor_x = monitor_position.x as f64 / scale_factor;
        let monitor_y = monitor_position.y as f64 / scale_factor;

        let win_width = width.clamp(360.0, monitor_width);
        let win_height = height.clamp(120.0, monitor_height);
        let window_x = monitor_x + (monitor_width - win_width) / 2.0;
        let window_y = monitor_y + (monitor_height - win_height) / 2.0;

        window
            .set_size(Size::Logical(LogicalSize::new(win_width, win_height)))
            .map_err(|e| e.to_string())?;
        window
            .set_position(Position::Logical(LogicalPosition::new(window_x, window_y)))
            .map_err(|e| e.to_string())?;
    } else {
        let win_width = width.clamp(360.0, 3000.0);
        let win_height = height.clamp(120.0, 2000.0);
        window
            .set_size(Size::Logical(LogicalSize::new(win_width, win_height)))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
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

        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<String>(64);

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
    let handle = {
        let mut slot = state.current_ai_task.lock().await;
        slot.take()
    };
    if let Some(handle) = handle {
        handle.abort();
        // Wait for the aborted task to fully unwind so its semaphore permit
        // (`_permit`, held inside the task) is dropped before we return. Without
        // this, a follow-up `ai_query` can observe the permit as still held and
        // fail with "AI response is still in progress".
        let _ = handle.await;
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
    Ok(omnilauncher_lib::server::slash_preview_backend(&query, &pm).await)
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
                // SECURITY: avoid `cmd /C start "" <url>` — `start` is a cmd.exe
                // builtin and treats `&`, `|`, `^`, `%` etc. in the URL as
                // shell metacharacters. `rundll32 url.dll,FileProtocolHandler`
                // hands the URL directly to the registered protocol handler
                // without a shell parser in the loop.
                spawn_external_command(
                    "rundll32",
                    &["url.dll,FileProtocolHandler", &action_data],
                    "open url",
                )
            }
        }
        "shell" | "open_app" => {
            #[cfg(target_os = "windows")]
            {
                // SECURITY: see above re. `cmd /C start`. For app launches we
                // route through explorer.exe, which resolves the target via
                // ShellExecute semantics without a cmd parser.
                spawn_external_command("explorer", &[&result.action_data], "open app")
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
        "kill_pid" => {
            // SECURITY: PID-typed kill path. action_data MUST parse to u32;
            // we never shell out, so no string can be smuggled into a command.
            match result.action_data.trim().parse::<u32>() {
                Ok(pid) => {
                    log::info!("kill_pid requested for pid={pid}");
                    use sysinfo::{Pid, Signal, System};
                    let mut sys = System::new();
                    sys.refresh_process(Pid::from_u32(pid));
                    match sys.process(Pid::from_u32(pid)) {
                        Some(proc) => {
                            // kill_with(SIGKILL) on unix, terminate on win.
                            let ok = proc.kill_with(Signal::Kill).unwrap_or_else(|| proc.kill());
                            if !ok {
                                log::warn!("kill_pid failed to terminate pid={pid}");
                            }
                            ok
                        }
                        None => {
                            log::warn!("kill_pid: pid={pid} not found");
                            false
                        }
                    }
                }
                Err(err) => {
                    log::warn!(
                        "kill_pid rejected non-numeric action_data {:?}: {err}",
                        result.action_data
                    );
                    false
                }
            }
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

/// Returns the launcher input rule-set (AI prefixes, slash-command catalog,
/// navigation aliases) so the frontend can evaluate which UI chrome to show
/// synchronously without re-implementing the rules. Single source of truth lives
/// in `omnilauncher_lib::launcher_config`.
#[tauri::command]
fn get_launcher_config() -> omnilauncher_lib::launcher_config::LauncherConfig {
    omnilauncher_lib::launcher_config::LauncherConfig::current()
}

// ─── Favorites commands ─────────────────────────────────────────────────────────

#[tauri::command]
fn list_favorites() -> Vec<QueryResult> {
    log::trace!("list_favorites invoked");
    omnilauncher_lib::db::favorites::list_favorites()
}

#[tauri::command]
fn add_favorite(result: QueryResult) -> Result<(), String> {
    log::debug!("add_favorite invoked id={}", result.id);
    omnilauncher_lib::db::favorites::add_favorite(&result)
}

#[tauri::command]
fn remove_favorite(id: String) -> Result<(), String> {
    log::debug!("remove_favorite invoked id={id}");
    omnilauncher_lib::db::favorites::remove_favorite(&id)
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
    *client = AiClient::with_timeout(
        settings.ai_base_url.clone(),
        settings.resolve_ai_api_key(),
        settings.ai_model.clone(),
        settings.ai_timeout_secs,
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
    // Installing shells out to git/gh/curl and may clone an entire repo, which
    // can take many seconds. Doing that directly on the async runtime thread
    // (while holding the lock) stalls IPC and makes the whole UI freeze / show
    // a black window. Run it on a dedicated blocking thread instead so the
    // runtime stays responsive.
    let mgr = state.skill_manager.clone();
    tokio::task::spawn_blocking(move || {
        let mut mgr = mgr.blocking_lock();
        if source.starts_with("http://") || source.starts_with("https://") {
            mgr.install_from_url(&source)
        } else {
            mgr.install_from_path(&source)
        }
    })
    .await
    .map_err(|e| format!("install task failed: {e}"))?
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
    // Like install, updating re-fetches over the network (git/gh/curl). Keep it
    // off the async runtime so the UI stays responsive during the clone.
    let mgr = state.skill_manager.clone();
    tokio::task::spawn_blocking(move || {
        let mut mgr = mgr.blocking_lock();
        mgr.update_skill(&name)
    })
    .await
    .map_err(|e| format!("update task failed: {e}"))?
}

// ─── Curator commands ─────────────────────────────────────────────────────────

#[tauri::command]
async fn list_skill_usage() -> Result<omnilauncher_lib::skills::curator::UsageStore, String> {
    Ok(omnilauncher_lib::skills::curator::snapshot())
}

#[tauri::command]
async fn pin_skill(name: String, pinned: bool) -> Result<bool, String> {
    omnilauncher_lib::skills::curator::set_pinned(&name, pinned);
    Ok(true)
}

/// Force the curator to run now (UI "Run curator" action). Returns counts of
/// transitions so the UI can surface a toast.
#[tauri::command]
async fn run_curator_now(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let mgr = state.skill_manager.lock().await;
    let names = mgr.user_skill_names();
    drop(mgr);
    let report =
        tokio::task::spawn_blocking(move || omnilauncher_lib::skills::curator::evaluate(&names))
            .await
            .map_err(|e| format!("curator task failed: {e}"))?;
    Ok(serde_json::json!({
        "marked_stale": report.marked_stale,
        "marked_archived": report.marked_archived,
        "seen_new": report.seen_new,
        "total_tracked": report.total_tracked,
    }))
}

/// LLM-driven consolidation phase 2: ask the model for proposals (read-only;
/// nothing on disk is touched). The frontend surfaces these for per-item
/// approval; only `apply_skill_consolidation` mutates files.
#[tauri::command]
async fn propose_skill_consolidation(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<omnilauncher_lib::skills::consolidate::Proposal>, String> {
    let skills_clone: Vec<omnilauncher_lib::skills::Skill> = {
        let mgr = state.skill_manager.lock().await;
        // Only consider user-installed skills — bundled ones are
        // canonical and not subject to user-initiated consolidation.
        let user_names = mgr.user_skill_names();
        mgr.list_meta()
            .iter()
            .filter(|m| user_names.iter().any(|n| n == &m.name))
            .filter_map(|m| mgr.get_by_name(&m.name).cloned())
            .collect()
    };
    let ai = state.ai_client.lock().await;
    omnilauncher_lib::skills::consolidate::propose(&skills_clone, &ai)
        .await
        .map_err(|e| format!("LLM propose failed: {e}"))
}

/// Apply one user-approved consolidation proposal. Always backs up the
/// affected SKILL.md(s) first; never deletes a primary skill.
#[tauri::command]
async fn apply_skill_consolidation(
    state: tauri::State<'_, AppState>,
    proposal: omnilauncher_lib::skills::consolidate::Proposal,
) -> Result<omnilauncher_lib::skills::consolidate::ApplyOutcome, String> {
    let mut mgr = state.skill_manager.lock().await;
    omnilauncher_lib::skills::consolidate::apply(&proposal, &mut mgr)
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

/// List installed plugins grouped into collections, ready for the UI to render.
/// Grouping (git-remote normalization, collection keying) is done in the
/// backend so the frontend no longer reshapes raw plugin JSON.
#[tauri::command]
fn list_plugin_collections(
) -> Vec<omnilauncher_lib::plugins::plugin_manager_cmd::PluginCollectionInfo> {
    log::trace!("list_plugin_collections invoked");
    omnilauncher_lib::plugins::plugin_manager_cmd::list_plugin_collections()
}

/// Update every git-backed repo in a collection (or a source-based collection),
/// returning a single summary result. Replaces the per-repo update loop that
/// used to live in the frontend.
#[tauri::command]
async fn update_plugin_collection_all(
    collection_source: Option<String>,
    repo_dirs: Vec<String>,
    git_repo_dirs: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<omnilauncher_lib::plugins::plugin_manager_cmd::CollectionOpResult, String> {
    log::debug!(
        "update_plugin_collection_all invoked ({} git repos)",
        git_repo_dirs.len()
    );
    let result = omnilauncher_lib::plugins::plugin_manager_cmd::update_plugin_collection_all(
        collection_source,
        repo_dirs,
        git_repo_dirs,
    )
    .await?;
    reload_external_plugins(&state).await;
    Ok(result)
}

/// Remove every repo in a collection, returning a single summary result.
/// Replaces the per-repo remove loop that used to live in the frontend.
#[tauri::command]
async fn remove_plugin_collection(
    repo_dirs: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<omnilauncher_lib::plugins::plugin_manager_cmd::CollectionOpResult, String> {
    log::debug!(
        "remove_plugin_collection invoked ({} repos)",
        repo_dirs.len()
    );
    let result =
        omnilauncher_lib::plugins::plugin_manager_cmd::remove_plugin_collection(repo_dirs).await?;
    reload_external_plugins(&state).await;
    Ok(result)
}

#[derive(Clone, serde::Serialize)]
struct PluginRuntimeProgress<'a> {
    id: &'a str,
    label: &'static str,
    message: String,
}

#[tauri::command]
fn list_plugin_runtime_dependencies(
) -> Vec<omnilauncher_lib::plugins::runtime_deps::PluginRuntimeDependency> {
    omnilauncher_lib::plugins::runtime_deps::list_runtime_dependencies()
}

#[tauri::command]
async fn install_plugin_runtime_dependency(
    id: String,
    window: tauri::WebviewWindow,
) -> Result<String, String> {
    match id.as_str() {
        "python" => {
            emit_runtime_progress(&window, "python", "Starting Python runtime install.");
            let progress_window = window.clone();
            let exe = omnilauncher_lib::python_installer::install_bundled_python_with_progress(
                |message| {
                    emit_runtime_progress(&progress_window, "python", message);
                },
            )
            .await?;
            emit_runtime_progress(&window, "python", "Python runtime installed.");
            Ok(format!("Python installed at {}", exe.display()))
        }
        "node" | "dotnet" => install_system_runtime(&id, &window).await,
        _ => Err(format!("Unknown plugin runtime dependency: {id}")),
    }
}

fn emit_runtime_progress(window: &tauri::WebviewWindow, id: &str, message: &str) {
    let _ = window.emit(
        "omnilauncher://plugin-runtime-progress",
        PluginRuntimeProgress {
            id,
            label: omnilauncher_lib::plugins::runtime_deps::runtime_label(id),
            message: message.to_string(),
        },
    );
}

async fn install_system_runtime(id: &str, window: &tauri::WebviewWindow) -> Result<String, String> {
    use omnilauncher_lib::plugins::runtime_deps::{
        runtime_install_plan, runtime_label, runtime_manual_command,
    };
    let (program, args, display_command) = runtime_install_plan(id)?;
    emit_runtime_progress(
        window,
        id,
        &format!("Starting {} installer.", runtime_label(id)),
    );
    emit_runtime_progress(window, id, &format!("Running: {display_command}"));
    let output = tokio::process::Command::new(&program)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to run {display_command}: {e}"))?;

    if output.status.success() {
        emit_runtime_progress(
            window,
            id,
            &format!("{} installer completed.", runtime_label(id)),
        );
        Ok(format!("Installed {}", runtime_label(id)))
    } else {
        emit_runtime_progress(
            window,
            id,
            &format!("{} installer failed.", runtime_label(id)),
        );
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("installer exited with status {}", output.status)
        };
        Err(format!(
            "Failed to install {}. Run manually: {}\n{}",
            runtime_label(id),
            runtime_manual_command(id).unwrap_or(display_command),
            detail.chars().take(1200).collect::<String>()
        ))
    }
}

#[tauri::command]
fn list_plugins() -> Vec<serde_json::Value> {
    log::trace!("list_plugins invoked");
    omnilauncher_lib::plugins::plugin_manager_cmd::list_plugins()
}

#[tauri::command]
async fn list_quarantined_plugins(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    log::trace!("list_quarantined_plugins invoked");
    let pm = state.plugin_manager.lock().await;
    Ok(omnilauncher_lib::plugins::plugin_manager_cmd::list_quarantined_plugins(&pm))
}

#[tauri::command]
async fn remove_plugin(name: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    log::debug!("remove_plugin invoked with name={name}");
    omnilauncher_lib::plugins::plugin_manager_cmd::remove_plugin(name).await?;
    reload_external_plugins(&state).await;
    Ok(())
}

/// Capture a screenshot locally and return it base64-encoded. This is the
/// *local* half of vision: only the machine with a screen can grab one, so it
/// stays in the desktop shell even when all business logic (the AI call) runs
/// on a remote backend. The frontend POSTs the returned base64 to the backend's
/// `/api/vision/analyze` endpoint.
#[tauri::command]
async fn capture_vision_screenshot(window: tauri::WebviewWindow) -> Result<String, String> {
    use std::io::Read;
    log::debug!("capture_vision_screenshot invoked");

    // Hide the launcher so it doesn't appear in the screenshot.
    let _ = window.hide();
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    let tmp_path = std::env::temp_dir().join("omnilauncher_vision.png");
    let tmp_str = tmp_path.to_string_lossy().to_string();

    #[cfg(target_os = "windows")]
    {
        // Interactive region snip via Snip & Sketch (Win+Shift+S) writes the
        // capture to the clipboard; we then save the clipboard image to file.
        let ps = format!(
            r#"Add-Type -AssemblyName System.Windows.Forms,System.Drawing;
Start-Process 'ms-screenclip:';
$deadline=(Get-Date).AddSeconds(60);
do {{ Start-Sleep -Milliseconds 300; $img=[System.Windows.Forms.Clipboard]::GetImage() }} while (-not $img -and (Get-Date) -lt $deadline);
if (-not $img) {{ exit 1 }};
$img.Save('{}');"#,
            tmp_str.replace('\'', "''")
        );
        let status = tokio::process::Command::new("powershell")
            .args(["-WindowStyle", "Hidden", "-NoProfile", "-Command", &ps])
            .status()
            .await
            .map_err(|e| format!("screenshot failed: {e}"))?;
        if !status.success() || !tmp_path.exists() {
            let _ = window.show();
            let _ = window.set_focus();
            return Err("Screenshot was cancelled or failed.".to_string());
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let output = tokio::process::Command::new("scrot")
            .args(["-s", "--overwrite", &tmp_str])
            .output()
            .await
            .map_err(|e| format!("scrot failed: {e}. Is scrot installed?"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let _ = window.show();
            let _ = window.set_focus();
            return Err(format!("scrot exited with error: {}", stderr));
        }
    }

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

    let _ = std::fs::remove_file(&tmp_path);

    // Bring the launcher back so the user sees the result flow.
    let _ = window.show();
    let _ = window.set_focus();

    Ok(b64)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let debug_enabled = args.iter().any(|arg| arg == "--debug");
    let server_mode = args.iter().any(|arg| arg == "--server");
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

    let role = if server_mode {
        "server"
    } else {
        "tauri-shell"
    };
    log_startup_banner(role, debug_enabled);

    if server_mode {
        let settings = load_settings();
        let ai_client = AiClient::with_timeout(
            settings.ai_base_url.clone(),
            settings.resolve_ai_api_key(),
            settings.ai_model.clone(),
            settings.ai_timeout_secs,
        );
        let mut skill_manager = SkillManager::new();
        skill_manager.load_all();

        // Resolve the per-launch auth token. When `OMNILAUNCHER_AUTH_TOKEN`
        // is set we pin to it (lets cross-machine deployments — e.g. WSL
        // backend + Windows shell — agree on a stable, user-configured token
        // without depending on the local-only token file). Otherwise we
        // generate a fresh random one as before. In both cases we still
        // write the token to `~/.config/omnilauncher/server-token` so the
        // same-machine shell continues to work with zero configuration.
        let (auth_token, token_source) = match std::env::var("OMNILAUNCHER_AUTH_TOKEN") {
            Ok(t) if !t.trim().is_empty() => (t.trim().to_string(), "OMNILAUNCHER_AUTH_TOKEN env"),
            _ => (server::generate_auth_token(), "freshly generated random token"),
        };
        log::info!("server auth token sourced from {token_source}");
        if let Err(e) = std::fs::write(server_token_path(), auth_token.as_bytes()) {
            log::warn!("failed to persist server auth token: {e}");
        } else {
            log::info!("server auth token written to {}", server_token_path().display());
        }

        let mut conversation = ConversationContext::default();
        let sid = omnilauncher_lib::db::conversation::current_session_id();
        conversation.session_id = sid;
        conversation.messages =
            omnilauncher_lib::db::conversation::load_recent_for_session(sid, 20);

        let state = server::ServerState {
            plugin_manager: Arc::new(Mutex::new(create_plugin_manager_builtin_only())),
            ai_client: Arc::new(Mutex::new(ai_client)),
            settings: Arc::new(Mutex::new(settings.clone())),
            conversation: Arc::new(Mutex::new(conversation)),
            ai_in_flight: Arc::new(Semaphore::new(1)),
            current_ai_task: Arc::new(Mutex::new(None)),
            skill_manager: Arc::new(Mutex::new(skill_manager)),
            event_bus: server::EventBus::default(),
            latest_selection: Arc::new(Mutex::new(None)),
            auth_token: Arc::new(auth_token),
        };

        let host =
            std::env::var("OMNILAUNCHER_SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = std::env::var("OMNILAUNCHER_SERVER_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(1422);

        let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        rt.block_on(async move {
            server::spawn_api_server(state, host, port).await;
        });
        return;
    }

    run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_shortcut_accepts_default_alt_space() {
        // The default shipped in settings.rs MUST parse without falling back.
        assert!(parse_shortcut("Alt+Space").is_some());
    }

    #[test]
    fn parse_shortcut_handles_modifiers_and_keys() {
        assert!(parse_shortcut("Ctrl+Shift+O").is_some());
        assert!(parse_shortcut("cmd+k").is_some());
        assert!(parse_shortcut("F12").is_some());
        assert!(parse_shortcut("Ctrl+5").is_some());
    }

    #[test]
    fn parse_shortcut_rejects_garbage_so_caller_falls_back() {
        // Empty / lone-modifier / unknown-modifier / unknown-key all return
        // None so the caller can warn-and-default instead of crashing or
        // registering a useless binding.
        assert!(parse_shortcut("").is_none());
        assert!(parse_shortcut("Ctrl").is_none()); // only a modifier
        assert!(parse_shortcut("Hyper+J").is_none()); // unknown modifier
        assert!(parse_shortcut("Ctrl+NotARealKey").is_none());
    }

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
            plugin_manager: Arc::new(Mutex::new(create_plugin_manager_builtin_only())),
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
