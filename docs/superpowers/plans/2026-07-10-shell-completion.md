# Shell Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add generated Bash, Zsh, Fish, PowerShell, and Elvish completion for both OmniLauncher executable names, including safe live local candidates.

**Architecture:** Keep the hand-written production dispatcher unchanged except for a new `completion` command and an early dynamic-completion hook. A focused `cli/completion.rs` defines a Clap builder schema, uses `clap_complete`'s dynamic completion engine for runtime candidates, and emits shell registration scripts to stdout. Candidate providers read only local settings, skill metadata, and plugin directories.

**Tech Stack:** Rust 2021, Clap builder API, `clap_complete` with `unstable-dynamic`, existing OmniLauncher settings/skills/plugin APIs.

---

## File map

- Create `src-tauri/src/cli/completion.rs`: shell parsing, completion-only command schema, local candidate providers, dynamic completion entrypoint, and shell registration generation.
- Modify `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock`: add Clap completion dependencies.
- Modify `src-tauri/src/cli/mod.rs`: expose the module, route `completion`, advertise it in help, and verify schema/help consistency.
- Modify `src-tauri/src/main.rs`: let the dynamic completion engine handle shell callbacks before logging or normal dispatch writes output.
- Modify `README.md`: document generation/setup for all supported shells and dynamic local behavior.

### Task 1: Add completion dependencies and shell parsing

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Create: `src-tauri/src/cli/completion.rs`
- Modify: `src-tauri/src/cli/mod.rs:13-17`

- [ ] **Step 1: Add a failing shell parsing test**

Create `src-tauri/src/cli/completion.rs` with a test module that requires `Shell::parse` to accept `bash`, `zsh`, `fish`, `powershell`, and `elvish`, and reject `nu`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_shells() {
        for name in ["bash", "zsh", "fish", "powershell", "elvish"] {
            assert!(Shell::parse(name).is_some(), "missing shell {name}");
        }
        assert!(Shell::parse("nu").is_none());
    }
}
```

Expose it with `pub mod completion;` in `cli/mod.rs`.

- [ ] **Step 2: Run the focused test and observe failure**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib cli::completion::tests::parses_supported_shells
```

Expected: compilation fails because `Shell` is undefined.

- [ ] **Step 3: Add dependencies and minimal shell type**

Add:

```toml
clap = "4.5"
clap_complete = { version = "4.6", features = ["unstable-dynamic"] }
```

Implement a private `Shell` enum with `parse`, `name`, and conversion to the appropriate `clap_complete::Shell`/dynamic shell adapter. Use the exact API from the locked crate source rather than handwritten shell strings.

- [ ] **Step 4: Run the focused test**

Run the same command. Expected: PASS and `Cargo.lock` updated.

### Task 2: Define and test the static completion schema

**Files:**
- Modify: `src-tauri/src/cli/completion.rs`
- Modify: `src-tauri/src/cli/manage.rs:13-39`

- [ ] **Step 1: Expose safe static settings metadata**

Change the existing constant to:

```rust
pub(crate) const SETTINGS_FIELDS: &[&str] = &[
    // existing entries unchanged
];
```

This lets completion reuse the validator's source of truth without exposing it outside the crate.

- [ ] **Step 2: Add failing schema tests**

Add tests that call `command()` and assert:

```rust
#[test]
fn schema_contains_advertised_top_level_commands() {
    let command = command();
    let names = command
        .get_subcommands()
        .map(|sub| sub.get_name())
        .collect::<Vec<_>>();
    for expected in [
        "serve", "start", "stop", "restart", "status", "health", "logs",
        "doctor", "settings", "providers", "skills", "plugins", "completion",
        "help", "version",
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
    let plugins = command.find_subcommand("plugins").unwrap();
    assert!(plugins.find_subcommand("install-runtime").is_some());
    let completion = command.find_subcommand("completion").unwrap();
    let shell = completion.get_arguments().find(|arg| arg.get_id() == "shell").unwrap();
    let values = shell
        .get_value_parser()
        .possible_values()
        .unwrap()
        .map(|value| value.get_name().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(values, ["bash", "zsh", "fish", "powershell", "elvish"]);
}
```

- [ ] **Step 3: Run tests and observe failure**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib cli::completion::tests::schema
```

Expected: FAIL because `command()` is undefined.

- [ ] **Step 4: Build the declarative command tree**

Implement `pub(crate) fn command() -> clap::Command` with:

- global `--json`, `--no-color`, `-q`/`--quiet`, and `--debug`;
- every top-level command in `OPS_COMMANDS`, plus `help` and `version`;
- all nested canonical commands and accepted aliases from `manage.rs`;
- `logs -f/--follow` and `-n/--lines <N>`;
- provider add flags (`--name`, `--kind`, `--base-url`, `--api-key`, `--model`, `--models`, `--id`, `--active`);
- plugin install `--target-dir`;
- fixed parsers for shell, provider-kind, and runtime values;
- `ValueHint::FilePath` or `ValueHint::AnyPath` for local source/path arguments.

Use small helpers (`named_arg`, `resource_name_arg`) to keep the builder readable; do not make the schema parse normal runtime commands.

- [ ] **Step 5: Run schema tests**

Run the Task 2 test command. Expected: PASS.

### Task 3: Add safe dynamic local candidates

**Files:**
- Modify: `src-tauri/src/cli/completion.rs`

- [ ] **Step 1: Add failing pure candidate tests**

Define tests around pure conversion/filtering helpers:

```rust
#[test]
fn provider_candidates_include_ids_and_deduplicated_models() {
    let settings = AppSettings {
        providers: vec![
            Provider { id: "local".into(), model: "qwen".into(), models: vec!["qwen".into(), "llama".into()], ..Provider::default() },
            Provider { id: "cloud".into(), model: "claude".into(), models: vec!["claude".into()], ..Provider::default() },
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
```

Also test plugin-directory extraction using a temporary directory containing directories, a regular file, and a hidden directory; only non-hidden directories should be returned.

- [ ] **Step 2: Run focused tests and observe failure**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib cli::completion::tests::provider_candidates cli::completion::tests::candidates_filter cli::completion::tests::plugin
```

Expected: compilation fails because the helpers are undefined.

- [ ] **Step 3: Implement pure extractors and runtime completers**

Implement sorted, deduplicated helpers for provider IDs/models and directory names. Implement `ArgValueCompleter` callbacks returning `CompletionCandidate` values for:

- `omnilauncher_lib::load_settings()` provider IDs/models;
- `SETTINGS_FIELDS`;
- a freshly loaded `SkillManager`'s installed skill metadata;
- child directories of `ext_plugins_dir()`.

All callbacks must return an empty vector on non-Unicode prefixes or filesystem/load failures, write nothing to stdout/stderr, and never expose `ai_api_key`, `github_token`, `a2a_token`, or any stored value.

Attach them to the relevant positional arguments in `command()` via Clap's extension mechanism.

- [ ] **Step 4: Run focused and module tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib cli::completion
```

Expected: PASS.

### Task 4: Generate scripts and route the public command

**Files:**
- Modify: `src-tauri/src/cli/completion.rs`
- Modify: `src-tauri/src/cli/mod.rs:111-178,180-260`

- [ ] **Step 1: Add failing generation tests**

For each supported shell, call a writer-based function:

```rust
#[test]
fn every_shell_generates_nonempty_registration_for_both_names() {
    for shell in Shell::ALL {
        let mut output = Vec::new();
        generate(*shell, &mut output).unwrap();
        let script = String::from_utf8(output).unwrap();
        assert!(!script.trim().is_empty(), "empty {} script", shell.name());
        assert!(script.contains("ol"), "{} missing ol", shell.name());
        assert!(script.contains("omnilauncher"), "{} missing omnilauncher", shell.name());
    }
}
```

Add dispatcher tests for missing/invalid shell returning `Handled(2)` and every valid shell returning `Handled(0)`.

- [ ] **Step 2: Run tests and observe failure**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib cli::completion cli::tests::completion
```

Expected: FAIL because generation and dispatch are not implemented.

- [ ] **Step 3: Implement writer-based generation**

Implement:

```rust
pub(crate) fn print(shell_name: Option<&str>) -> i32
```

It parses the shell, generates registration for `ol`, generates or aliases registration for `omnilauncher` using the library's supported bin/completer API, writes only scripts to stdout on success, and prints this exact failure on invalid input:

```text
usage: ol completion <bash|zsh|fish|powershell|elvish>
```

Return 1 on generation/write errors and 2 on usage errors.

- [ ] **Step 4: Route and advertise `completion`**

Add to `dispatch_command`:

```rust
"completion" => Dispatch::Handled(completion::print(args.first().map(String::as_str))),
```

Add to `OPS_COMMANDS`:

```rust
OpsCommand {
    name: "completion",
    desc: "generate shell completion for ol and omnilauncher",
},
```

- [ ] **Step 5: Run all CLI unit tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib cli::
```

Expected: PASS.

### Task 5: Handle dynamic completion callbacks before normal output

**Files:**
- Modify: `src-tauri/src/cli/completion.rs`
- Modify: `src-tauri/src/main.rs:72-110`

- [ ] **Step 1: Add a dynamic entrypoint test**

Add a test that calls the dynamic engine's non-exiting API with a normal argv and asserts it reports `false` (not handled). Add a callback-shaped argv/environment case using the exact protocol emitted by the locked `clap_complete` version and assert it reports `true` with a representative candidate.

- [ ] **Step 2: Run the test and observe failure**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib cli::completion::tests::dynamic
```

Expected: FAIL because `try_complete` is undefined.

- [ ] **Step 3: Implement the early hook**

Implement `pub(crate) fn complete_env()` around `CompleteEnv::with_factory(command)` (or the locked version's equivalent), configured so the generated integrations invoke the installed binary and support the five selected shells.

Call it as the first statement in `main()`, before argument logging, logger initialization, help, or any stdout write:

```rust
fn main() {
    cli::completion::complete_env();
    let args: Vec<String> = std::env::args().collect();
    // existing main body unchanged
}
```

The completion engine exits only when it handled a callback; normal commands continue unchanged.

- [ ] **Step 4: Run unit tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Expected: PASS.

### Task 6: Document setup and behavior

**Files:**
- Modify: `README.md:52-112`

- [ ] **Step 1: Add the command to CLI examples**

Add:

```bash
ol completion bash              # generate completion (bash/zsh/fish/powershell/elvish)
```

- [ ] **Step 2: Add shell setup examples**

Document generation/source or standard install commands for Bash, Zsh, Fish, PowerShell, and Elvish. Use `ol completion <shell>` in each example and state that the generated integration covers both `ol` and `omnilauncher`.

- [ ] **Step 3: Document dynamic behavior and safety**

State that provider IDs/models, installed skill names, and installed plugin names are read from local state at completion time; no backend or network request is made and secrets are not suggested.

- [ ] **Step 4: Check documentation command accuracy**

Run each generation command against the debug binary and confirm it exits 0 with non-empty output.

### Task 7: Full verification and quality pass

**Files:**
- Modify as required by verification findings only.

- [ ] **Step 1: Format and check formatting**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

Expected: PASS.

- [ ] **Step 2: Run the complete unit suite**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Expected: all tests PASS.

- [ ] **Step 3: Run Clippy**

Run:

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml --lib --tests -- -D warnings
```

Expected: PASS with no warnings.

- [ ] **Step 4: Build release binary**

Run:

```bash
cargo build --manifest-path src-tauri/Cargo.toml --release
```

Expected: PASS.

- [ ] **Step 5: Smoke every generator**

Run:

```bash
for shell in bash zsh fish powershell elvish; do
  src-tauri/target/release/omnilauncher completion "$shell" > "/tmp/omnilauncher-$shell-completion"
  test -s "/tmp/omnilauncher-$shell-completion"
  grep -q 'ol' "/tmp/omnilauncher-$shell-completion"
  grep -q 'omnilauncher' "/tmp/omnilauncher-$shell-completion"
done
```

Expected: exit 0.

- [ ] **Step 6: Exercise completion end to end**

For every installed supported shell, source/load its generated integration in a clean shell and request candidates for:

- `ol st` → `start`, `status`, `stop`;
- `omnilauncher completion ` → all five shell names;
- `ol settings get ` → setting fields;
- `ol providers set-active ` → configured provider IDs;
- `ol skills view ` → installed skill names;
- `ol plugins update ` → installed plugin directory names.

Record unavailable shell executables as skipped rather than claiming verification.

- [ ] **Step 7: Review the final diff**

Run the project code-review and simplification workflows over only the changed files. Apply verified findings, then rerun Steps 1-5.
