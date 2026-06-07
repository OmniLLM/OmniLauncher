# 🚀 OmniLauncher

> An AI-native, cross-platform launcher — hit **Alt+Space**, type anything, get results instantly.

![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-blue)
![Rust](https://img.shields.io/badge/rust-2021_edition-orange)
![Tauri](https://img.shields.io/badge/tauri-v2-brightgreen)
![Version](https://img.shields.io/badge/version-2.0.0-informational)

![OmniLauncher](omni-initial.png)

---

## ✨ What is OmniLauncher?

OmniLauncher is a keyboard-driven launcher built with **Tauri v2 + Rust** and a
**React 18 + TypeScript** frontend. Press **Alt+Space** from anywhere to
summon it, then type:

- **A prefix command** → instant local results from one of 45+ built-in plugins
- **`?` or `ai `** → switches to AI Chat mode, backed by any OpenAI-compatible model
- **`/something`** → structured slash command with tab-completion preview
- **`plugins`** or **`skills`** → built-in management panels for external plugins / AI skills
- Selected text in any app → auto-pre-filled into the launcher for contextual actions

One interface. 45+ plugins. Optional Raycast / Flow Launcher extension support.
Zero friction.

---

## 🖥️ Two-Mode UI

### 🔍 Launcher Mode (default)

The window starts as a compact input bar that grows only when there are
results. Designed to stay out of your way.

- Instant results — no AI latency, results appear as you type
- **Hint bar** when the input is empty: lists available prefix commands
- Results expand below the bar; **↑/↓** to navigate, **Enter** to execute
- Window auto-resizes between **56 px** (just the bar) and **1200 px** (full panel)
- Width is **⅓ of screen** in launcher mode, **½ of screen** in AI or panel mode
- Press **Ctrl+,** to open the inline settings panel
- Click & drag the launcher anywhere — position is persisted to
  `~/.config/omnilauncher/window-pos.json`

### 🤖 AI Chat Mode

Triggered by typing **`?`** or **`ai `** at the start of your query. The window
becomes a chat-bubble conversation interface.

- Full multi-turn conversation, persisted to SQLite across restarts
- Multiple named sessions — switch, rename, delete via the session menu
- The AI calls plugins as tools automatically (shell, files, web, scheduler, …)
- Tool chips above each AI response: `📁 file_read`, `🔍 grep_search`, etc.
- **"New conversation"** button starts a fresh session
- Markdown rendering: bold, italic, code blocks, tables, lists
- Cancel an in-flight AI request with the stop button (calls `ai_cancel`)

---

## ⌨️ Built-in Plugins & Prefix Commands

Plugins registered by `create_plugin_manager()` in `src-tauri/src/lib.rs`.
Type a prefix to route directly to the plugin; results appear immediately with
no AI involved.

### Productivity & Files

| Prefix | Plugin | Example |
|--------|--------|---------|
| *(no prefix)* | 🚀 **App Launcher** — fuzzy search installed apps | `chrome` |
| `= ` | 🧮 **Calculator** | `= (2^10 + 5) * 3` |
| `> ` | 💻 **Shell** — run any shell command | `> git status` |
| `cb` | 📋 **Clipboard** — last 50 clipboard entries | `cb github.com` |
| `snip ` | 📋 **Snippets** — store/recall text snippets | `snip deploy cmd` |
| `todo ` | 📝 **Todo** — persistent todos (SQLite) | `todo review PR #42` |
| `timer ` | ⏱️ **Timer** — countdown timers | `timer 25m` |
| `pomo` | 🍅 **Pomodoro** — work / short / long break sessions | `pomo start` |
| `sched` | 🗓️ **Scheduler** — interval & cron jobs that run scripts | `sched list` |
| `cron ` | 🧠 **Cron Explainer** — natural-language cron reading | `cron */5 9-17 * * 1-5` |
| `conv ` | 📐 **Unit Converter** | `conv 100 km to miles` |
| `color ` | 🎨 **Color Picker** — hex/rgb/hsl/name convert | `color #ff6600` |
| `emoji ` | 😀 **Emoji Picker** — fuzzy emoji search | `emoji rocket` |

### Search & Files

| Prefix | Plugin | Example |
|--------|--------|---------|
| *(no prefix)* | 🔍 **Web Search** — fallback Google/YouTube/GitHub | `rust async tutorial` |
| `bm ` | 🔖 **Browser Bookmarks** — Chrome & Edge | `bm tauri docs` |
| *(none)* | 📁 **File Search / Glob / Grep / Ls / File Read / Write** — AI tools (no prefix) | — |

### System

| Prefix | Plugin | Example |
|--------|--------|---------|
| `sys ` | ⏻ **System Commands** — lock/sleep/shutdown/restart | `sys shutdown` |
| `ps ` | 🖥️ **Process Manager** | `ps node` |
| `net ` | 🌍 **Network** — IP, ping, DNS flush, ports, WiFi | `net ip` |
| `hosts ` | 🌐 **Hosts file** | `hosts localhost` |
| `env ` | 🔑 **Env Vars** | `env PATH` |
| `git ` | 🌿 **Git** — status/log/branch/diff/stash | `git log --oneline` |
| `gh ` | 🐙 **GitHub** — repos / issues / PRs across servers | `gh issues my-repo` |
| `settings ` | ⚙️ **Windows Settings** — quick-open 35+ pages | `settings bluetooth` |
| `resize ` | 🪟 **Window Resize / Tile** — left half, top right, … | `resize left half` |
| `ss` | 📸 **Screenshot** — list / new / full screen | `ss new` |

### AI-Augmented Workflows

| Prefix | Plugin | Example |
|--------|--------|---------|
| `vq ` / `vision ` | 👁️ **Vision Analyze** — capture region + ask AI | `vq what does this error mean?` |
| `sel ` *(or auto-captured selection)* | ✂️ **Selection Actions** — translate, ask AI, search, copy | `sel some highlighted text` |
| *(via `@agent_name`)* | 🤝 **Agent Delegate** — hand off to Claude / Codex / OmniCode / OpenCode CLIs | `@claude refactor this module` |

### Management Panels

| Type | Opens |
|------|-------|
| `plugins` or `pm` | **Plugin Manager** — install / update / remove external plugins from Git or local paths |
| `skills` | **Skill Manager** — install / update / remove AI skills (Markdown skill docs from `assets/skills/` or `~/.omnilauncher/skills/`) |

---

## `/` Slash Commands

Type `/` for structured commands with tab-completion previews. Implemented in
`Router::slash_command` (`src-tauri/src/ai/router.rs`).

| Command | Shortcut | What it does |
|---------|----------|--------------|
| `/app <query>` | `/a` | Launch best-matching application |
| `/run <cmd>` | `/r` | Execute a shell command instantly |
| `/open <target>` | `/o` | Open app, file, or URL with the OS opener |
| `/find <name>` | `/f` | Search files by name |
| `/grep <pattern> [path]` | `/g` | Regex search through file contents |
| `/cat <file>` | | Read a file |
| `/ls [path]` | | List a directory |
| `/git [subcmd]` | | Run git subcommands |
| `/calc <expr>` | `/c` | Quick calculator |
| `/todo [text]` | `/t` | Manage todo list |
| `/web <query>` | `/w` | Search Google / YouTube / GitHub |
| `/ip` | | Show your public IP |
| `/ports` | | List listening network ports |
| `/ps` | | Top processes by CPU |
| `/kill <name\|PID>` | | Kill a process |
| `/env <var>` | | Read an environment variable |
| `/color <value>` | | Convert color formats |
| `/sys <action>` | | lock / sleep / shutdown / restart |
| `/clip [term]` | `/cb` | Search clipboard history |
| `/skill <name>` | | Invoke a registered AI skill |
| `/help` | `/?` | Show all available slash commands |

---

## 🤖 AI Mode — Tool Calling

### Triggering AI Mode

| Prefix | Example |
|--------|---------|
| `?` | `? what's eating my disk space?` |
| `ai ` | `ai explain this Cargo.toml` |
| `Ctrl+Enter` | force AI mode for the current query |

Any query starting with either prefix bypasses local plugins and goes straight
to the AI router (`src-tauri/src/ai/router.rs`).

### Tools exposed to the AI

The AI is allowed to call any plugin that returns a `tool_schema()`. The full
catalogue at the time of writing:

| Tool | What the AI can do |
|------|--------------------|
| `shell_exec` | Run shell commands (PowerShell on Windows, bash elsewhere) |
| `file_read` | Read file contents with optional line ranges |
| `file_write` | Write / create files (auto-creates parent dirs) |
| `file_edit` | Find-and-replace text in files |
| `file_search` | Find files by name |
| `glob_files` | Find files by glob pattern (`**/*.rs`) |
| `grep_search` | Regex search through file contents |
| `list_dir` | List directory contents (flat or recursive) |
| `code_execute` | Run Python / JS / Rust / Bash / PowerShell snippets |
| `git_ops` | Run any git subcommand |
| `github` | Repos, issues, PRs across configured GitHub servers (incl. GHE) |
| `web_fetch` | Fetch a URL and strip to plain text |
| `web_search` | Search Google / YouTube / GitHub |
| `http_request` | Full HTTP client (GET/POST/PUT/DELETE) |
| `open_url` | Open URL / file in the OS default handler |
| `sys_info` | CPU, memory, disk, processes, uptime, OS |
| `network` | IP, ping, DNS, port checks |
| `process_manager` | List / kill processes |
| `system_commands` | lock / sleep / shutdown / restart |
| `system_settings` | Quick-open Windows Settings pages |
| `app_launcher` | Launch an installed application |
| `browser_bookmarks` | Search Chrome / Edge bookmarks |
| `clipboard_search` | Search clipboard history |
| `snippets` | Save / recall text snippets |
| `todo_memory` | Persistent todos + notes in `~/.omnilauncher/notes/` |
| `env_vars` | Read environment variables |
| `hosts` | View / search system hosts file |
| `calculator` | Evaluate math expressions |
| `convert_unit` | Unit conversions |
| `color_picker` | Convert color formats |
| `emoji_picker` | Find an emoji by keyword |
| `set_timer` | Set a countdown timer |
| `pomodoro` | Start / stop / status a Pomodoro session |
| `scheduler` | Add / list / remove scheduled jobs |
| `cron_explainer` | Explain a cron expression |
| `translate_text` | Translate text between languages |
| `window_resize` | Tile / move / resize windows |
| `take_screenshot` | Capture screen or region |
| `act_on_selection` | Operate on the captured text selection |
| `run_user_script` | Run a script from `~/.omnilauncher/scripts/` |
| `delegate_to_agent` | Hand off a sub-task to Claude / Codex / OmniCode / OpenCode |

External plugins and Raycast / Flow extensions that declare a `tool_schema`
are added on top of this list at startup.

### Example interactions

```
? what's using port 8080?
  → shell_exec { command: "lsof -i :8080" }                 (macOS/Linux)
  → shell_exec { command: "netstat -an | findstr :8080" }   (Windows)

? find all TODO comments in src/
  → grep_search { pattern: "TODO", path: "src" }

? read the first 20 lines of Cargo.toml
  → file_read { path: "Cargo.toml", end_line: 20 }

? run this python: print(sum(range(100)))
  → code_execute { language: "python", code: "print(sum(range(100)))" }

? add 'review PR #42' to my todo list
  → todo_memory { action: "add", text: "review PR #42" }

? schedule "git pull" every 30 minutes in ~/code/repo
  → scheduler { action: "add", schedule: "interval:1800", command: "git pull", cwd: "~/code/repo" }
```

The AI detects the host OS automatically and chooses the right shell syntax.

---

## 🌐 Live Dashboard

A read-only HTTP dashboard runs on **http://127.0.0.1:1421** while the app is
open. Useful pages:

| Path | Shows |
|------|-------|
| `/dashboard` | Overview (everything below at a glance) |
| `/dashboard/todos` | All persistent todos |
| `/dashboard/conversation` | Full AI conversation history |
| `/dashboard/jobs` | Scheduler — past runs and upcoming triggers |
| `/dashboard/tables` | Raw SQLite tables |
| `/dashboard/github` | Repos / issues / PRs across configured servers |

Each page is also available as a JSON endpoint at `<page>/data`.

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| **Alt+Space** | Toggle launcher window (global hotkey — configurable via `hotkey` in settings) |
| **Ctrl+,** | Open / close settings panel |
| **Escape** | Clear query / close panel |
| **Ctrl+Enter** | Force AI mode for the current query |
| **Enter** | Execute selected result / send AI message |
| **Tab** | Auto-complete slash command |
| **↑ / ↓** | Navigate result list |

The system-tray icon is a click-fallback for the hotkey.

---

## 🚀 Getting Started

### Prerequisites

- **Rust** — [rustup.rs](https://rustup.rs)
- **Node.js 18+** and npm
- **Tauri CLI v2** — `cargo install tauri-cli --version "^2"` (or use `npx tauri`)
- **Linux only** — system libraries:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
                 patchelf libssl-dev libayatana-appindicator3-dev
```

Optional runtime helpers (auto-detected when present):

- `gh` — auto-discover authenticated GitHub servers
- `wmctrl` (Linux) — window resize plugin
- `scrot` / `gnome-screenshot` / `import` (Linux) — screenshot plugin
- `tesseract` — OCR for the screenshot plugin
- `python3`, `node`, `dotnet` — needed by external plugins that use them

### Build & Run

```bash
git clone https://github.com/your-org/OmniLauncher.git
cd OmniLauncher

make install        # npm install + cargo fetch
make dev            # hot-reload dev mode (Vite + Tauri)
make release        # optimised production binary
make bundle         # installer packages (MSI, NSIS, DMG, …)
make test           # run all tests (single-threaded)
```

| Target | Description |
|--------|-------------|
| `dev` | Start dev server (Vite + Tauri) with hot reload |
| `dev-debug` | Same as `dev` with verbose file logging (`--debug`) |
| `prod` | Build release and start the binary |
| `prod-debug` | Build release and start with `--debug` logging |
| `restart` | Restart the running production app (`REBUILD=1` to rebuild first) |
| `restart-rebuild` | Shortcut for `restart REBUILD=1` |
| `build` | Build frontend + Tauri (debug) |
| `build-frontend` | Build frontend only (Vite) |
| `release` | Build release binary (no bundle) |
| `bundle` | Create platform installers |
| `install` / `install-deps` | npm install + cargo fetch |
| `clean` | Remove `dist`, `node_modules`, `src-tauri/target` |
| `lint` | `cargo clippy -- -D warnings` |
| `format` | Prettier + `cargo fmt` |
| `check` | `tsc --noEmit` + `cargo check` |
| `test` | `cargo test -- --test-threads=1` |
| `status` | Show app status (`scripts/status.{sh,ps1}`) |
| `logs` | Tail the debug log live (`scripts/logs.{sh,ps1}`) |

> The Makefile defaults to PowerShell (`SHELL := pwsh`) because the primary
> dev target is Windows. On Linux/macOS use the underlying `npm run`,
> `cargo`, and `npx tauri` commands directly, or run individual targets via
> `make -f Makefile <target>` after switching `SHELL` to `bash`.

### Server/frontend production

You can also deploy the headless server + frontend architecture in production.

#### Backend service (WSL / Linux)

```bash
make backend-prod
```

This runs the Rust backend in release mode with `--server`.

You can override bind settings:

```bash
make backend-prod SERVER_HOST=0.0.0.0 SERVER_PORT=15000
```

#### Frontend build (Windows or anywhere)

```bash
make frontend-prod FRONTEND_BACKEND_URL=http://your-backend-host:1422
```

This produces a static web build in `dist/` configured to talk to the headless server backend.

#### Local production preview

```bash
make serve-frontend-prod FRONTEND_BACKEND_URL=http://127.0.0.1:1422
```

That serves the production frontend locally using Vite preview.

In a real deployment, you can instead serve `dist/` from nginx, Caddy, IIS, or any static file host.


You can now run the backend and frontend separately:

#### 1. Run backend in WSL / Linux

```bash
make backend-dev
```

This starts the Rust backend-only API server on `0.0.0.0:1422` by default.
Override with:

```bash
make backend-dev SERVER_HOST=0.0.0.0 SERVER_PORT=15000
```

#### 2. Run frontend in Windows

In a Windows terminal from the same repo:

```bash
make frontend-dev FRONTEND_BACKEND_URL=http://<wsl-host-or-localhost>:1422
```

If Windows can already reach WSL through localhost port forwarding, this is enough:

```bash
make frontend-dev FRONTEND_BACKEND_URL=http://127.0.0.1:1422
```

The browser frontend uses `VITE_OMNILAUNCHER_BACKEND_URL` to talk to the backend over HTTP/SSE instead of Tauri IPC.

#### Server-dev notes

- Integrated desktop development still works with `make dev`
- Server mode is intended for launcher UI + backend logic development without Tauri coupling during iteration
- Some desktop-only capabilities remain Tauri-only in browser mode, such as screenshot/vision flows and native window behavior

### 🔐 Cross-machine deployment (Token auth)

Whenever the `--server` backend runs on a different machine than the frontend
(WSL backend ↔ Windows shell, Docker backend ↔ desktop, remote VM ↔ laptop),
the shell can't read the per-launch token file the backend writes locally and
auth will fail. The cross-machine flow is built on a single shared secret you
pin on both sides.

#### ⚠️ Two distinct credentials in Settings — don't mix them up

OmniLauncher's Settings panel has **two separate password-style fields**.
They go to completely different places and you almost always want to leave
one of them empty:

| Field in Settings | Underlying key | What it is | Sent to | When to set |
|---|---|---|---|---|
| **API Key** | `ai_api_key` | Your **LLM provider** key (OpenAI / Anthropic-compat / etc.) | `ai_base_url` (third-party AI endpoint) | When using a paid / hosted LLM provider. Leave empty for local Ollama / LM Studio. |
| **Backend Token** | `backend_token` | OmniLauncher's **own backend** auth token | `backend_url` (your `--server` instance) | **Only** when frontend and backend run on different machines. Leave empty for the default single-process Tauri build. |

Symptom of confusing them: pasting your OpenAI key into Backend Token → every
launcher request gets HTTP 401 from your own backend; pasting your backend
token into API Key → your LLM provider returns 401/invalid-key on AI mode.

#### Authentication model (Backend Token)

- Every backend request must carry the token, either as
  `X-OmniLauncher-Token: <token>` (canonical) **or** `Authorization: Bearer <token>`
  (so plain `curl` / browser fetch / scripted clients work without a custom header).
- Exempt endpoints: `OPTIONS *` (CORS preflight) and `GET /health`.
- The token is resolved with the same three-tier precedence on both ends:
  1. `OMNILAUNCHER_AUTH_TOKEN` environment variable
  2. `backend_token` field in `~/.config/omnilauncher/settings.json` (UI: Settings → General → Backend Token)
  3. `~/.config/omnilauncher/server-token` file (same-machine fallback that
     the backend writes at startup — **only meaningful when shell and backend
     share a filesystem**)

When `OMNILAUNCHER_AUTH_TOKEN` is set on the backend, the `--server` process
pins to it and **skips overwriting** the local token file. That's how both
ends end up agreeing on the same value.

#### Backend (WSL / Linux) — pin a stable token

```bash
# Generate once, reuse forever
export OMNILAUNCHER_AUTH_TOKEN=$(openssl rand -hex 32)
export OMNILAUNCHER_SERVER_HOST=0.0.0.0          # default; allow non-loopback
export OMNILAUNCHER_SERVER_PORT=1422             # default

# Persist to ~/.bashrc / ~/.zshrc so it survives reboots
echo "export OMNILAUNCHER_AUTH_TOKEN=$OMNILAUNCHER_AUTH_TOKEN" >> ~/.bashrc

make start-backend    # or: target/release/omnilauncher-backend --server
echo "Share this token with the frontend: $OMNILAUNCHER_AUTH_TOKEN"
```

> If you forget to set the env var, the backend will generate a random token
> on every launch and write it to `~/.config/omnilauncher/server-token` —
> fine for same-machine, useless for cross-machine because the value changes
> every restart.

#### Frontend (Windows desktop shell) — two ways

**Option A — Settings UI (recommended, no restart required)**

1. Start the Tauri shell once (it'll fail auth, that's fine)
2. Press **Ctrl+,** → **General** tab
3. **Backend URL** → `http://<wsl-host>:1422` (e.g. `http://172.17.0.1:1422` or
   whatever `ip addr show eth0` reports in WSL; `http://127.0.0.1:1422` also
   works if you have WSL2 `localhostForwarding`)
4. **Backend Token** → paste the value from the backend export above
5. **Save** → close & reopen the shell window so the values get injected
   into `window.__OMNILAUNCHER_BACKEND_URL__` / `window.__OMNILAUNCHER_TOKEN__`

**Option B — Environment variables (CI / scripted / portable)**

```powershell
$env:OMNILAUNCHER_BACKEND_URL = "http://<wsl-host>:1422"
$env:OMNILAUNCHER_AUTH_TOKEN  = "<token-from-backend>"
.\target\release\omnilauncher.exe
```

Env values always win over `settings.json`, so this overrides whatever's in
the UI without modifying it.

#### Browser frontend (static build)

When you ship the `make frontend-prod` static bundle, the served HTML needs
the token injected before the first request. Two patterns:

- Reverse proxy injects `window.__OMNILAUNCHER_TOKEN__ = "..."` into the
  served `index.html` per-session
- The user pastes the token into Settings → Backend Token once; `runtime.ts`
  falls back to the saved settings value

#### Scripted / `curl` access

```bash
TOKEN=$(cat ~/.config/omnilauncher/server-token)   # same-machine
# or just hardcode the pinned value cross-machine

# Custom header (canonical)
curl -H "X-OmniLauncher-Token: $TOKEN" http://wsl-host:1422/dashboard/data

# Standard Bearer (works the same — useful for tools that don't allow custom headers)
curl -H "Authorization: Bearer $TOKEN" http://wsl-host:1422/dashboard/data

# Health probe needs no auth
curl http://wsl-host:1422/health
```

The bundled `scripts/smoke-endpoints.sh` resolves the token via
`--token` arg → `OMNILAUNCHER_AUTH_TOKEN` env → local file, in that order.

#### Threat model & caveats

- **No TLS.** Path C currently assumes the frontend ↔ backend hop is trusted
  (same physical host via loopback / WSL vEthernet, or a private LAN). Don't
  expose `OMNILAUNCHER_SERVER_HOST=0.0.0.0` to the public internet without
  putting nginx/Caddy in front for HTTPS termination + token rate-limiting.
- **Token comparison is not constant-time.** Acceptable for now (single
  secret, no online brute force given the 256-bit entropy when generated
  from `openssl rand -hex 32`); revisit if you reduce token length.
- **Don't commit your token.** It belongs in env / settings / a vault, never
  in repo files or screenshots.

---

## ⚙️ AI Provider Configuration

> Configures the **third-party LLM** (OpenAI / Ollama / any OpenAI-compatible
> server) the launcher talks to in AI mode. This is **separate** from the
> `backend_token` used by the OmniLauncher backend itself — see
> [🔐 Cross-machine deployment](#-cross-machine-deployment-token-auth)
> if you also need to configure that.

1. Press **Ctrl+,** to open Settings (or click the ⚙ gear icon)
2. **API Base URL** — e.g. `https://api.openai.com`, a local Ollama endpoint,
   or any OpenAI-compatible server
3. **API Key** — your **LLM provider's** key (OpenAI sk-…, Anthropic, etc.).
   Leave empty for local endpoints like Ollama / LM Studio. **Not** the same
   field as Backend Token.
4. **Model** — pick from the dropdown (auto-fetched from `/v1/models`)
5. **Save** — the `AiClient` is rebuilt at runtime, no restart needed

Settings are stored at `~/.config/omnilauncher/settings.json`:

```json
{
  "ai_base_url": "http://localhost:5000",
  "ai_model": "auto",
  "ai_api_key": "",
  "theme": "system",
  "hotkey": "Alt+Space",
  "max_results": 10,
  "background_url": "",
  "backend_url": "",
  "backend_token": "",
  "plugin_dirs": [],
  "github_servers": [
    {
      "hostname": "github.com",
      "api_base": "",
      "token": "",
      "orgs": ["my-org"]
    }
  ],
  "capture_selection_on_open": false
}
```

> `backend_url` / `backend_token` are only needed for the **cross-machine**
> deployment path (e.g. WSL backend + Windows shell). See [🔐 Cross-machine
> deployment (Token auth)](#-cross-machine-deployment-token-auth) above. Both
> can be left empty for the default integrated Tauri build.
>
> **`backend_token` ≠ `ai_api_key`.** The former authenticates against the
> OmniLauncher backend; the latter is your LLM provider key. The Settings UI
> labels them **Backend Token** and **API Key** respectively.


> The `hotkey` field in `~/.config/omnilauncher/settings.json` controls the
> global shortcut. Default is **Alt+Space**. Tokens are `+`-separated and
> case-insensitive — modifiers: `ctrl`/`control`, `shift`, `alt`/`option`,
> `cmd`/`command`/`super`/`meta`/`win`; key: letters `A-Z`, digits `0-9`,
> `Space`/`Enter`/`Escape`/`Tab`/`Backspace`/`F1`–`F12`. If parsing fails the
> launcher logs a warning and falls back to **Ctrl+Shift+O** so the window is
> always reachable.

GitHub token resolution order for each server:

1. explicit `token` field in `settings.json`
2. `gh auth token --hostname <host>` (works with keyring & file storage)
3. `oauth_token` parsed from `~/.config/gh/hosts.yml`

When no servers are configured, OmniLauncher auto-discovers any host you are
already logged into with `gh`.

---

## 🧩 Extending OmniLauncher

### External plugins

Drop a plugin directory under `~/.omnilauncher/plugins/<name>/` (or any path
listed in `settings.plugin_dirs`). A minimal plugin is:

```
~/.omnilauncher/plugins/my-plugin/
├── plugin.json          ← manifest
└── run.sh               ← any executable (Python, Node, shell, …)
```

`plugin.json`:

```json
{
  "name": "my-plugin",
  "description": "What it does",
  "version": "1.0.0",
  "keyword": "mp ",
  "icon": "🧩",
  "entry": "run.sh",
  "entry_windows": "run.ps1",
  "tool_schema": null
}
```

If `tool_schema` is provided (OpenAI function-calling format), the plugin is
also exposed as a tool to the AI. See `docs/external-plugins.md` for the full
stdin/stdout protocol.

Install / update / remove plugins from the UI by typing `plugins` (or `pm`).

### Raycast & Flow Launcher extensions

OmniLauncher auto-detects and adapts a useful subset of:

- **Raycast** extensions (anything with `@raycast/api` in `package.json` and a
  `commands` array). View-style commands degrade to printing captured
  side-effects (toast / HUD / clipboard).
- **Flow.Launcher** plugins (anything with `ExecuteFileName`, `Language`,
  and `ID`/`Name` in `plugin.json`). Python, JS, and C#/F# plugins are
  supported; the shim handles JSON-RPC translation.

Just install the upstream extension into `~/.omnilauncher/plugins/` (clone or
copy) — the adapters take care of the rest.

### AI Skills

Skills are short Markdown documents the AI is allowed to read on demand. They
ship in `assets/skills/` (bundled) and `~/.omnilauncher/skills/` (user).
Built-in skills include: `code-helper`, `dashboard-assistant`,
`file-organizer`, `system-helper`, `translator`, `web-summarizer`,
`windows-expert`, `windows-sysinternals`.

Manage them with the `skills` panel or `/skill <name>`.

### User scripts

Scripts dropped into `~/.omnilauncher/scripts/` are surfaced as first-class
launcher actions. Metadata is read from leading comments:

```bash
#!/usr/bin/env bash
# name: My Script
# icon: 🚀
# desc: Does something useful
echo "Hello"
```

Supports `.sh`, `.py`, `.js`, `.ps1` (and `.bat`/`.cmd` on Windows).

---

## 🏗️ Architecture

```
OmniLauncher
├── src/                          React 18 + TypeScript (Vite 8)
│   ├── App.tsx                   Two-mode UI + chat bubbles + markdown
│   ├── main.tsx                  Entry point
│   ├── components/
│   │   ├── SearchBar.tsx         Input bar + hint bar + mode indicator
│   │   ├── ResultList.tsx        Keyboard-navigable result list
│   │   ├── SettingsWindow.tsx    Standalone settings window
│   │   ├── AIResponsePane.tsx    Chat-bubble pane
│   │   ├── FormattedSubtitle.tsx Markdown-ish subtitle renderer
│   │   ├── FavoritesList.tsx     Pinned actions
│   │   ├── PluginManager.tsx     Install / update / remove plugins
│   │   └── SkillManager.tsx      Install / update / remove skills
│   └── utils/
│       ├── aiPrefix.ts           `?` / `ai ` detection
│       └── markdown.ts           Lightweight Markdown renderer
│
└── src-tauri/                    Rust backend
    ├── Cargo.toml
    ├── migrations/               SQLite schema (todos, jobs, conversation …)
    ├── assets/                   Raycast + Flow shim assets
    └── src/
        ├── main.rs               Tauri entry, command handlers, tray, hotkey
        ├── lib.rs                create_plugin_manager() — registers built-ins
        ├── settings.rs           AppSettings + GitHub server discovery
        ├── path_config.rs        Cross-platform data / config paths
        ├── guardrails.rs         Scheduler safety net
        ├── live_server.rs        Embedded HTTP server (port 1421)
        ├── python_installer.rs   Bundled-Python helper for plugin runtimes
        ├── ai/
        │   ├── client.rs         OpenAI-compatible HTTP client
        │   ├── router.rs         Tool-calling orchestration, slash commands
        │   ├── errors.rs
        │   └── mod.rs
        ├── db/
        │   ├── mod.rs            Migration runner
        │   └── conversation.rs   Persistent multi-session chat
        ├── dashboard/            Live HTML/JSON dashboard pages
        ├── plugins/
        │   ├── mod.rs            Plugin trait + PluginManager + QueryResult
        │   ├── external.rs       Manifest-driven external plugin loader
        │   ├── raycast.rs        Raycast extension adapter
        │   ├── flow.rs           Flow.Launcher plugin adapter
        │   └── *.rs              45+ built-in plugins (see lib.rs)
        └── skills/               AI skill loader (SKILL.md frontmatter)
```

### Key design points

- All plugin logic is async Rust; `PluginManager::query_all` fans out
  concurrently and returns merged, scored results.
- Each plugin implements `Plugin::query()` (keyword mode) and optionally
  `tool_schema()` + `execute_tool()` (AI mode). They are completely
  independent — adding one is a single `pm.register(...)` line.
- External plugins are registered last with `register_override`, so a
  user-supplied plugin can replace a built-in by reusing its name or keyword.
- The `AiClient` is hot-swappable: saving settings recreates it without
  restarting the app.
- AI conversations are persisted to SQLite (`migrations/005`, `006`) so
  follow-up questions survive a restart and can be browsed in the dashboard.
- The scheduler runs inside the Tokio runtime; jobs are guarded by
  `guardrails.rs` against runaway processes.
- The frontend is intentionally thin — every business decision lives in Rust
  and is exposed via Tauri commands.

### Tauri command surface (`src-tauri/src/main.rs`)

`search`, `ai_query`, `ai_cancel`, `execute_result`, `slash_preview`,
`execute_slash_command`, `get_settings`, `save_settings_cmd`,
`clear_conversation`, `list_ai_sessions`, `current_ai_session`,
`switch_ai_session`, `delete_ai_session`, `list_models`, `list_skills`,
`reload_skills`, `install_skill`, `delete_skill`, `update_skill`,
`set_window_geometry`, `save_window_position`, `install_plugin`,
`update_plugin`, `update_plugin_collection`, `list_plugins`, `remove_plugin`,
`vision_analyze`, `list_plugin_runtime_dependencies`,
`install_plugin_runtime_dependency`, `install_python_command`,
`check_bundled_python`.

---

*Built with Tauri v2 · Rust 2021 · React 18 · Vite 8*
