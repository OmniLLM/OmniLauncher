//! Interactive REPL for the `ol` CLI.
//!
//! Entered via `ol` with no args on a TTY, or explicitly with `ol repl`.
//! Grammar:
//!   omni> ps                    bare word  → query / slash command
//!   omni> /grep TODO src/       explicit slash also accepted
//!   omni> ? explain lifetimes   AI mode (also `ai …`)
//!   omni> :start   :status      ops commands use a ':' prefix
//!   omni> help                  command list (from SLASH_COMMANDS)
//!   omni> exit  /  Ctrl-D       quit
//!
//! History persists to `~/.omnilauncher/repl_history`; command names from the
//! shared `SLASH_COMMANDS` catalog drive tab-completion.

use crate::cli::ops;
use crate::cli::process;
use crate::cli::query;
use crate::cli::render::Output;
use omnilauncher_lib::launcher_config::SLASH_COMMANDS;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Editor, Helper};

/// rustyline helper providing tab-completion over slash-command names and the
/// `:`-prefixed ops verbs. The hint/highlight/validate traits use their default
/// (no-op) behavior; only completion is customized.
struct ReplHelper {
    candidates: Vec<String>,
}

impl Highlighter for ReplHelper {}
impl Validator for ReplHelper {}
impl Hinter for ReplHelper {
    type Hint = String;
}
impl Helper for ReplHelper {}

impl ReplHelper {
    fn new() -> Self {
        let mut candidates: Vec<String> = Vec::new();
        // Bare + slash forms of every catalog command that the CLI actually
        // exposes (skips UI-only navigation and shell-duplicating utilities like
        // grep/cat/ls — the REPL user already has a real shell for those).
        for c in SLASH_COMMANDS {
            if !query::is_cli_exposed(c.name) {
                continue;
            }
            candidates.push(c.name.to_string()); // "/grep"
            candidates.push(c.name.trim_start_matches('/').to_string()); // "grep"
            if let Some(sc) = c.shortcut {
                candidates.push(sc.to_string());
            }
        }
        // Ops verbs.
        for op in [":start", ":stop", ":restart", ":status", ":logs", ":doctor"] {
            candidates.push(op.to_string());
        }
        for kw in ["help", "exit", "quit", "ai ", "? "] {
            candidates.push(kw.to_string());
        }
        candidates.sort();
        candidates.dedup();
        ReplHelper { candidates }
    }
}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        // Complete only the first token (the command word).
        let prefix = &line[..pos];
        if prefix.contains(' ') {
            return Ok((pos, Vec::new()));
        }
        let matches: Vec<Pair> = self
            .candidates
            .iter()
            .filter(|c| c.starts_with(prefix))
            .map(|c| Pair {
                display: c.clone(),
                replacement: c.clone(),
            })
            .collect();
        Ok((0, matches))
    }
}

/// Run the interactive REPL. Returns a process exit code.
pub fn run(out: &Output) -> i32 {
    let mut rl: Editor<ReplHelper, rustyline::history::DefaultHistory> = match Editor::new() {
        Ok(e) => e,
        Err(e) => {
            out.failure(&format!("failed to start REPL: {e}"));
            return 1;
        }
    };
    rl.set_helper(Some(ReplHelper::new()));

    let history = process::repl_history_file();
    let _ = rl.load_history(&history);

    banner(out);

    loop {
        match rl.readline("omni> ") {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(input);
                if !dispatch_line(out, input) {
                    break; // exit requested
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl-C: clear the current line, keep going.
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl-D: quit.
                break;
            }
            Err(e) => {
                out.failure(&format!("input error: {e}"));
                break;
            }
        }
    }

    let _ = rl.save_history(&history);
    0
}

fn banner(out: &Output) {
    if out.quiet {
        return;
    }
    println!("{}", out.cyan("OmniLauncher REPL"));
    println!(
        "{}",
        out.dim(
            "type a query, /command, `? question` for AI, `:status` for ops, `help`, or `exit`"
        )
    );
}

/// Handle one line of REPL input. Returns `false` when the user asked to exit.
fn dispatch_line(out: &Output, input: &str) -> bool {
    // Quit words.
    if input == "exit" || input == "quit" {
        return false;
    }
    // Help.
    if input == "help" {
        print_help(out);
        return true;
    }
    // Ops verbs: ':' prefix disambiguates from query terms like "status".
    if let Some(op) = input.strip_prefix(':') {
        run_op(out, op.trim());
        return true;
    }
    // AI mode: `? ...` or `ai ...`.
    if let Some(rest) = input.strip_prefix('?') {
        query::run_ai(out, rest.trim());
        return true;
    }
    if let Some(rest) = input.strip_prefix("ai ") {
        query::run_ai(out, rest.trim());
        return true;
    }
    // Explicit slash command.
    if let Some(rest) = input.strip_prefix('/') {
        let mut parts = rest.splitn(2, ' ');
        let name = parts.next().unwrap_or("");
        let args = split_args(parts.next().unwrap_or(""));
        // Shell-duplicating commands (grep/cat/ls/git/env/run) are intentionally
        // not exposed in the REPL — the user is already at a shell. Redirect
        // rather than silently running a second-class reimplementation.
        if is_shell_duplicate(name) {
            note_shell_duplicate(out, name);
            return true;
        }
        query::run_slash(out, name, &args);
        return true;
    }
    // Bare word → treat the first token as a slash command if it matches the
    // catalog, otherwise run a launcher search over the whole line.
    let mut parts = input.splitn(2, ' ');
    let first = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("");
    if is_known_command(first) {
        query::run_slash(out, first, &split_args(rest));
    } else {
        query::run_search(out, input);
    }
    true
}

/// Run a `:`-prefixed ops verb from within the REPL.
fn run_op(out: &Output, op: &str) {
    match op {
        "start" => {
            ops::start(out, false);
        }
        "stop" => {
            ops::stop(out);
        }
        "restart" => {
            ops::restart(out, false);
        }
        "status" => {
            ops::status(out);
        }
        "logs" => {
            ops::logs(out, 50, false);
        }
        "doctor" => {
            ops::doctor(out);
        }
        other => out.failure(&format!(
            "unknown op ':{other}' (try :status :start :stop :logs)"
        )),
    }
}

/// Whether `token` (bare, no slash) is a known operational slash command.
fn is_known_command(token: &str) -> bool {
    query::cli_commands()
        .iter()
        .any(|c| c.name == token || c.alias == Some(token))
}

/// Whether `name` (with or without a leading slash) is a shell-duplicating
/// command that the REPL intentionally does not expose.
fn is_shell_duplicate(name: &str) -> bool {
    let slashed = if name.starts_with('/') {
        name.to_string()
    } else {
        format!("/{name}")
    };
    query::SHELL_DUPLICATE_COMMANDS.contains(&slashed.as_str())
}

/// Tell the user a shell-duplicating command was dropped from the REPL and point
/// them at the real shell equivalent they already have.
fn note_shell_duplicate(out: &Output, name: &str) {
    let bare = name.trim_start_matches('/');
    out.failure(&format!(
        "/{bare} isn't available in the REPL — you're already at a shell; run `{bare}` directly"
    ));
}

/// Split a raw argument tail into whitespace-separated args. The router re-joins
/// with spaces, so this only needs to be simple.
fn split_args(rest: &str) -> Vec<String> {
    rest.split_whitespace().map(|s| s.to_string()).collect()
}

fn print_help(out: &Output) {
    println!("{}", out.cyan("Commands"));
    for c in query::cli_commands() {
        let alias = c.alias.map(|a| format!(" (/{a})")).unwrap_or_default();
        println!(
            "  {}{}   {}",
            c.usage,
            out.dim(&alias),
            out.dim(c.description)
        );
    }
    println!("{}", out.cyan("Ops"));
    for (verb, desc) in [
        (":start", "start the backend"),
        (":stop", "stop the backend"),
        (":restart", "restart the backend"),
        (":status", "backend status"),
        (":logs", "show recent logs"),
        (":doctor", "run diagnostics"),
    ] {
        println!("  {verb}   {}", out.dim(desc));
    }
    println!("{}", out.cyan("Other"));
    println!("  {}   {}", "? <question>", out.dim("ask the AI"));
    println!("  {}   {}", "exit", out.dim("quit (or Ctrl-D)"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_args_splits_on_whitespace() {
        assert_eq!(split_args("TODO src/"), vec!["TODO", "src/"]);
        assert_eq!(split_args(""), Vec::<String>::new());
    }

    #[test]
    fn known_command_recognizes_names_and_aliases() {
        assert!(is_known_command("calc"));
        assert!(is_known_command("c")); // alias of /calc
        assert!(is_known_command("find"));
        assert!(is_known_command("f")); // alias of /find
        assert!(!is_known_command("definitely-not-a-command"));
        // UI-only commands are not known CLI commands.
        assert!(!is_known_command("plugins"));
        // Shell-duplicating commands are no longer known CLI commands.
        assert!(!is_known_command("grep"));
        assert!(!is_known_command("g")); // alias of /grep
        assert!(!is_known_command("cat"));
    }

    #[test]
    fn shell_duplicate_detection_handles_slash_and_bare() {
        assert!(is_shell_duplicate("grep"));
        assert!(is_shell_duplicate("/grep"));
        assert!(is_shell_duplicate("cat"));
        assert!(!is_shell_duplicate("calc"));
        assert!(!is_shell_duplicate("/find"));
    }

    #[test]
    fn helper_candidates_include_ops_and_launcher_commands() {
        let h = ReplHelper::new();
        assert!(h.candidates.iter().any(|c| c == ":status"));
        // Launcher-native commands are offered.
        assert!(h.candidates.iter().any(|c| c == "calc"));
        assert!(h.candidates.iter().any(|c| c == "/calc"));
        assert!(h.candidates.iter().any(|c| c == "find"));
        // UI-only excluded.
        assert!(!h.candidates.iter().any(|c| c == "/plugins"));
        // Shell-duplicating commands excluded from completion.
        assert!(!h.candidates.iter().any(|c| c == "grep"));
        assert!(!h.candidates.iter().any(|c| c == "/grep"));
        assert!(!h.candidates.iter().any(|c| c == "cat"));
    }
}
