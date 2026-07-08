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
- **GNU Make** — drives the build/run targets
- **Tauri v2 system dependencies** — follow the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS

Platform specifics:

| Item                 | Linux / macOS                                  | Windows                                                                                          |
| -------------------- | ---------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Shell behind `make`  | `bash` (built in) runs `scripts/ops.sh`        | Windows PowerShell 5.1 (built in) runs `scripts/ops.ps1`                                         |
| Extra shell          | —                                              | **PowerShell 7+ (`pwsh`)** for `make logs` and the smoke / e2e test targets                      |
| Installing `make`    | preinstalled, or `apt install make` / `brew install make` | `choco install make` / `scoop install make`, or run under MSYS2 / Git Bash            |
| Webview + toolchain  | WebKitGTK (from Tauri prereqs)                 | WebView2 (preinstalled on Windows 11) + MSVC Build Tools                                         |

### Installation

Clone the repo, install deps, and start the app. The clone/install steps are identical on every OS:

```bash
git clone https://github.com/OmniLLM/OmniLauncher.git
cd OmniLauncher

# Install JS deps and prefetch Rust crates
npm install
cd src-tauri && cargo fetch && cd ..

# Day-to-day dev: start frontend + backend together
make start
```

The `make` commands (`make start`, `make stop`, `make status`, …) are the **same on every OS** — they dispatch to `scripts/ops.sh` on Linux/macOS and `scripts/ops.ps1` on Windows automatically. See [Linux & Windows notes](#linux--windows-notes) for the few real differences.

> [!TIP]
> Press **Ctrl+Shift+O** to open the launcher, **Ctrl+,** to open Settings, and **Esc** to close.

### Backend Token

If your backend runs on a separate machine, set its token **before** configuring the UI.

```bash
# On the backend machine — Linux / macOS
export OMNILAUNCHER_AUTH_TOKEN=<paste-your-token-here>
make start-backend
```

```powershell
# On the backend machine — Windows PowerShell
$env:OMNILAUNCHER_AUTH_TOKEN = "<paste-your-token-here>"
make start-backend
```

| Behavior                           | Setting                                         |
| ---------------------------------- | ----------------------------------------------- |
| Use a shared token (split-machine) | set `OMNILAUNCHER_AUTH_TOKEN` on the backend    |
| Single-machine dev                 | leave it unset — a random one is generated      |
| Backend token file fallback        | `~/.config/omnilauncher/server-token`           |
| Frontend saved connection token    | `~/.config/omnilauncher/backend-token`          |

> [!NOTE]
> Paths shown with `~/` resolve to your home directory on every OS. On Windows that means `C:\Users\<you>\.config\omnilauncher\...` and `C:\Users\<you>\.omnilauncher\...` — OmniLauncher uses these dot-directories on Windows too, **not** `%APPDATA%`.

When the frontend starts and no saved connection token exists, it prompts for the backend token and stores it in `~/.config/omnilauncher/backend-token`. The token is not stored in `settings.json` and does not appear on the Settings page.

In the UI press **Ctrl+,**, open **General**, set **Backend URL**, then open **AI** and set **Provider URL**, **API Key**, and **Model**. Click **Save Settings**.

> [!IMPORTANT]
> The backend token authenticates OmniLauncher itself. **API Key** authenticates your LLM provider. They are separate — configure the backend token prompt first when using a split-machine backend.

### Modes

OmniLauncher switches mode based on the prefix you type.

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

OmniLauncher ships as a single self-dispatching binary that also works as a first-class terminal CLI. Put it on your `PATH` as `ol`:

```bash
# Linux / macOS — symlinks ~/.local/bin/ol -> the release binary
make install-cli
```

```powershell
# Windows — `make install-cli` is Unix-only. Add the release dir to PATH,
# or copy/alias the exe. One-off for the current session:
Set-Alias ol "$PWD\src-tauri\target\release\omnilauncher.exe"

# Persistent: add the release directory to your user PATH, then call it as `omnilauncher`
[Environment]::SetEnvironmentVariable(
  "Path",
  "$env:Path;$PWD\src-tauri\target\release",
  "User")
```

`ol` runs local operations **in-process** — no backend required, works offline — and exposes the same slash-command surface you use in the GUI, plus lifecycle/ops commands.

```bash
ol                          # interactive REPL (when run on a TTY)
ol calc "2+2*10"            # one-shot slash command
ol grep TODO src/           # any /command works as `ol <command>`
ol ai "explain lifetimes"   # route through the AI
ol status                   # health / process / port view
ol start | stop | restart   # manage a detached backend
ol logs -f                  # tail the log file
ol doctor                   # diagnostics: config, token, AI, deps
ol --help                   # full command list
```

Global flags apply anywhere: `--json` (machine-readable output), `--no-color` (also honors `NO_COLOR` and non-TTY pipes), `-q`/`--quiet`, and `--debug` (file logging to `~/.omnilauncher/omnilauncher.log`).

In the **REPL**, bare words are queries, `/cmd` runs a slash command, `? question` (or `ai …`) asks the AI, and `:status` / `:start` / `:stop` run ops (the `:` prefix disambiguates them from query terms like "status"). History persists to `~/.omnilauncher/repl_history`; press **Tab** to complete command names, **Ctrl-D** to exit.

> [!NOTE]
> The same binary still launches the desktop GUI when run with no arguments, and still accepts the legacy `--server` / `--debug` flags. Split-machine deploys use `ol serve` on the backend host and `ol gui` on the desktop host.

### AGENTS.md Context

Drop an `AGENTS.md` file at `~/.config/omnilauncher/AGENTS.md` to use it as the primary global AI system prompt source. OmniLauncher also picks up legacy/project context files automatically (load order — most general first, most specific last):

1. `~/.config/omnilauncher/AGENTS.md` — primary global app system prompt
   - Legacy fallback: `~/.config/omnilauncher/AGENT.md`
2. `AGENT.md` walking upward from the current working directory — project context
3. `~/AGENT.md` — user-global

Missing files are silently skipped. See [`src-tauri/src/ai/agent_context.rs`](src-tauri/src/ai/agent_context.rs) for the loader.

### Common Commands

```bash
make start         # start frontend + backend
make stop          # stop everything
make restart       # restart everything
make status        # show backend status
make logs          # tail logs (Windows: needs pwsh)
make install-cli   # symlink the `ol` CLI onto your PATH (Linux/macOS only)
make test          # run the full test suite
```

Run `make help` for the full target list, or `make help-advanced` for compatibility aliases and variables.

### Linux & Windows notes

The `make` targets are identical across platforms — the Makefile picks the right helper script per OS (`scripts/ops.sh` on Linux/macOS via `bash`, `scripts/ops.ps1` on Windows via PowerShell). The differences you actually hit:

| Topic                | Linux / macOS                                | Windows                                                                      |
| -------------------- | -------------------------------------------- | --------------------------------------------------------------------------- |
| Setting env vars     | `export OMNILAUNCHER_AUTH_TOKEN=…`           | `$env:OMNILAUNCHER_AUTH_TOKEN = "…"` (PowerShell)                            |
| Release binary       | `src-tauri/target/release/omnilauncher`      | `src-tauri\target\release\omnilauncher.exe`                                  |
| `make install-cli`   | symlinks `~/.local/bin/ol` → the binary      | not supported — alias the `.exe` or add its dir to `PATH` (see [Terminal CLI](#terminal-cli-ol)) |
| `make logs` / smoke / e2e | run under `bash`, need `curl`           | need **PowerShell 7+ (`pwsh`)** on `PATH`                                    |
| Home paths (`~/…`)   | `/home/<you>/…` or `/Users/<you>/…`          | `C:\Users\<you>\…` (dot-directories, not `%APPDATA%`)                        |
| Split-machine backend | run `make start-backend` on the host        | `make start BACKEND_MODE=wsl` runs the backend inside WSL (Windows-only)     |

All the everyday lifecycle commands work the same way on both:

```bash
make start                 # start frontend + backend (both OSes)
make stop                  # stop everything
make restart               # rebuild + restart
make status                # backend health / process / port
make start ROLE=backend    # backend only
make start ROLE=frontend   # desktop shell only
```

> [!NOTE]
> On Windows, run `make` from a shell where `make` is installed (MSYS2, Git Bash, or a native `make` from Chocolatey/Scoop). The helper it invokes uses the built-in Windows PowerShell, so you don't need `pwsh` for `start` / `stop` / `status` — only for `logs` and the smoke/e2e test targets.

### Config Files

| Path                                   | Purpose                            |
| -------------------------------------- | ---------------------------------- |
| `~/.config/omnilauncher/settings.json` | Main settings (UI-editable)        |
| `~/.config/omnilauncher/server-token`  | Backend token fallback             |
| `~/.config/omnilauncher/AGENTS.md`  | Primary global AI system prompt     |
| `~/.omnilauncher/`                     | Runtime data (DB, plugins, skills) |
| `~/.omnilauncher/run/`                 | CLI PID files (backend, gui)       |
| `~/.omnilauncher/repl_history`         | `ol` REPL command history          |

### Troubleshooting

- **Settings won't save** — backend token mismatch between UI and backend. Re-set both.
- **AI requests fail** — check **AI** tab values (Provider URL, API Key, Model).
- **UI reaches backend but saves still fail** — backend wasn't started with the same `OMNILAUNCHER_AUTH_TOKEN`.
- **`make start` fails** — try `make stop` first, then `make start` again.
- **Windows: `make: command not found`** — install `make` (Chocolatey/Scoop) or run from MSYS2 / Git Bash.
- **Windows: `make logs` / smoke / e2e fail** — install **PowerShell 7+ (`pwsh`)** and ensure it's on `PATH`.
- **Windows: env var didn't take effect** — use PowerShell syntax `$env:NAME = "value"` (not `export`), and set it in the *same* shell that runs `make`.

### Documentation

More detail in [`docs/`](./docs) and in [`SOURCE_FILES_MANIFEST.md`](./SOURCE_FILES_MANIFEST.md).

### Contributing

PRs welcome. Please run `make test` (or at least `cargo test --lib` + `npm test`) before submitting, and keep new behavior covered by tests.

### Security

Report security issues per [`SECURITY.md`](./SECURITY.md). Don't open public issues for vulnerabilities.

### Building on OmniLauncher

If you're shipping a project that uses "omnilauncher" in its name (e.g. `omnilauncher-plugin-foo`), please add a note to your README clarifying that it isn't built by the OmniLauncher team and isn't affiliated with us.

---

**Project** [OmniLLM/OmniLauncher](https://github.com/OmniLLM/OmniLauncher) · **License** see [LICENSE](./LICENSE)
