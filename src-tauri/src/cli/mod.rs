//! `ol` CLI entry: global flags, argv[0] multi-call dispatch, and back-compat
//! with the historical `--server` / `--debug` flags.
//!
//! Dispatch model (busybox-style multi-call):
//!   - invoked as `omnilauncher` with no command → GUI (unchanged desktop launch)
//!   - invoked as `ol` with no command → print the ops help, then exit
//!   - both names accept every subcommand and global flag
//!
//! `ol` is an ops-only controller for the omnilauncher binary: it exposes the
//! lifecycle verbs (serve, gui, start, stop, restart, status, health, logs,
//! doctor) listed in `OPS_COMMANDS`, plus `help`/`version`. It deliberately does
//! NOT expose the GUI launcher's query palette (calc, web, ps, …) — in a
//! terminal the user already has a shell. We parse with a small hand-rolled argv
//! scan rather than a clap derive tree.

pub mod ops;
pub mod process;
pub mod render;

use render::Output;

/// Global flags recognized before/around any subcommand.
#[derive(Debug, Default, Clone)]
pub struct Globals {
    pub json: bool,
    pub no_color: bool,
    pub quiet: bool,
    pub debug: bool,
}

/// The basename `ol` triggers help-by-default; anything else (i.e.
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
/// `status`, `health`, `doctor`) as opposed to a long-running mode (`serve`, `gui`,
/// legacy `--server`) or the bare-GUI default. Used by `main` to keep INFO log
/// chatter off the terminal for one-shot commands. Bare `ol` (no subcommand,
/// which now prints help) also counts as foreground.
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
        // No subcommand: `ol` → help (foreground); `omnilauncher` → GUI.
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
            // Bare `ol` (TTY or not) prints the ops help and exits. There is no
            // interactive REPL: `ol` only operates the omnilauncher binary.
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

        // ── Help / version ───────────────────────────────────────────────
        "help" | "--help" | "-h" => {
            print_help(out);
            Dispatch::Handled(0)
        }
        "version" | "--version" | "-V" => {
            println!("ol (OmniLauncher) v{}", env!("CARGO_PKG_VERSION"));
            Dispatch::Handled(0)
        }

        // ── Unknown ──────────────────────────────────────────────────────
        other => {
            out.failure(&format!(
                "unknown command '{other}' — run `ol help` for the command list"
            ));
            Dispatch::Handled(2)
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

/// One operational verb: the name as typed on the CLI plus its one-line help.
/// `OPS_COMMANDS` is the single source of truth for the advertised ops verbs and
/// their help text — `print_help` renders straight from it, so the command list
/// and its descriptions cannot drift. (Dispatch keeps explicit per-verb `match`
/// arms by design, since each verb parses its own flags.)
struct OpsCommand {
    name: &'static str,
    desc: &'static str,
}

/// The lifecycle/ops verbs `ol` exposes. `help`/`version` are handled separately
/// (they are not lifecycle verbs). Adding a verb here + a `match` arm in
/// `dispatch_command` is all that's needed to surface a new ops command.
const OPS_COMMANDS: &[OpsCommand] = &[
    OpsCommand { name: "serve",   desc: "run the backend API server (foreground)" },
    OpsCommand { name: "gui",     desc: "launch the desktop shell (--detached to background)" },
    OpsCommand { name: "start",   desc: "start the backend detached and wait for health" },
    OpsCommand { name: "stop",    desc: "stop the backend (--gui shell, --all both)" },
    OpsCommand { name: "restart", desc: "stop then start" },
    OpsCommand { name: "status",  desc: "health / process / port / binary view" },
    OpsCommand { name: "health",  desc: "probe the backend /health endpoint (exit 0 if ok)" },
    OpsCommand { name: "logs",    desc: "print/tail the log file (-f follow, -n N)" },
    OpsCommand { name: "doctor",  desc: "diagnostics: config, token, AI, deps" },
];

/// Print the top-level help / command list to stdout.
pub fn print_help(out: &Output) {
    print!("{}", render_help_to_string(out));
}

/// Render the help text to a string (so it can be asserted in tests).
fn render_help_to_string(out: &Output) -> String {
    let mut s = String::new();
    s.push_str(&format!("{}\n", out.cyan("ol — OmniLauncher CLI")));
    s.push('\n');
    s.push_str(&format!("{}\n", out.dim("USAGE")));
    s.push_str("  ol [FLAGS] [COMMAND] [ARGS...]\n");
    s.push('\n');
    s.push_str(&format!("{}\n", out.dim("GLOBAL FLAGS")));
    s.push_str("  --json        machine-readable JSON output\n");
    s.push_str("  --no-color    disable ANSI color (also NO_COLOR / non-TTY)\n");
    s.push_str("  -q, --quiet   errors only\n");
    s.push_str("  --debug       enable file logging (~/.omnilauncher/omnilauncher.log)\n");
    s.push('\n');
    s.push_str(&format!("{}\n", out.dim("COMMANDS")));
    for c in OPS_COMMANDS {
        s.push_str(&format!("  {:<9} {}\n", c.name, out.dim(c.desc)));
    }
    s.push_str(&format!("  {:<9} {}\n", "help", out.dim("show this help")));
    s.push_str(&format!("  {:<9} {}\n", "version", out.dim("print version")));
    s
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
    fn query_commands_are_unknown_now() {
        // The launcher-query surface (calc/web/…), ai, search, and repl are no
        // longer CLI commands. Each must be a handled usage error (exit 2), not
        // routed anywhere.
        for variant in [
            &["calc", "2+2"][..],
            &["ai", "hi"][..],
            &["search", "x"][..],
            &["repl"][..],
            &["grep", "TODO"][..],
        ] {
            match dispatch("ol", &s(variant)) {
                Dispatch::Handled(code) => assert_eq!(code, 2, "{variant:?} should be usage error 2"),
                _ => panic!("`{variant:?}` should be a handled usage error"),
            }
        }
    }

    #[test]
    fn bare_ol_prints_help_not_repl() {
        // Bare `ol` no longer launches a REPL; it prints ops help and exits 0.
        match dispatch("ol", &[]) {
            Dispatch::Handled(code) => assert_eq!(code, 0),
            _ => panic!("bare `ol` should print help and exit 0"),
        }
    }

    #[test]
    fn help_lists_every_ops_command() {
        // Help must show every ops verb AND its description, plus help/version,
        // and must NOT show any removed launcher-query verb or the old QUERY
        // section. Asserting the description too (not just the name) catches desc
        // drift, without coupling the test to exact column padding.
        let out = Output::resolve(false, true, false); // no-color for stable matching
        let help = render_help_to_string(&out);
        for c in OPS_COMMANDS {
            assert!(help.contains(c.name), "help missing ops verb '{}'", c.name);
            assert!(help.contains(c.desc), "help missing description for '{}'", c.name);
        }
        assert!(help.contains("help"), "help missing 'help' entry");
        assert!(help.contains("version"), "help missing 'version' entry");
        // Removed surface must not reappear.
        assert!(!help.contains("QUERY"), "help still has a QUERY section");
        for removed in ["calc", "search", "repl"] {
            assert!(
                !help.contains(removed),
                "help unexpectedly lists removed command '{removed}'"
            );
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
    fn foreground_cli_classification() {
        // One-shot commands are foreground (quiet logging).
        assert!(is_foreground_cli("ol", &s(&["status"])));
        assert!(is_foreground_cli("omnilauncher", &s(&["status"])));
        assert!(is_foreground_cli("ol", &s(&["--json", "status"])));
        // Long-running / windowed modes are not.
        assert!(!is_foreground_cli("ol", &s(&["serve"])));
        assert!(!is_foreground_cli("ol", &s(&["gui"])));
        assert!(!is_foreground_cli("omnilauncher", &s(&["--server"])));
        // Bare invocation: `ol` → help (foreground); `omnilauncher` → GUI.
        assert!(is_foreground_cli("ol", &[]));
        assert!(!is_foreground_cli("omnilauncher", &[]));
    }
}
