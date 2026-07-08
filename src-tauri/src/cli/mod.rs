//! `ol` CLI entry: clap command definitions, global flags, argv[0] multi-call
//! dispatch, and back-compat with the historical `--server` / `--debug` flags.
//!
//! Dispatch model (busybox-style multi-call):
//!   - invoked as `omnilauncher` with no command → GUI (unchanged desktop launch)
//!   - invoked as `ol` with no command → REPL when stdin is a TTY, else help
//!   - both names accept every subcommand and global flag
//!
//! The query subcommands (`grep`, `calc`, `ps`, …) are generated from
//! `launcher_config::SLASH_COMMANDS` so they never drift from the GUI. Because
//! that set is data-driven, we parse with a small hand-rolled argv scan rather
//! than a fully static clap derive tree — clap is still used for `--help` text
//! and the fixed ops subcommands.

pub mod ops;
pub mod process;
pub mod query;
pub mod render;
pub mod repl;

use render::Output;
use std::io::IsTerminal;

/// Global flags recognized before/around any subcommand.
#[derive(Debug, Default, Clone)]
pub struct Globals {
    pub json: bool,
    pub no_color: bool,
    pub quiet: bool,
    pub debug: bool,
}

/// The basename `ol` triggers REPL-by-default; anything else (i.e.
/// `omnilauncher`) triggers GUI-by-default.
fn invoked_as_ol(argv0: &str) -> bool {
    std::path::Path::new(argv0)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|name| name == "ol")
        .unwrap_or(false)
}

/// Parse global flags out of an argv tail, returning the flags plus the
/// remaining (non-global) tokens in order. Unknown `--flags` are left in place
/// so subcommand handlers (or clap) can deal with them.
fn extract_globals(args: &[String]) -> (Globals, Vec<String>) {
    let mut g = Globals::default();
    let mut rest = Vec::with_capacity(args.len());
    for a in args {
        match a.as_str() {
            "--json" => g.json = true,
            "--no-color" => g.no_color = true,
            "-q" | "--quiet" => g.quiet = true,
            "--debug" => g.debug = true,
            _ => rest.push(a.clone()),
        }
    }
    (g, rest)
}

/// Resolve the `Output` presentation from parsed globals.
fn output_from(globals: &Globals) -> Output {
    Output::resolve(globals.json, globals.no_color, globals.quiet)
}

/// Whether the raw argv requests the legacy backend mode (`--server`).
/// Preserved verbatim so existing launchers / scripts keep working.
pub fn wants_legacy_server(args: &[String]) -> bool {
    args.iter().any(|a| a == "--server")
}

/// Whether the raw argv enables debug file logging (`--debug`), anywhere.
pub fn wants_debug(args: &[String]) -> bool {
    args.iter().any(|a| a == "--debug")
}

/// Whether this invocation is a short-lived, foreground CLI command (e.g.
/// `calc`, `ps`, `status`) as opposed to a long-running mode (`serve`, `gui`,
/// legacy `--server`) or the bare-GUI default. Used by `main` to keep INFO log
/// chatter off the terminal for one-shot commands. The REPL (`ol` with no
/// command on a TTY, or `repl`) also counts as foreground.
pub fn is_foreground_cli(argv0: &str, rest: &[String]) -> bool {
    if wants_legacy_server(rest) {
        return false; // serve is long-running
    }
    let (_, tokens) = extract_globals(rest);
    match tokens.first().map(|s| s.as_str()) {
        // Long-running / windowed modes: keep normal logging.
        Some("serve") | Some("gui") => false,
        // Any other explicit subcommand is a foreground CLI command.
        Some(_) => true,
        // No subcommand: `ol` → REPL (foreground); `omnilauncher` → GUI.
        None => invoked_as_ol(argv0),
    }
}

/// Outcome of dispatch, so `main()` can decide whether to fall through to the
/// GUI/serve paths (which need to run outside this function to keep their exact
/// runtime setup) or exit with a code.
pub enum Dispatch {
    /// Launch the desktop GUI (foreground). `main` calls `run()`.
    Gui,
    /// Run the backend server in the foreground. `main` calls `serve_backend`.
    Serve,
    /// A CLI command handled fully in-process; exit with this code.
    Handled(i32),
}

/// Top-level dispatch. `argv0` is `args[0]` (the program name); `rest` is
/// everything after it. Returns a `Dispatch` telling `main()` what to do.
pub fn dispatch(argv0: &str, rest: &[String]) -> Dispatch {
    let (globals, tokens) = extract_globals(rest);
    let out = output_from(&globals);

    // Back-compat: `--server` anywhere → serve, regardless of subcommand.
    if wants_legacy_server(rest) {
        return Dispatch::Serve;
    }

    // No subcommand: default action depends on argv[0].
    let Some(command) = tokens.first().cloned() else {
        if invoked_as_ol(argv0) {
            if std::io::stdin().is_terminal() {
                return Dispatch::Handled(repl::run(&out));
            }
            print_help(&out);
            return Dispatch::Handled(0);
        }
        // `omnilauncher` with no args → GUI (unchanged).
        return Dispatch::Gui;
    };

    let cmd_args: Vec<String> = tokens[1..].to_vec();
    dispatch_command(&out, &globals, &command, &cmd_args)
}

/// Dispatch a resolved subcommand name + its args.
fn dispatch_command(out: &Output, globals: &Globals, command: &str, args: &[String]) -> Dispatch {
    match command {
        // ── Lifecycle / ops ──────────────────────────────────────────────
        "serve" => Dispatch::Serve,
        "gui" => {
            let detached = args.iter().any(|a| a == "--detached");
            Dispatch::Handled(ops::gui(out, detached, globals.debug))
        }
        "start" => Dispatch::Handled(ops::start(out, globals.debug)),
        "stop" => {
            // `stop` (backend, default), `stop --gui` (detached shell), or
            // `stop --all` (both). Keeps the common no-flag case unchanged.
            if args.iter().any(|a| a == "--all") {
                Dispatch::Handled(ops::stop_all(out))
            } else if args.iter().any(|a| a == "--gui") {
                Dispatch::Handled(ops::stop_gui(out))
            } else {
                Dispatch::Handled(ops::stop(out))
            }
        }
        "restart" => Dispatch::Handled(ops::restart(out, globals.debug)),
        "status" => Dispatch::Handled(ops::status(out)),
        "health" => Dispatch::Handled(ops::health(out)),
        "logs" => {
            let follow = args.iter().any(|a| a == "-f" || a == "--follow");
            let lines = arg_value(args, "-n")
                .or_else(|| arg_value(args, "--lines"))
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(50);
            Dispatch::Handled(ops::logs(out, lines, follow))
        }
        "doctor" => Dispatch::Handled(ops::doctor(out)),
        "repl" => Dispatch::Handled(repl::run(out)),

        // ── Query surface ────────────────────────────────────────────────
        "ai" => {
            let prompt = args.join(" ");
            if prompt.trim().is_empty() {
                out.failure("usage: ol ai <text>");
                return Dispatch::Handled(2);
            }
            Dispatch::Handled(query::run_ai(out, &prompt))
        }
        "search" => {
            let text = args.join(" ");
            if text.trim().is_empty() {
                out.failure("usage: ol search <text>");
                return Dispatch::Handled(2);
            }
            Dispatch::Handled(query::run_search(out, &text))
        }

        // ── Help / version ───────────────────────────────────────────────
        "help" | "--help" | "-h" => {
            print_help(out);
            Dispatch::Handled(0)
        }
        "version" | "--version" | "-V" => {
            println!("ol (OmniLauncher) v{}", env!("CARGO_PKG_VERSION"));
            Dispatch::Handled(0)
        }

        // ── Generated query subcommands from SLASH_COMMANDS ──────────────
        other => {
            // Resolve a bare name or short alias to a catalog command.
            if let Some(c) = query::cli_commands()
                .into_iter()
                .find(|c| c.name == other || c.alias == Some(other))
            {
                let _ = globals; // presentation already baked into `out`
                Dispatch::Handled(query::run_slash(out, c.name, args))
            } else if query::SHELL_DUPLICATE_COMMANDS.contains(&format!("/{other}").as_str()) {
                // Shell-duplicating commands (grep/cat/ls/git/env/run) are not
                // exposed on the CLI — the user already has a real shell.
                out.failure(&format!(
                    "'{other}' isn't an ol command — you're at a shell already; run `{other}` directly"
                ));
                Dispatch::Handled(2)
            } else {
                out.failure(&format!(
                    "unknown command '{other}' — run `ol help` for the command list"
                ));
                Dispatch::Handled(2)
            }
        }
    }
}

/// Extract the value following a `--flag value` pair from an arg list.
fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Print the top-level help / command list.
pub fn print_help(out: &Output) {
    println!("{}", out.cyan("ol — OmniLauncher CLI"));
    println!();
    println!("{}", out.dim("USAGE"));
    println!("  ol [FLAGS] [COMMAND] [ARGS...]");
    println!();
    println!("{}", out.dim("GLOBAL FLAGS"));
    println!("  --json        machine-readable JSON output");
    println!("  --no-color    disable ANSI color (also NO_COLOR / non-TTY)");
    println!("  -q, --quiet   errors only");
    println!("  --debug       enable file logging (~/.omnilauncher/omnilauncher.log)");
    println!();
    println!("{}", out.dim("LIFECYCLE"));
    for (name, desc) in [
        ("serve", "run the backend API server (foreground)"),
        ("gui", "launch the desktop shell (--detached to background)"),
        ("start", "start the backend detached and wait for health"),
        ("stop", "stop the backend (--gui shell, --all both)"),
        ("restart", "stop then start"),
        ("status", "health / process / port / binary view"),
        ("health", "probe the backend /health endpoint (exit 0 if ok)"),
        ("logs", "print/tail the log file (-f follow, -n N)"),
        ("doctor", "diagnostics: config, token, AI, deps"),
        (
            "repl",
            "interactive prompt (default when run as `ol` on a TTY)",
        ),
    ] {
        println!("  {:<9} {}", name, out.dim(desc));
    }
    println!();
    println!("{}", out.dim("QUERY"));
    println!("  {:<9} {}", "ai", out.dim("route text through the AI"));
    println!("  {:<9} {}", "search", out.dim("bare launcher search"));
    for c in query::cli_commands() {
        println!("  {:<9} {}", c.name, out.dim(c.description));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn ol_basename_detected_across_paths() {
        assert!(invoked_as_ol("ol"));
        assert!(invoked_as_ol("/usr/local/bin/ol"));
        assert!(invoked_as_ol("/home/x/.local/bin/ol"));
        assert!(!invoked_as_ol("omnilauncher"));
        assert!(!invoked_as_ol("/opt/omnilauncher/omnilauncher"));
    }

    #[test]
    fn globals_are_extracted_and_stripped() {
        let (g, rest) = extract_globals(&s(&["--json", "grep", "TODO", "--quiet"]));
        assert!(g.json);
        assert!(g.quiet);
        assert!(!g.no_color);
        assert_eq!(rest, s(&["grep", "TODO"]));
    }

    #[test]
    fn legacy_server_flag_forces_serve() {
        assert!(wants_legacy_server(&s(&["--server"])));
        assert!(wants_legacy_server(&s(&["--debug", "--server"])));
        assert!(!wants_legacy_server(&s(&["serve"])));
    }

    #[test]
    fn debug_flag_detected_anywhere() {
        assert!(wants_debug(&s(&["grep", "--debug", "x"])));
        assert!(!wants_debug(&s(&["grep", "x"])));
    }

    #[test]
    fn omnilauncher_no_args_is_gui() {
        match dispatch("omnilauncher", &[]) {
            Dispatch::Gui => {}
            _ => panic!("omnilauncher with no args must default to GUI"),
        }
    }

    #[test]
    fn server_flag_routes_to_serve_for_both_names() {
        assert!(matches!(
            dispatch("omnilauncher", &s(&["--server"])),
            Dispatch::Serve
        ));
        assert!(matches!(dispatch("ol", &s(&["--server"])), Dispatch::Serve));
    }

    #[test]
    fn serve_subcommand_routes_to_serve() {
        assert!(matches!(dispatch("ol", &s(&["serve"])), Dispatch::Serve));
    }

    #[test]
    fn unknown_command_is_usage_error() {
        match dispatch("ol", &s(&["totally-bogus"])) {
            Dispatch::Handled(code) => assert_eq!(code, 2),
            _ => panic!("unknown command should be a handled usage error"),
        }
    }

    #[test]
    fn ai_without_text_is_usage_error() {
        match dispatch("ol", &s(&["ai"])) {
            Dispatch::Handled(code) => assert_eq!(code, 2),
            _ => panic!("`ai` with no text should be usage error 2"),
        }
    }

    #[test]
    fn version_is_handled_success() {
        match dispatch("ol", &s(&["version"])) {
            Dispatch::Handled(code) => assert_eq!(code, 0),
            _ => panic!("version should be handled with exit 0"),
        }
    }

    #[test]
    fn stop_variants_are_handled() {
        // All three stop forms route to a handled outcome (exit code depends on
        // whether something is running, which is environment-specific; we only
        // assert routing here, not the code).
        for variant in [&["stop"][..], &["stop", "--gui"][..], &["stop", "--all"][..]] {
            match dispatch("ol", &s(variant)) {
                Dispatch::Handled(_) => {}
                _ => panic!("`{variant:?}` should be handled"),
            }
        }
    }

    #[test]
    fn shell_duplicate_command_is_rejected_with_redirect() {
        // `ol grep ...` is no longer a command — it should be a usage error (2),
        // not silently run a reimplementation.
        match dispatch("ol", &s(&["grep", "TODO"])) {
            Dispatch::Handled(code) => assert_eq!(code, 2),
            _ => panic!("shell-duplicating command should be a handled usage error"),
        }
    }

    #[test]
    fn foreground_cli_classification() {
        // One-shot commands are foreground (quiet logging).
        assert!(is_foreground_cli("ol", &s(&["calc", "2+2"])));
        assert!(is_foreground_cli("omnilauncher", &s(&["status"])));
        assert!(is_foreground_cli("ol", &s(&["--json", "ps"])));
        // Long-running / windowed modes are not.
        assert!(!is_foreground_cli("ol", &s(&["serve"])));
        assert!(!is_foreground_cli("ol", &s(&["gui"])));
        assert!(!is_foreground_cli("omnilauncher", &s(&["--server"])));
        // Bare invocation: `ol` → REPL (foreground); `omnilauncher` → GUI.
        assert!(is_foreground_cli("ol", &[]));
        assert!(!is_foreground_cli("omnilauncher", &[]));
    }
}
