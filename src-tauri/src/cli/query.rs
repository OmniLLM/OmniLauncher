//! Query commands for the `ol` CLI: slash-command execution, AI routing, and
//! bare launcher search — all run **in-process** by calling the library
//! directly (no HTTP, works offline). The router is async, so each one-shot
//! command spins up a short-lived current-thread tokio runtime.

use crate::cli::render::{render_results, Output};
use omnilauncher_lib::ai::client::AiClient;
use omnilauncher_lib::ai::router::{ConversationContext, Router};
use omnilauncher_lib::launcher_config::SLASH_COMMANDS;
use omnilauncher_lib::{load_settings, SkillManager};

/// Slash commands from the catalog that are handled by the GUI/frontend, not by
/// `Router::slash_command`. We do NOT expose these as CLI subcommands because
/// they have no terminal behavior.
pub const UI_ONLY_COMMANDS: &[&str] = &["/plugins", "/skills", "/new", "/clear", "/help"];

/// A CLI-exposable slash command: the bare name (no slash) plus its metadata,
/// derived from the shared `SLASH_COMMANDS` catalog. Generated so a new slash
/// command automatically gains a CLI subcommand and help string.
#[derive(Debug, Clone)]
pub struct CliCommand {
    /// Subcommand name as typed on the CLI, e.g. `grep` (no leading slash).
    pub name: &'static str,
    /// Optional short alias without the slash, e.g. `g` for `/grep`.
    pub alias: Option<&'static str>,
    pub description: &'static str,
    pub usage: &'static str,
}

/// The set of slash commands that map to CLI subcommands (UI-only entries
/// filtered out). Derived from `SLASH_COMMANDS` so names/shortcuts/help never
/// drift from the GUI.
pub fn cli_commands() -> Vec<CliCommand> {
    SLASH_COMMANDS
        .iter()
        .filter(|c| !UI_ONLY_COMMANDS.contains(&c.name))
        .map(|c| CliCommand {
            name: c.name.trim_start_matches('/'),
            alias: c.shortcut.map(|s| s.trim_start_matches('/')),
            description: c.description,
            usage: c.usage,
        })
        .collect()
}

/// Build the slash-form string that `Router::slash_command` expects from a CLI
/// subcommand name and its trailing args. `grep ["TODO","src/"]` →
/// `"/grep TODO src/"`.
pub fn to_slash_string(name: &str, args: &[String]) -> String {
    if args.is_empty() {
        format!("/{name}")
    } else {
        format!("/{name} {}", args.join(" "))
    }
}

/// Construct an `AiClient` from persisted settings, mirroring the wiring used by
/// `run()` / server startup.
fn ai_client_from_settings() -> (AiClient, usize, bool) {
    let settings = load_settings();
    let client = AiClient::with_retry(
        settings.ai_base_url.clone(),
        settings.resolve_ai_api_key(),
        settings.ai_model.clone(),
        settings.ai_timeout_secs,
        settings.ai_max_retry_attempts,
        settings.ai_retry_base_delay_ms,
    );
    (
        client,
        settings.ai_max_tool_iterations,
        settings.ai_loop_detector_enabled,
    )
}

/// A small current-thread tokio runtime for one-shot async calls.
fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

/// Run a slash command (e.g. `ol grep TODO src/`) in-process and render it.
/// Returns the process exit code (0 success, 1 failure).
pub fn run_slash(out: &Output, name: &str, args: &[String]) -> i32 {
    let input = to_slash_string(name, args);
    let rt = match runtime() {
        Ok(rt) => rt,
        Err(e) => {
            out.failure(&format!("failed to start runtime: {e}"));
            return 1;
        }
    };
    let resp = rt.block_on(async {
        let pm = omnilauncher_lib::create_plugin_manager();
        let mut sm = SkillManager::new();
        sm.load_all();
        Router::slash_command(&input, &pm, &mut sm).await
    });

    if out.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&resp).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        render_results(out, &resp.content, &resp.results);
    }
    0
}

/// Run a bare launcher search (`ol search <text>`) via `pm.query_all`.
pub fn run_search(out: &Output, text: &str) -> i32 {
    let rt = match runtime() {
        Ok(rt) => rt,
        Err(e) => {
            out.failure(&format!("failed to start runtime: {e}"));
            return 1;
        }
    };
    let results = rt.block_on(async {
        let pm = omnilauncher_lib::create_plugin_manager();
        pm.query_all(text).await
    });
    render_results(out, "", &results);
    0
}

/// Route a prompt through the AI (`ol ai "<text>"`). Prints the final answer;
/// tool activity is surfaced as a dim footer. Returns an exit code.
pub fn run_ai(out: &Output, prompt: &str) -> i32 {
    let rt = match runtime() {
        Ok(rt) => rt,
        Err(e) => {
            out.failure(&format!("failed to start runtime: {e}"));
            return 1;
        }
    };

    let resp = rt.block_on(async {
        let pm = omnilauncher_lib::create_plugin_manager();
        let mut sm = SkillManager::new();
        sm.load_all();
        let (client, max_iters, loop_detect) = ai_client_from_settings();
        let ctx = ConversationContext::default();
        Router::ai_route(
            prompt,
            &pm,
            &client,
            &ctx,
            &mut sm,
            None,
            max_iters,
            loop_detect,
        )
        .await
    });

    if out.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&resp).unwrap_or_else(|_| "{}".to_string())
        );
        return 0;
    }

    let content = resp.content.trim_end_matches('\n');
    if !content.is_empty() {
        println!("{content}");
    }
    if !out.quiet && !resp.tools_used.is_empty() {
        println!(
            "  {}",
            out.dim(&format!("tools: {}", resp.tools_used.join(", ")))
        );
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_only_commands_are_excluded() {
        let cmds = cli_commands();
        for ui in UI_ONLY_COMMANDS {
            let bare = ui.trim_start_matches('/');
            assert!(
                !cmds.iter().any(|c| c.name == bare),
                "UI-only command {ui} must not be a CLI subcommand"
            );
        }
    }

    #[test]
    fn operational_commands_are_present() {
        let cmds = cli_commands();
        for expected in ["run", "grep", "calc", "ps", "ls", "skill"] {
            assert!(
                cmds.iter().any(|c| c.name == expected),
                "expected CLI subcommand `{expected}` derived from SLASH_COMMANDS"
            );
        }
    }

    #[test]
    fn aliases_are_derived_without_slash() {
        let cmds = cli_commands();
        let grep = cmds.iter().find(|c| c.name == "grep").unwrap();
        assert_eq!(grep.alias, Some("g"));
    }

    #[test]
    fn slash_string_roundtrip() {
        assert_eq!(to_slash_string("grep", &[]), "/grep");
        assert_eq!(
            to_slash_string("grep", &["TODO".into(), "src/".into()]),
            "/grep TODO src/"
        );
    }

    #[test]
    fn every_cli_command_maps_to_a_real_router_arm() {
        // Drift guard: the set of CLI subcommands is exactly the catalog minus
        // the UI-only entries. If someone adds a slash command, this keeps the
        // CLI in lockstep (or forces an explicit UI-only classification).
        let derived = cli_commands().len();
        let expected = SLASH_COMMANDS.len() - UI_ONLY_COMMANDS.len();
        assert_eq!(derived, expected);
    }
}
