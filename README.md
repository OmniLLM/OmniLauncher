<p align="center">
  <h1 align="center">OmniLauncher</h1>
</p>
<p align="center">A backend service and CLI for local AI tools, plugins, skills, slash commands, and A2A.</p>
<p align="center">
  <a href="https://github.com/OmniLLM/OmniLauncher/actions/workflows/ci.yml"><img alt="Build status" src="https://img.shields.io/github/actions/workflow/status/OmniLLM/OmniLauncher/ci.yml?style=flat-square&branch=main" /></a>
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/github/license/OmniLLM/OmniLauncher?style=flat-square" /></a>
  <a href="https://github.com/OmniLLM/OmniLauncher/releases"><img alt="Release" src="https://img.shields.io/github/v/release/OmniLLM/OmniLauncher?style=flat-square&include_prereleases" /></a>
</p>

---

### Requirements

OmniLauncher is now backend-only. You need:

- **Rust** (stable, via [rustup](https://rustup.rs)) — provides `cargo`
- **GNU Make** — optional build/install helper

Node, Vite, React, Tauri app assets, and desktop-shell prerequisites are no longer part of this repository.

### Build and install

Clone the repo, build the single backend binary, and put it on your `PATH`:

```bash
git clone https://github.com/OmniLLM/OmniLauncher.git
cd OmniLauncher
cd src-tauri && cargo fetch && cd ..

# Build release binary
make build

# Symlink ~/.local/bin/ol and ~/.local/bin/omnilauncher to the release binary
make install
```

`make` is intentionally small: it only builds, installs, uninstalls, cleans, and removes the release binary. Runtime management lives in the binary CLI.

### Backend Token

If your backend runs on a separate machine, set its token before starting the backend:

```bash
export OMNILAUNCHER_AUTH_TOKEN=<paste-your-token-here>
ol start
```

```powershell
$env:OMNILAUNCHER_AUTH_TOKEN = "<paste-your-token-here>"
ol start
```

| Behavior                           | Setting                                      |
| ---------------------------------- | -------------------------------------------- |
| Use a shared token (split-machine) | set `OMNILAUNCHER_AUTH_TOKEN` on the backend |
| Single-machine dev                 | leave it unset — a random one is generated   |
| Backend token file fallback        | `~/.config/omnilauncher/server-token`        |

> [!NOTE]
> Paths shown with `~/` resolve to your home directory on every OS. On Windows that means `C:\Users\<you>\.config\omnilauncher\...` and `C:\Users\<you>\.omnilauncher\...` — OmniLauncher uses these dot-directories on Windows too, **not** `%APPDATA%`.

### Terminal CLI (`ol`)

`ol` operates the OmniLauncher backend and admin resources directly.

```bash
ol help                       # command list
ol serve                      # backend API server in the foreground
ol start | stop | restart     # manage detached backend
ol status                     # health / process / port view
ol health                     # probe /health
ol logs -f                    # tail the log file
ol doctor                     # diagnostics

ol settings show              # print settings JSON
ol settings get ai_model       # legacy flat field, synced from active provider
ol settings set ai_model gpt-4.1

ol providers list              # saved LLM providers; active row is marked '*'
ol providers active            # show active provider + capabilities
ol providers add local --kind custom --base-url http://127.0.0.1:5000 --model auto --active
ol providers add foundry --kind azure-foundry --base-url https://<endpoint>/models --api-key <key> --models gpt-5,gpt-5-mini
ol providers set-active foundry
ol providers set-model gpt-5-mini
ol providers caps              # custom / github-copilot / azure-foundry capability table

ol skills list
ol skills view <name>
ol skills install <url-or-SKILL.md-path>
ol skills update <name>
ol skills remove <name>
ol skills usage
ol skills pin <name>
ol skills curator run

ol plugins list
ol plugins collections
ol plugins install <url-or-path>
ol plugins update <repo-dir-name>
ol plugins remove <repo-dir-name>
ol plugins runtimes
ol plugins install-runtime python

ol completion bash            # generate shell completion (bash/zsh/fish/powershell/elvish)
```

Global flags apply anywhere: `--json` (machine-readable output), `--no-color` (also honors `NO_COLOR` and non-TTY pipes), `-q`/`--quiet`, and `--debug` (file logging to `~/.omnilauncher/omnilauncher.log`).

Running `omnilauncher` with no arguments prints the same help as `ol`. The legacy `--server` / `--debug` flags still work.

### Shell completion

`ol completion <shell>` writes a completion integration to stdout for Bash, Zsh,
Fish, PowerShell, or Elvish. Each generated script registers completion for
**both** `ol` and `omnilauncher`, regardless of which name you generated it with.

```bash
# Bash — source per-session, or install once:
source <(ol completion bash)
ol completion bash | sudo tee /etc/bash_completion.d/ol > /dev/null

# Zsh — put it on your $fpath, e.g.:
ol completion zsh > "${fpath[1]}/_ol"

# Fish:
ol completion fish > ~/.config/fish/completions/ol.fish

# Elvish:
ol completion elvish >> ~/.config/elvish/rc.elv
```

```powershell
# PowerShell — add to your profile:
ol completion powershell | Out-String | Invoke-Expression
```

Completion is **live and local**: provider IDs/models, installed skill names,
installed plugin directory names, and settings field names are read from local
state at the moment you press Tab. It makes **no** network or backend request and
never suggests secrets (API keys, tokens, or stored setting values). If local
state can't be read, completion silently falls back to the static command tree.

### AGENTS.md Context

Drop an `AGENTS.md` file at `~/.config/omnilauncher/AGENTS.md` to use it as the primary global AI system prompt source. OmniLauncher also picks up legacy/project context files automatically (load order — most general first, most specific last):

1. `~/.config/omnilauncher/AGENTS.md` — primary global app system prompt
   - Legacy fallback: `~/.config/omnilauncher/AGENT.md`
2. `AGENT.md` walking upward from the current working directory — project context
3. `~/AGENT.md` — user-global

Missing files are silently skipped. See [`src-tauri/src/ai/agent_context.rs`](src-tauri/src/ai/agent_context.rs) for the loader.

### Make targets

```bash
make build       # cargo build --release
make install     # build and symlink ol + omnilauncher into ~/.local/bin
make uninstall   # remove those symlinks
make clean       # remove Rust build artifacts
```

### Config Files

| Path                                   | Purpose                            |
| -------------------------------------- | ---------------------------------- |
| `~/.config/omnilauncher/settings.json` | Main settings (CLI editable)       |
| `~/.config/omnilauncher/server-token`  | Backend token fallback             |
| `~/.config/omnilauncher/AGENTS.md`     | Primary global AI system prompt    |
| `~/.omnilauncher/`                     | Runtime data (DB, plugins, skills) |
| `~/.omnilauncher/run/`                 | CLI PID files                      |

### Troubleshooting

- **Backend won't start** — try `ol stop`, then `ol start` again; check `ol logs`.
- **AI requests fail** — check the active provider with `ol providers active`; legacy fields are visible via `ol settings get ai_base_url` / `ol settings get ai_model`.
- **Token mismatch** — set `OMNILAUNCHER_AUTH_TOKEN` consistently, or delete `~/.config/omnilauncher/server-token` and restart.
- **Windows: `make: command not found`** — install `make` (Chocolatey/Scoop) or use `cargo build --manifest-path src-tauri/Cargo.toml --release` directly.
- **Windows: env var didn't take effect** — use PowerShell syntax `$env:NAME = "value"`, and set it in the *same* shell that runs `ol`.

### Documentation

More detail in [`docs/`](./docs).

### Contributing

PRs welcome. Please run the relevant checks (`cargo test`, `cargo clippy`) before submitting, and keep new behavior covered by tests.

### Security

Report security issues per [`SECURITY.md`](./SECURITY.md). Don't open public issues for vulnerabilities.

### Building on OmniLauncher

If you're shipping a project that uses "omnilauncher" in its name (e.g. `omnilauncher-plugin-foo`), please add a note to your README clarifying that it isn't built by the OmniLauncher team and isn't affiliated with us.

---

**Project** [OmniLLM/OmniLauncher](https://github.com/OmniLLM/OmniLauncher) · **License** see [LICENSE](./LICENSE)
