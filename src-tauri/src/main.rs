use omnilauncher_lib::{
    ai::{client::AiClient, router::ConversationContext},
    create_plugin_manager_builtin_only, save_settings, server,
    settings::load_settings_with_overrides,
    SkillManager,
};
use simplelog::{ColorChoice, ConfigBuilder, LevelFilter, TermLogger, TerminalMode, WriteLogger};
use std::{fs, fs::OpenOptions, path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, RwLock, Semaphore};

/// Terminal CLI (`ol`): lifecycle/ops commands plus resource management.
/// See `cli/mod.rs`.
mod cli;

fn server_token_path() -> PathBuf {
    let dir = omnilauncher_lib::path_config::config_dir();
    let _ = fs::create_dir_all(&dir);
    dir.join("server-token")
}

fn debug_log_path() -> PathBuf {
    omnilauncher_lib::path_config::data_dir().join("omnilauncher.log")
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

/// Emit a startup banner so logs and stderr always identify the process role.
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let argv0 = args.first().cloned().unwrap_or_default();
    let rest: Vec<String> = args.iter().skip(1).cloned().collect();

    let debug_enabled = cli::wants_debug(&args);
    init_debug_logging(debug_enabled);

    if debug_enabled {
        log::info!("Running with --debug");
        log::debug!(
            "CLI args: {}",
            omnilauncher_lib::log_masking::mask_argv(&args)
        );
    } else {
        // Foreground CLI commands should not spew INFO chatter onto the terminal.
        // Only the long-running server path wants normal info logging.
        let term_level = if cli::is_foreground_cli(&argv0, &rest) {
            LevelFilter::Warn
        } else {
            LevelFilter::Info
        };
        if TermLogger::init(
            term_level,
            ConfigBuilder::new().build(),
            TerminalMode::Stderr,
            ColorChoice::Never,
        )
        .is_ok()
        {
            log::info!("Running without debug file logging");
        }
    }

    match cli::dispatch(&argv0, &rest) {
        cli::Dispatch::Serve => {
            log_startup_banner("server", debug_enabled);
            serve_backend(&args);
        }
        cli::Dispatch::Handled(code) => {
            if code != 0 {
                std::process::exit(code);
            }
        }
    }
}

/// Run the backend API server in the foreground. This is the body of the
/// historical `--server` mode, shared with the `ol serve` subcommand.
pub fn serve_backend(args: &[String]) {
    let settings = load_settings_with_overrides(args);
    let ai_client = AiClient::from_settings(&settings);
    let mut skill_manager = SkillManager::new();
    skill_manager.load_all();

    // Resolve the per-launch auth token. Precedence:
    //   1. `OMNILAUNCHER_AUTH_TOKEN` env (user-pinned, takes priority).
    //   2. Existing `~/.config/omnilauncher/server-token` on disk, reused across
    //      restarts so same-machine clients keep authenticating.
    //   3. Generate a fresh random token.
    let (auth_token, token_source) = match std::env::var("OMNILAUNCHER_AUTH_TOKEN") {
        Ok(t) if !t.trim().is_empty() => (t.trim().to_string(), "OMNILAUNCHER_AUTH_TOKEN env"),
        _ => match fs::read_to_string(server_token_path()) {
            Ok(s) if !s.trim().is_empty() => (
                s.trim().to_string(),
                "existing server-token file (reused across restarts)",
            ),
            _ => (
                server::generate_auth_token(),
                "freshly generated random token",
            ),
        },
    };
    log::info!("server auth token sourced from {token_source}");
    if !omnilauncher_lib::settings::write_private_file(&server_token_path(), auth_token.as_bytes())
    {
        log::warn!("failed to persist server auth token");
    } else {
        log::info!(
            "server auth token written to {}",
            server_token_path().display()
        );
    }

    let mut conversation = ConversationContext::default();
    let sid = omnilauncher_lib::db::conversation::current_session_id();
    conversation.session_id = sid;
    conversation.max_turns = settings.ai_max_tool_iterations;
    conversation.messages = omnilauncher_lib::db::conversation::load_recent_for_session(sid, 20);

    let state = server::ServerState {
        plugin_manager: Arc::new(RwLock::new(create_plugin_manager_builtin_only())),
        ai_client: Arc::new(RwLock::new(ai_client)),
        settings: Arc::new(RwLock::new(settings.clone())),
        conversation: Arc::new(Mutex::new(conversation)),
        ai_in_flight: Arc::new(Semaphore::new(1)),
        current_ai_task: Arc::new(Mutex::new(None)),
        skill_manager: Arc::new(Mutex::new(skill_manager)),
        event_bus: server::EventBus::default(),
        latest_selection: Arc::new(Mutex::new(None)),
        auth_token: Arc::new(auth_token),
    };

    let host = std::env::var("OMNILAUNCHER_SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("OMNILAUNCHER_SERVER_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(1422);

    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.block_on(async move {
        // In backend-only mode the server owns local scheduled jobs.
        omnilauncher_lib::plugins::scheduler::migrate_inline_commands_to_files();
        omnilauncher_lib::plugins::scheduler::start_scheduler();

        // ── Conditionally start the A2A server alongside the main API server ──
        if settings.a2a_enabled {
            let mut a2a_settings = settings.clone();

            let a2a_token = match a2a_settings
                .a2a_token
                .as_ref()
                .filter(|t| !t.trim().is_empty())
            {
                Some(token) => {
                    log::info!("a2a: using existing token");
                    token.clone()
                }
                None => {
                    let new_token = server::generate_auth_token();
                    log::info!("a2a: generated new auth token (first enable)");
                    a2a_settings.a2a_token = Some(new_token.clone());
                    save_settings(&a2a_settings);
                    new_token
                }
            };

            let a2a_host = if a2a_settings.a2a_bind_lan {
                "0.0.0.0".to_string()
            } else {
                "127.0.0.1".to_string()
            };
            let a2a_port = a2a_settings.a2a_port;

            if a2a_settings.a2a_hub_auto_register {
                match omnilauncher_lib::a2a::hub_registration::register_with_hub(
                    &a2a_settings,
                    &a2a_token,
                )
                .await
                {
                    Ok(()) => log::info!("a2a: omni-agent-hub upstream registration complete"),
                    Err(err) => {
                        log::warn!("a2a: omni-agent-hub upstream registration failed: {err}")
                    }
                }
            }

            let a2a_state = omnilauncher_lib::a2a::server::A2aServerState {
                adapter: omnilauncher_lib::a2a::adapter::A2aAdapterState {
                    plugin_manager: state.plugin_manager.clone(),
                    ai_client: state.ai_client.clone(),
                    settings: state.settings.clone(),
                    conversation: state.conversation.clone(),
                    skill_manager: state.skill_manager.clone(),
                    task_registry: Arc::new(Mutex::new(
                        omnilauncher_lib::a2a::tasks::TaskRegistry::new(100),
                    )),
                },
                auth_token: Arc::new(a2a_token),
            };

            tokio::spawn(async move {
                omnilauncher_lib::a2a::server::spawn_a2a_server(a2a_state, a2a_host, a2a_port)
                    .await;
            });
        }

        // Bind first so we only record a PID file for a backend that actually
        // owns the port.
        let pid_file = cli::process::backend_pid_file();
        match server::bind_api_listener(host.as_str(), port).await {
            Ok(listener) => {
                let _ = cli::process::write_pid(&pid_file, std::process::id());
                server::serve_bound(listener, state).await;
                cli::process::clear_pid(&pid_file);
            }
            Err(error) => {
                log::error!("failed to bind server on {}:{}: {}", host, port, error);
            }
        }
    });
}
