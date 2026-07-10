use omnilauncher_lib::{
    ai::{client::AiClient, router::ConversationContext},
    create_plugin_manager_builtin_only, save_settings, server,
    settings::load_settings_with_overrides,
    SkillManager,
};
use simplelog::{ColorChoice, ConfigBuilder, LevelFilter, TermLogger, TerminalMode, WriteLogger};
use std::{fs, fs::OpenOptions, path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, RwLock};

/// Terminal CLI (`ol`): lifecycle/ops commands plus resource management.
/// See `cli/mod.rs`.
mod cli;

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
    // If invoked by a generated shell completion integration, answer the
    // callback and exit before any logging or stdout write. No-op otherwise.
    cli::completion::handle_completion_env();

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

/// Run the OmniLauncher A2A agent in the foreground. Historical `--server`
/// and `ol serve` entry points now expose only the A2A protocol surface; the
/// legacy REST control-plane on port 1422 is intentionally not started.
pub fn serve_backend(args: &[String]) {
    let mut settings = load_settings_with_overrides(args);
    settings.a2a_enabled = true;
    let ai_client = AiClient::from_settings(&settings);
    let mut skill_manager = SkillManager::new();
    skill_manager.load_all();

    let a2a_token = match settings.a2a_token.as_ref().filter(|t| !t.trim().is_empty()) {
        Some(token) => {
            log::info!("a2a: using existing token");
            token.clone()
        }
        None => {
            let new_token = server::generate_auth_token();
            log::info!("a2a: generated new auth token (first enable)");
            settings.a2a_token = Some(new_token.clone());
            save_settings(&settings);
            new_token
        }
    };

    let mut conversation = ConversationContext::default();
    let sid = omnilauncher_lib::db::conversation::current_session_id();
    conversation.session_id = sid;
    conversation.max_turns = settings.ai_max_tool_iterations;
    conversation.messages = omnilauncher_lib::db::conversation::load_recent_for_session(sid, 20);

    let plugin_manager = Arc::new(RwLock::new(create_plugin_manager_builtin_only()));
    let ai_client = Arc::new(RwLock::new(ai_client));
    let settings = Arc::new(RwLock::new(settings.clone()));
    let conversation = Arc::new(Mutex::new(conversation));
    let skill_manager = Arc::new(Mutex::new(skill_manager));

    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.block_on(async move {
        // The A2A agent owns local scheduled jobs and plugin execution.
        omnilauncher_lib::plugins::scheduler::migrate_inline_commands_to_files();
        omnilauncher_lib::plugins::scheduler::start_scheduler();

        let settings_snapshot = settings.read().await.clone();
        let a2a_host = if settings_snapshot.a2a_bind_lan {
            "0.0.0.0".to_string()
        } else {
            "127.0.0.1".to_string()
        };
        let a2a_port = settings_snapshot.a2a_port;

        let a2a_state = omnilauncher_lib::a2a::server::A2aServerState {
            adapter: omnilauncher_lib::a2a::adapter::A2aAdapterState {
                plugin_manager,
                ai_client,
                settings,
                conversation,
                skill_manager,
                task_registry: Arc::new(Mutex::new(
                    omnilauncher_lib::a2a::tasks::TaskRegistry::new(100),
                )),
            },
            auth_token: Arc::new(a2a_token),
        };

        let pid_file = cli::process::backend_pid_file();
        let _ = cli::process::write_pid(&pid_file, std::process::id());
        omnilauncher_lib::a2a::server::spawn_a2a_server(a2a_state, a2a_host, a2a_port).await;
        cli::process::clear_pid(&pid_file);
    });
}
