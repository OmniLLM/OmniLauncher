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

### Configure AI

1. Press **Ctrl+,** to open Settings (or click the ⚙ gear icon)
2. **API Base URL** — e.g. `https://api.openai.com`, a local Ollama endpoint,
   or any OpenAI-compatible server
3. **API Key** — leave empty for local endpoints
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
│   │   ├── SettingsPanel.tsx     Inline settings (AI, theme, plugins, GitHub)
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
