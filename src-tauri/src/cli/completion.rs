//! Shell completion for the `ol` / `omnilauncher` CLI.
//!
//! The production dispatcher (`cli::mod`) stays the runtime authority and keeps
//! its hand-rolled argv scan. This module owns a *separate* Clap builder schema
//! that mirrors the dispatch tree purely so `clap_complete` can generate shell
//! integrations and answer dynamic completion callbacks. It never parses normal
//! runtime commands.
//!
//! Design: `docs/superpowers/specs/2026-07-10-shell-completion-design.md`.

use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};
use clap_complete::env::EnvCompleter;
use clap_complete::env::Shells;

use clap::{Arg, ArgAction, Command, ValueHint};

use std::ffi::OsStr;
use std::path::Path;

use crate::cli::manage::SETTINGS_FIELDS;
use omnilauncher_lib::AppSettings;

/// Fixed value set for the `completion` shell argument.
const SHELL_VALUES: [&str; 5] = ["bash", "zsh", "fish", "powershell", "elvish"];
/// Fixed value set for provider `--kind`.
const PROVIDER_KINDS: [&str; 3] = ["custom", "github-copilot", "azure-foundry"];
/// Fixed value set for `plugins install-runtime`.
const RUNTIME_IDS: [&str; 3] = ["python", "node", "dotnet"];

/// One of the five shells we generate completion integrations for.
// `PowerShell` "ends with the enum name" (Shell), but these are the standard,
// user-facing shell names; renaming would obscure them.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shell {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Elvish,
}

impl Shell {
    /// Every supported shell, in the order advertised to users.
    #[cfg(test)]
    pub(crate) const ALL: &'static [Shell] = &[
        Shell::Bash,
        Shell::Zsh,
        Shell::Fish,
        Shell::PowerShell,
        Shell::Elvish,
    ];

    /// Parse a user-supplied shell name. Returns `None` for anything we do not
    /// generate integrations for (e.g. `nu`).
    pub(crate) fn parse(name: &str) -> Option<Shell> {
        match name {
            "bash" => Some(Shell::Bash),
            "zsh" => Some(Shell::Zsh),
            "fish" => Some(Shell::Fish),
            "powershell" => Some(Shell::PowerShell),
            "elvish" => Some(Shell::Elvish),
            _ => None,
        }
    }

    /// Canonical lowercase name, matching the accepted CLI spelling.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Shell::Bash => "bash",
            Shell::Zsh => "zsh",
            Shell::Fish => "fish",
            Shell::PowerShell => "powershell",
            Shell::Elvish => "elvish",
        }
    }

    /// Resolve the matching `clap_complete` shell integration writer.
    ///
    /// We look the completer up by name in the crate's built-in registry rather
    /// than referencing private shell structs, so the mapping tracks the locked
    /// crate exactly.
    pub(crate) fn env_completer(self) -> &'static dyn EnvCompleter {
        // `powershell` is the name the built-in PowerShell completer answers to.
        // Access the static builtins slice (`.0`) directly so the returned
        // reference is `'static` rather than borrowing a temporary `Shells`.
        let name = self.name();
        Shells::builtins()
            .0
            .iter()
            .copied()
            .find(|completer| completer.is(name))
            .expect("clap_complete builtins must provide every supported shell")
    }
}

/// A positional resource-name argument (optional, one value), used for the many
/// `<name>` / `<id>` / `<repo-dir>` positionals in the tree.
fn resource_name_arg(id: &'static str) -> Arg {
    Arg::new(id).required(false).num_args(0..=1)
}

/// A named `--flag <value>` option that takes a single value.
fn named_arg(long: &'static str) -> Arg {
    Arg::new(long).long(long).num_args(1)
}

/// A positional path argument that opts into native shell path completion.
fn path_arg(id: &'static str, hint: ValueHint) -> Arg {
    Arg::new(id)
        .required(false)
        .num_args(0..=1)
        .value_hint(hint)
}

/// Build the declarative completion-only command tree.
///
/// This mirrors the hand-written dispatch tree in `cli::mod` and `cli::manage`
/// so `clap_complete` can offer accurate static completion. It is **only** used
/// for completion; normal runtime parsing stays in the hand-rolled dispatcher.
pub(crate) fn command() -> Command {
    Command::new("ol")
        .about("OmniLauncher backend controller")
        // Global flags (see `extract_globals`).
        .arg(
            Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                .global(true),
        )
        .arg(
            Arg::new("no-color")
                .long("no-color")
                .action(ArgAction::SetTrue)
                .global(true),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .action(ArgAction::SetTrue)
                .global(true),
        )
        .arg(
            Arg::new("debug")
                .long("debug")
                .action(ArgAction::SetTrue)
                .global(true),
        )
        // ── Lifecycle / ops ───────────────────────────────────────────────
        .subcommand(Command::new("serve").about("run the backend API server (foreground)"))
        .subcommand(Command::new("start").about("start the backend detached and wait for health"))
        .subcommand(Command::new("stop").about("stop the backend"))
        .subcommand(Command::new("restart").about("stop then start"))
        .subcommand(Command::new("status").about("health / process / port / binary view"))
        .subcommand(Command::new("health").about("probe the backend /health endpoint"))
        .subcommand(
            Command::new("logs")
                .about("print/tail the log file")
                .arg(
                    Arg::new("follow")
                        .short('f')
                        .long("follow")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("lines")
                        .short('n')
                        .long("lines")
                        .num_args(1)
                        .value_name("N"),
                ),
        )
        .subcommand(Command::new("doctor").about("diagnostics: config, token, AI, deps"))
        // ── Resource management ───────────────────────────────────────────
        .subcommand(settings_command())
        .subcommand(providers_command())
        .subcommand(skills_command())
        .subcommand(plugins_command())
        // ── Completion / help / version ───────────────────────────────────
        .subcommand(
            Command::new("completion")
                .about("generate shell completion for ol and omnilauncher")
                .arg(
                    Arg::new("shell")
                        .required(false)
                        .num_args(0..=1)
                        .value_parser(SHELL_VALUES.to_vec()),
                ),
        )
        .subcommand(Command::new("help").about("show help"))
        .subcommand(Command::new("version").about("print version"))
}

fn settings_command() -> Command {
    Command::new("settings")
        .about("show/update settings.json fields")
        .subcommand(Command::new("show").visible_alias("list"))
        .subcommand(Command::new("get").arg(with_completer(
            settings_field_arg(),
            complete_settings_fields,
        )))
        .subcommand(
            Command::new("set")
                .arg(with_completer(
                    settings_field_arg(),
                    complete_settings_fields,
                ))
                .arg(Arg::new("value").required(false).num_args(0..=1)),
        )
        .subcommand(Command::new("path").visible_alias("dir"))
        .subcommand(Command::new("help"))
}

fn settings_field_arg() -> Arg {
    Arg::new("field").required(false).num_args(0..=1)
}

fn providers_command() -> Command {
    Command::new("providers")
        .about("list/add/select/update LLM providers and models")
        .subcommand(Command::new("list").visible_alias("ls"))
        .subcommand(Command::new("active").visible_alias("current"))
        .subcommand(
            Command::new("add")
                .arg(resource_name_arg("name"))
                .arg(named_arg("name-flag").long("name"))
                .arg(named_arg("kind").value_parser(PROVIDER_KINDS.to_vec()))
                .arg(named_arg("base-url").value_hint(ValueHint::Url))
                .arg(named_arg("api-key"))
                .arg(named_arg("model"))
                .arg(named_arg("models"))
                .arg(named_arg("id"))
                .arg(Arg::new("active").long("active").action(ArgAction::SetTrue))
                .arg(
                    Arg::new("interactive")
                        .short('i')
                        .long("interactive")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("set-active")
                .visible_aliases(["select", "use"])
                .arg(with_completer(
                    resource_name_arg("id"),
                    complete_provider_ids,
                )),
        )
        .subcommand(
            Command::new("set-model")
                .visible_alias("model")
                .arg(with_completer(
                    resource_name_arg("model"),
                    complete_provider_models,
                ))
                .arg(with_completer(
                    resource_name_arg("provider-id"),
                    complete_provider_ids,
                )),
        )
        .subcommand(
            Command::new("remove")
                .visible_alias("delete")
                .arg(with_completer(
                    resource_name_arg("id"),
                    complete_provider_ids,
                )),
        )
        .subcommand(Command::new("caps").visible_alias("kinds"))
        .subcommand(
            Command::new("login")
                .visible_alias("auth")
                .arg(with_completer(
                    resource_name_arg("id"),
                    complete_provider_ids,
                )),
        )
        .subcommand(
            Command::new("logout")
                .visible_alias("signout")
                .arg(with_completer(
                    resource_name_arg("id"),
                    complete_provider_ids,
                )),
        )
        .subcommand(Command::new("help"))
}

fn skills_command() -> Command {
    Command::new("skills")
        .about("list/install/update/remove skills")
        .subcommand(Command::new("list").visible_alias("ls"))
        .subcommand(
            Command::new("view")
                .visible_alias("show")
                .arg(with_completer(
                    resource_name_arg("name"),
                    complete_skill_names,
                )),
        )
        .subcommand(
            Command::new("install")
                .visible_alias("add")
                .arg(path_arg("source", ValueHint::FilePath)),
        )
        .subcommand(Command::new("update").arg(with_completer(
            resource_name_arg("name"),
            complete_skill_names,
        )))
        .subcommand(Command::new("usage"))
        .subcommand(Command::new("pin").arg(with_completer(
            resource_name_arg("name"),
            complete_skill_names,
        )))
        .subcommand(Command::new("unpin").arg(with_completer(
            resource_name_arg("name"),
            complete_skill_names,
        )))
        .subcommand(Command::new("curator").subcommand(Command::new("run")))
        .subcommand(
            Command::new("remove")
                .visible_aliases(["delete", "uninstall"])
                .arg(with_completer(
                    resource_name_arg("name"),
                    complete_skill_names,
                )),
        )
        .subcommand(Command::new("reload"))
        .subcommand(Command::new("dir").visible_alias("path"))
        .subcommand(Command::new("help"))
}

fn plugins_command() -> Command {
    Command::new("plugins")
        .about("list/install/update/remove external plugins")
        .subcommand(Command::new("list").visible_alias("ls"))
        .subcommand(Command::new("collections"))
        .subcommand(
            Command::new("install")
                .visible_alias("add")
                .arg(path_arg("source", ValueHint::AnyPath))
                .arg(named_arg("target-dir").value_hint(ValueHint::DirPath)),
        )
        .subcommand(Command::new("update").arg(with_completer(
            resource_name_arg("repo-dir"),
            complete_plugin_dirs,
        )))
        .subcommand(
            Command::new("remove")
                .visible_aliases(["delete", "uninstall"])
                .arg(with_completer(
                    resource_name_arg("repo-dir"),
                    complete_plugin_dirs,
                )),
        )
        .subcommand(
            Command::new("update-collection")
                .arg(
                    named_arg("source")
                        .short('s')
                        .value_hint(ValueHint::AnyPath),
                )
                .arg(
                    Arg::new("repo-dir")
                        .num_args(0..)
                        .value_hint(ValueHint::AnyPath),
                ),
        )
        .subcommand(Command::new("remove-collection").arg(Arg::new("repo-dir").num_args(0..)))
        .subcommand(Command::new("runtimes").visible_alias("runtime-deps"))
        .subcommand(
            Command::new("install-runtime")
                .arg(resource_name_arg("id").value_parser(RUNTIME_IDS.to_vec())),
        )
        .subcommand(Command::new("dir").visible_alias("path"))
        .subcommand(Command::new("help"))
}

// ── Dynamic local candidate providers ────────────────────────────────────────
//
// All of these read only local state (settings file, skill metadata, plugin
// directories). They never touch the network or the running backend, never emit
// diagnostics, and never surface secret values (API keys, tokens, or stored
// setting values). On any failure they degrade to an empty candidate list.

/// Sorted, de-duplicated provider IDs from settings.
fn provider_ids(settings: &AppSettings) -> Vec<String> {
    let mut ids = settings
        .providers
        .iter()
        .map(|p| p.id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

/// Sorted, de-duplicated known model names across all providers (each
/// provider's active `model` plus every entry in its `models` list).
fn provider_models(settings: &AppSettings) -> Vec<String> {
    let mut models = Vec::new();
    for provider in &settings.providers {
        if !provider.model.is_empty() {
            models.push(provider.model.clone());
        }
        models.extend(provider.models.iter().cloned());
    }
    models.retain(|m| !m.is_empty());
    models.sort();
    models.dedup();
    models
}

/// Keep only candidates that start with `prefix`, preserving input order.
fn filter_candidates<I, S>(prefix: &str, values: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    values
        .into_iter()
        .map(Into::into)
        .filter(|value| value.starts_with(prefix))
        .collect()
}

/// Non-hidden immediate child directory names of `dir`, sorted. Returns an empty
/// vector if the directory cannot be read.
fn child_dir_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        names.push(name);
    }
    names.sort();
    names
}

/// Turn a Tab-time argument prefix into a `&str`, or `None` if it is not valid
/// Unicode (in which case the completer yields no candidates).
fn prefix_str(current: &OsStr) -> Option<&str> {
    current.to_str()
}

/// Map filtered string candidates into `clap_complete` candidates.
fn candidates(values: Vec<String>) -> Vec<CompletionCandidate> {
    values.into_iter().map(CompletionCandidate::new).collect()
}

/// Completer for configured provider IDs.
fn complete_provider_ids(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(prefix) = prefix_str(current) else {
        return Vec::new();
    };
    let settings = omnilauncher_lib::load_settings();
    candidates(filter_candidates(prefix, provider_ids(&settings)))
}

/// Completer for known provider model names.
fn complete_provider_models(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(prefix) = prefix_str(current) else {
        return Vec::new();
    };
    let settings = omnilauncher_lib::load_settings();
    candidates(filter_candidates(prefix, provider_models(&settings)))
}

/// Completer for settings field names.
fn complete_settings_fields(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(prefix) = prefix_str(current) else {
        return Vec::new();
    };
    candidates(filter_candidates(
        prefix,
        SETTINGS_FIELDS.iter().map(|f| f.to_string()),
    ))
}

/// Completer for installed skill names.
fn complete_skill_names(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(prefix) = prefix_str(current) else {
        return Vec::new();
    };
    let mut mgr = omnilauncher_lib::SkillManager::new();
    mgr.load_all();
    let mut names = mgr
        .list_meta()
        .into_iter()
        .map(|meta| meta.name.clone())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    candidates(filter_candidates(prefix, names))
}

/// Completer for installed external plugin directory names.
fn complete_plugin_dirs(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(prefix) = prefix_str(current) else {
        return Vec::new();
    };
    let dir = omnilauncher_lib::plugins::external::ext_plugins_dir();
    candidates(filter_candidates(prefix, child_dir_names(&dir)))
}

/// Attach a dynamic value completer to a positional argument by id.
fn with_completer(arg: Arg, completer: fn(&OsStr) -> Vec<CompletionCandidate>) -> Arg {
    arg.add(ArgValueCompleter::new(completer))
}

/// Environment variable the generated integrations set to request completion.
/// Must match the variable used by [`complete_env`].
const COMPLETE_VAR: &str = "COMPLETE";

/// The two executable names we register completion for. A script generated for
/// either name registers both, so users get completion regardless of which name
/// they invoke.
const BIN_NAMES: [&str; 2] = ["ol", "omnilauncher"];

/// Write the shell registration script for `shell` to `writer`, registering
/// completion for both `ol` and `omnilauncher`.
pub(crate) fn generate(shell: Shell, writer: &mut dyn std::io::Write) -> std::io::Result<()> {
    let completer = shell.env_completer();
    for bin in BIN_NAMES {
        // `completer` is the binary invoked to compute candidates. We call the
        // same-named binary on PATH, matching how the user invoked us.
        completer.write_registration(COMPLETE_VAR, bin, bin, bin, writer)?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

/// Handle `ol completion <shell>`: write the registration script to stdout and
/// return an exit code. Invalid/missing shell prints a usage error to stderr and
/// returns 2; write/generation failure returns 1.
pub(crate) fn print(shell_name: Option<&str>) -> i32 {
    let Some(shell) = shell_name.and_then(Shell::parse) else {
        eprintln!("usage: ol completion <bash|zsh|fish|powershell|elvish>");
        return 2;
    };
    let mut buffer = Vec::new();
    if generate(shell, &mut buffer).is_err() {
        eprintln!("failed to generate {} completion script", shell.name());
        return 1;
    }
    use std::io::Write as _;
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    if handle
        .write_all(&buffer)
        .and_then(|_| handle.flush())
        .is_err()
    {
        return 1;
    }
    0
}

/// Build the `CompleteEnv` used to handle dynamic completion callbacks. Limited
/// to our five supported shells and driven by the completion-only schema.
fn complete_env<'a>() -> clap_complete::env::CompleteEnv<'a, fn() -> Command> {
    clap_complete::env::CompleteEnv::with_factory(command as fn() -> Command)
        .var(COMPLETE_VAR)
        .shells(Shells::builtins())
}

/// First statement in `main()`: if this process was invoked by a generated shell
/// integration (i.e. `COMPLETE` is set), answer the completion request and exit.
/// For a normal invocation this is a cheap no-op and returns without side
/// effects. Must run before any stdout write or logger init.
pub fn handle_completion_env() {
    complete_env().complete();
}

/// Non-exiting variant used in tests: returns `Ok(true)` when a completion
/// callback was handled, `Ok(false)` for a normal invocation.
#[cfg(test)]
fn try_complete<I, T>(args: I, current_dir: Option<&Path>) -> clap::error::Result<bool>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString>,
{
    complete_env().try_complete(args, current_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use omnilauncher_lib::Provider;

    #[test]
    fn parses_supported_shells() {
        for name in ["bash", "zsh", "fish", "powershell", "elvish"] {
            assert!(Shell::parse(name).is_some(), "expected {name} to parse");
        }
        assert!(Shell::parse("nu").is_none());
    }

    #[test]
    fn every_shell_resolves_a_builtin_completer() {
        for shell in Shell::ALL {
            // Must not panic and must round-trip the canonical name.
            let completer = shell.env_completer();
            assert!(
                completer.is(shell.name()),
                "{} completer name mismatch",
                shell.name()
            );
        }
    }

    #[test]
    fn schema_contains_advertised_top_level_commands() {
        let command = command();
        let names = command
            .get_subcommands()
            .map(|sub| sub.get_name())
            .collect::<Vec<_>>();
        for expected in [
            "serve",
            "start",
            "stop",
            "restart",
            "status",
            "health",
            "logs",
            "doctor",
            "settings",
            "providers",
            "skills",
            "plugins",
            "completion",
            "help",
            "version",
        ] {
            assert!(names.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn schema_contains_nested_commands_and_fixed_values() {
        let command = command();
        let providers = command.find_subcommand("providers").unwrap();
        assert!(providers.find_subcommand("set-active").is_some());
        assert!(providers.find_subcommand("set-model").is_some());
        assert!(providers.find_subcommand("login").is_some());
        assert!(providers.find_subcommand("logout").is_some());
        let plugins = command.find_subcommand("plugins").unwrap();
        assert!(plugins.find_subcommand("install-runtime").is_some());
        let completion = command.find_subcommand("completion").unwrap();
        let shell = completion
            .get_arguments()
            .find(|arg| arg.get_id() == "shell")
            .unwrap();
        let values = shell
            .get_value_parser()
            .possible_values()
            .unwrap()
            .map(|value| value.get_name().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(values, ["bash", "zsh", "fish", "powershell", "elvish"]);
    }

    fn provider(id: &str, model: &str, models: &[&str]) -> Provider {
        Provider {
            id: id.into(),
            model: model.into(),
            models: models.iter().map(|m| m.to_string()).collect(),
            ..Provider::default()
        }
    }

    #[test]
    fn provider_candidates_include_ids_and_deduplicated_models() {
        let settings = AppSettings {
            providers: vec![
                provider("local", "qwen", &["qwen", "llama"]),
                provider("cloud", "claude", &["claude"]),
            ],
            ..AppSettings::default()
        };
        assert_eq!(provider_ids(&settings), ["cloud", "local"]);
        assert_eq!(provider_models(&settings), ["claude", "llama", "qwen"]);
    }

    #[test]
    fn candidates_filter_by_current_prefix() {
        assert_eq!(filter_candidates("cl", ["cloud", "local"]), ["cloud"]);
    }

    #[test]
    fn child_dir_names_returns_only_visible_dirs_sorted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("beta")).unwrap();
        std::fs::create_dir(dir.path().join("alpha")).unwrap();
        std::fs::create_dir(dir.path().join(".hidden")).unwrap();
        std::fs::write(dir.path().join("regular-file"), b"x").unwrap();
        assert_eq!(child_dir_names(dir.path()), ["alpha", "beta"]);
    }

    #[test]
    fn child_dir_names_of_missing_dir_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        assert!(child_dir_names(&missing).is_empty());
    }

    #[test]
    fn every_shell_generates_nonempty_registration_for_both_names() {
        for shell in Shell::ALL {
            let mut output = Vec::new();
            generate(*shell, &mut output).unwrap();
            let script = String::from_utf8(output).unwrap();
            assert!(!script.trim().is_empty(), "empty {} script", shell.name());
            assert!(script.contains("ol"), "{} missing ol", shell.name());
            assert!(
                script.contains("omnilauncher"),
                "{} missing omnilauncher",
                shell.name()
            );
        }
    }

    #[test]
    fn print_rejects_missing_and_invalid_shell() {
        assert_eq!(print(None), 2);
        assert_eq!(print(Some("nu")), 2);
    }

    #[test]
    fn print_accepts_every_supported_shell() {
        for shell in Shell::ALL {
            assert_eq!(print(Some(shell.name())), 0, "{} failed", shell.name());
        }
    }

    #[test]
    fn dynamic_engine_ignores_normal_invocation() {
        // With the COMPLETE env var unset, a normal argv is not a completion
        // callback: try_complete must report `false` (not handled) so main()
        // proceeds to ordinary dispatch.
        let handled = try_complete(["ol", "status"], None).unwrap();
        assert!(
            !handled,
            "normal invocation must not be treated as completion"
        );
    }
}
