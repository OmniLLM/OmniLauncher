<p align="center">
  <h1 align="center">OmniLauncher</h1>
</p>
<p align="center">A keyboard-first launcher with local search, AI mode, plugins, and slash commands.</p>
<p align="center">
  <a href="https://github.com/OmniLLM/OmniLauncher/actions/workflows/ci.yml"><img alt="Build status" src="https://img.shields.io/github/actions/workflow/status/OmniLLM/OmniLauncher/ci.yml?style=flat-square&branch=main" /></a>
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/github/license/OmniLLM/OmniLauncher?style=flat-square" /></a>
  <a href="https://github.com/OmniLLM/OmniLauncher/releases"><img alt="Release" src="https://img.shields.io/github/v/release/OmniLLM/OmniLauncher?style=flat-square&include_prereleases" /></a>
</p>

![OmniLauncher Chat UI](screenshot-chat-ui.png)

---

### Requirements

OmniLauncher is built from source today (Tauri v2 + Rust backend + a small Node/Vite frontend). You need these on **both Linux/macOS and Windows**:

- **Rust** (stable, via [rustup](https://rustup.rs)) — provides `cargo`
- **Node.js 18+** — provides `npm`
- **GNU Make** — build/install helper only
- **Tauri v2 system dependencies** — follow the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS

### Build and install

Clone the repo, install deps, build the single binary, and put it on your `PATH`:

```bash
git clone https://github.com/OmniLLM/OmniLauncher.git
cd OmniLauncher
npm install
cd src-tauri && cargo fetch && cd ..

# Build frontend assets + release binary
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

| Behavior                           | Setting                                         |
| ---------------------------------- | ----------------------------------------------- |
| Use a shared token (split-machine) | set `OMNILAUNCHER_AUTH_TOKEN` on the backend    |
| Single-machine dev                 | leave it unset — a random one is generated      |
| Backend token file fallback        | `~/.config/omnilauncher/server-token`           |
| Frontend saved connection token    | `~/.config/omnilauncher/backend-token`          |

> [!NOTE]
> Paths shown with `~/` resolve to your home directory on every OS. On Windows that means `C:\Users\<you>\.config\omnilauncher\...` and `C:\Users\<you>\.omnilauncher\...` — OmniLauncher uses these dot-directories on Windows too, **not** `%APPDATA%`.

### Modes

OmniLauncher switches mode based on the prefix you type in the GUI:

- **Launcher** — bare text. Apps, files, URLs, built-in actions.
- **AI** — type `?` or `ai `. Explanations, summaries, code help, tool-assisted tasks.
- **Slash** — type `/`. Structured commands.

Common slash commands:

| Command                 | Purpose                         |
| ----------------------- | ------------------------------- |
| `/app`                  | launch an app                   |
| `/run`                  | run a shell command             |
| `/open`                 | open a file, app, or URL        |
| `/find`, `/grep`, `/ls` | search and inspect files        |
| `/todo`                 | manage todos                    |
| `/web`                  | search the web                  |
| `/skills`               | manage AI skills                |
| `/plugins`              | install, update, remove plugins |

### Terminal CLI (`ol`)

`ol` operates the OmniLauncher binary and admin resources directly. It does **not** duplicate the GUI query palette or shell tools.

```bash
ol help                       # command list
ol serve                      # backend API server in the foreground
ol gui                        # desktop shell in the foreground
ol start | stop | restart     # manage detached backend
ol status                     # health / process / port view
ol logs -f                    # tail the log file
ol doctor                     # diagnostics

ol settings show              # print settings JSON
ol settings get ai_model
ol settings set ai_model gpt-4.1

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
```

Global flags apply anywhere: `--json` (machine-readable output), `--no-color` (also honors `NO_COLOR` and non-TTY pipes), `-q`/`--quiet`, and `--debug` (file logging to `~/.omnilauncher/omnilauncher.log`).

> [!NOTE]
> Running `omnilauncher` with no arguments still launches the desktop GUI. The legacy `--server` / `--debug` flags still work. Split-machine deploys use `ol serve` on the backend host and `ol gui` on the desktop host.

### AGENTS.md Context

Drop an `AGENTS.md` file at `~/.config/omnilauncher/AGENTS.md` to use it as the primary global AI system prompt source. OmniLauncher also picks up legacy/project context files automatically (load order — most general first, most specific last):

1. `~/.config/omnilauncher/AGENTS.md` — primary global app system prompt
   - Legacy fallback: `~/.config/omnilauncher/AGENT.md`
2. `AGENT.md` walking upward from the current working directory — project context
3. `~/AGENT.md` — user-global

Missing files are silently skipped. See [`src-tauri/src/ai/agent_context.rs`](src-tauri/src/ai/agent_context.rs) for the loader.

### Make targets

```bash
make build       # npm run build + cargo build --release
make install     # build and symlink ol + omnilauncher into ~/.local/bin
make uninstall   # remove those symlinks
make clean       # remove dist/ and src-tauri/target/
```

### Config Files

| Path                                   | Purpose                            |
| -------------------------------------- | ---------------------------------- |
| `~/.config/omnilauncher/settings.json` | Main settings (UI/CLI editable)    |
| `~/.config/omnilauncher/server-token`  | Backend token fallback             |
| `~/.config/omnilauncher/AGENTS.md`     | Primary global AI system prompt    |
| `~/.omnilauncher/`                     | Runtime data (DB, plugins, skills) |
| `~/.omnilauncher/run/`                 | CLI PID files (backend, gui)       |

### Troubleshooting

- **Settings won't save** — backend token mismatch between UI and backend. Re-set both.
- **AI requests fail** — check **AI** tab values (Provider URL, API Key, Model), or use `ol settings get ai_base_url` / `ol settings get ai_model`.
- **UI reaches backend but saves still fail** — backend wasn't started with the same `OMNILAUNCHER_AUTH_TOKEN`.
- **Backend won't start** — try `ol stop`, then `ol start` again; check `ol logs`.
- **Windows: `make: command not found`** — install `make` (Chocolatey/Scoop) or run from MSYS2 / Git Bash.
- **Windows: env var didn't take effect** — use PowerShell syntax `$env:NAME = "value"`, and set it in the *same* shell that runs `ol`.

### Documentation

More detail in [`docs/`](./docs) and in [`SOURCE_FILES_MANIFEST.md`](./SOURCE_FILES_MANIFEST.md).

### Contributing

PRs welcome. Please run the relevant checks (`cargo test`, `npm test`, `npm run build`) before submitting, and keep new behavior covered by tests.

### Security

Report security issues per [`SECURITY.md`](./SECURITY.md). Don't open public issues for vulnerabilities.

### Building on OmniLauncher

If you're shipping a project that uses "omnilauncher" in its name (e.g. `omnilauncher-plugin-foo`), please add a note to your README clarifying that it isn't built by the OmniLauncher team and isn't affiliated with us.

---

**Project** [OmniLLM/OmniLauncher](https://github.com/OmniLLM/OmniLauncher) · **License** see [LICENSE](./LICENSE)
