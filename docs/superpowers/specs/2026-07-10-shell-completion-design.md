# Shell Completion for `ol` / `omnilauncher` — Design

**Date:** 2026-07-10
**Status:** Approved
**Scope:** Add generated, live shell completion for the backend-only OmniLauncher CLI.

## 1. Goals

- Add `ol completion <shell>` for Bash, Zsh, Fish, PowerShell, and Elvish.
- Write completion scripts to stdout so users can source or install them using normal shell conventions.
- Register each script for both executable names: `ol` and `omnilauncher`.
- Complete the full static command tree: top-level commands, resource subcommands, aliases, flags, and fixed flag values.
- Complete safe local values dynamically, including configured provider IDs/models, installed skill names, installed plugin names, setting field names, provider kinds, and plugin runtime IDs.
- Preserve the existing hand-rolled CLI parser, output, exit codes, and command behavior.

## 2. Non-goals

- Do not migrate normal CLI parsing or help rendering to Clap.
- Do not install completion files automatically.
- Do not contact the backend or network during completion.
- Do not complete secrets or setting values.
- Do not add shell support beyond Bash, Zsh, Fish, PowerShell, and Elvish.

## 3. User interface

```text
ol completion bash
ol completion zsh
ol completion fish
ol completion powershell
ol completion elvish
```

The command emits a script to stdout and exits 0. `omnilauncher completion <shell>` behaves identically. Missing or unsupported shell names print `usage: ol completion <bash|zsh|fish|powershell|elvish>` as a failure and exit 2.

Each generated script registers completion for both `ol` and `omnilauncher`, regardless of which name generated it.

## 4. Architecture

Add `src-tauri/src/cli/completion.rs`. It owns a completion-only Clap command schema and completion generation. The existing dispatcher remains the runtime authority and gains one explicit `completion` arm.

The completion module contains:

1. A shell enum/parser for the five supported shell names.
2. A Clap builder command tree that mirrors the current hand-written dispatch tree.
3. Custom value completers that load current local state when the user presses Tab.
4. A generator that writes the selected shell integration to a supplied writer and registers both binary names.

`OPS_COMMANDS` remains the source used by top-level human help. It gains a `completion` entry. Tests require the completion schema to contain every advertised top-level command, preventing drift between help and completion.

The completion schema does not parse ordinary application invocations and does not replace `dispatch_command` or resource handlers.

## 5. Completion coverage

### Static

- Top-level lifecycle, resource, help, version, and completion commands.
- Nested settings, providers, skills, and plugins commands, including accepted aliases.
- Global and command-specific flags.
- Fixed values:
  - shells: `bash`, `zsh`, `fish`, `powershell`, `elvish`
  - provider kinds: `custom`, `github-copilot`, `azure-foundry`
  - plugin runtimes: `python`, `node`, `dotnet`

### Dynamic local values

- Provider IDs for select/remove/provider-target arguments.
- Known models for model selection.
- Installed skill names for view/update/pin/unpin/remove.
- Installed external plugin directory names for update/remove.
- Settings field names for get/set.

Dynamic completers read local files/directories only. They return an empty candidate list when local state cannot be loaded. They produce no diagnostic output, do not mutate state, and never include API keys or stored setting values.

Path/source arguments remain eligible for native shell path completion where supported.

## 6. Error handling

- Invalid completion command syntax exits 2 through the existing `Output::failure` path.
- Script write/generation failures print a concise failure and exit 1.
- Local dynamic-source failures degrade to static completion without terminal output.
- Candidate text is passed through the completion library’s shell-specific escaping rather than interpolated into handwritten shell code.

## 7. Dependencies

Add compatible `clap` and `clap_complete` dependencies. Use Clap’s builder API; no derive migration is needed. The completion engine is invoked only for completion generation/runtime completion and is isolated from normal argument dispatch.

## 8. Documentation

Update README’s terminal CLI section to list `ol completion <shell>`. Add setup examples for all five shells. Examples may either source generated output in the current session or direct it to the shell’s normal completion location. State that local providers, models, skills, and plugins are read dynamically and that completion performs no network requests.

## 9. Testing

Unit tests cover:

- parsing all supported shell names and rejecting unknown names;
- presence of every advertised top-level command in the completion schema;
- nested commands, aliases, flags, and fixed value sets;
- pure candidate extraction/filtering for providers/models/skills/plugins/settings;
- generated output for every shell;
- registration of both `ol` and `omnilauncher` in every generated integration;
- dispatcher success/error routing for `completion`.

Verification gates:

1. `cargo fmt --check --manifest-path src-tauri/Cargo.toml`
2. `cargo test --manifest-path src-tauri/Cargo.toml --lib`
3. `cargo clippy --manifest-path src-tauri/Cargo.toml --lib --tests -- -D warnings`
4. `cargo build --manifest-path src-tauri/Cargo.toml --release`
5. Generate each shell script from the built binary and verify non-empty output containing both executable registrations.
6. Where the shell executable is available, load the generated script and request representative static and dynamic candidates.

## 10. Risks and mitigations

- **Schema drift:** keep the module declarative and test it against `OPS_COMMANDS`; add focused nested-schema tests.
- **Slow Tab response:** load only local state and avoid backend/network operations. The local datasets are small.
- **Shell-specific escaping:** delegate generation and candidate formatting to `clap_complete` rather than maintaining five handwritten implementations.
- **Dependency/API instability:** pin compatible crate versions in `Cargo.lock` and exercise all five generators in tests and release smoke checks.
