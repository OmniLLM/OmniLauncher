# 🚀 OmniLauncher

> An AI-native, cross-platform launcher — hit **Ctrl+Shift+O**, type anything, get results instantly.

![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-blue)
![Rust](https://img.shields.io/badge/rust-2021_edition-orange)
![Tauri](https://img.shields.io/badge/tauri-v2-brightgreen)
![Version](https://img.shields.io/badge/version-2.0.0-informational)

---

## ✨ What is OmniLauncher?

OmniLauncher is a keyboard-driven launcher built with **Tauri v2 + Rust** and a **React** frontend. Press **Ctrl+Shift+O** from anywhere to summon it, then type:

- **A prefix command** → instant local results from a plugin, no network needed
- **`?` or `ai `** → switches to AI Chat mode, backed by any OpenAI-compatible model

One interface. 30+ plugins. Zero friction.

---

## 🖥️ Two-Mode UI

### 🔍 Launcher Mode (default)

The window starts as a compact **64 px tall** input bar that grows only when there are results. Designed to stay out of your way.

- Instant results — no AI latency, results appear as you type
- **Hint bar** shown when the input is empty: lists all available prefix commands at a glance
- Results expand below the bar; **↑/↓** to navigate, **Enter** to execute
- Press **Ctrl+,** to open the settings panel inline

```
╔══════════════════════════════════════╗
║  🔍  = 2+2                           ║  ← input bar (64px)
╠══════════════════════════════════════╣
║  🧮  4          Copy result          ║  ← results expand below
╚══════════════════════════════════════╝
```

### 🤖 AI Chat Mode

Triggered by typing **`?`** or **`ai `** at the start of your query. The window expands to **560 px** and becomes a chat-bubble conversation interface.

- Full multi-turn conversation (up to 10 turns of context)
- The AI calls plugins as tools automatically (e.g. runs shell commands, reads files, fetches URLs)
- Tool chips shown above each AI response: `📁 file_read`, `🔍 grep_search`, etc.
- **"New conversation"** button clears history; or press **Escape** then retype `?`
- Responses render Markdown: bold, italic, code blocks, tables, lists

```
╔══════════════════════════════════════╗
║  ✦ AI Chat              [New convo]  ║  ← top bar
╠══════════════════════════════════════╣
║  ┌──────────────────────────────┐    ║
║  │ what's using port 8080?      │    ║  ← user bubble
║  └──────────────────────────────┘    ║
║  📁 bash_exec                        ║
║  ┌────────────────────────────────┐  ║
║  │ Port 8080 is used by node.js  │  ║  ← AI bubble
║  │ (PID 12345)                   │  ║
║  └────────────────────────────────┘  ║
╠══════════════════════════════════════╣
║  ✦  ? ask me anything...             ║  ← input bar
╚══════════════════════════════════════╝
```

---

## ⌨️ Prefix Commands (Launcher Mode)

Type a prefix to route your query directly to a plugin. Results appear immediately — no AI involved.

| Prefix | Plugin | Example |
|--------|--------|---------|
| *(no prefix)* | 🚀 **App Launcher** — fuzzy search installed apps | `chrome` |
| *(no prefix)* | 🔍 **Web Search** — fallback Google/YouTube/GitHub | `rust async tutorial` |
| `= ` | 🧮 **Calculator** — math expressions, copies result | `= (2^10 + 5) * 3` |
| `> ` | 💻 **Shell** — run any shell command | `> git status` |
| `cb` | 📋 **Clipboard** — search clipboard history (last 50) | `cb github.com` |
| `sys ` | ⏻ **System Commands** — lock/sleep/shutdown/restart | `sys shutdown` |
| `bm ` | 🔖 **Browser Bookmarks** — Chrome & Edge bookmarks | `bm tauri docs` |
| `color ` | 🎨 **Color Picker** — convert hex/rgb/hsl/name | `color #ff6600` |
| `hosts ` | 🌐 **Hosts** — view/search system hosts file | `hosts localhost` |
| `net ` | 🌍 **Network** — IP, ping, DNS flush, ports, WiFi | `net ip` |
| `snip ` | 📋 **Snippets** — store/recall text snippets | `snip deploy cmd` |
| `env ` | 🔑 **Env Vars** — search & copy environment variables | `env PATH` |
| `todo ` | 📝 **Todo** — persistent todo list | `todo review PR #42` |
| `git ` | 🌿 **Git** — status/log/branch/diff/stash | `git log --oneline` |
| `timer ` | ⏱️ **Timer** — set countdown timers | `timer 25m` |
| `conv ` | 📐 **Unit Converter** — convert units | `conv 100 km to miles` |
| `ps ` | 🖥️ **Process Manager** — list/search processes | `ps node` |
| `settings ` | ⚙️ **Windows Settings** — quick-open 35+ settings pages | `settings bluetooth` |

---

## `/` Slash Commands

Type `/` for structured commands with tab-completion previews in the results list.

| Command | Shortcut | What it does |
|---------|----------|--------------|
| `/app <query>` | `/a` | Search & launch applications |
| `/run <cmd>` | `/r` | Execute a shell command |
| `/open <target>` | `/o` | Open app, file, or URL |
| `/find <name>` | `/f` | Search files by name (home dir, depth 5) |
| `/grep <pattern>` | `/g` | Search file contents with regex |
| `/cat <file>` | | Read and display a file |
| `/ls [path]` | | List directory contents |
| `/git [subcmd]` | | Run git commands |
| `/calc <expr>` | `/c` | Quick calculator |
| `/todo [text]` | `/t` | Manage todo list |
| `/web <query>` | `/w` | Search Google / YouTube / GitHub |
| `/ip` | | Show your public IP address |
| `/ports` | | Show listening network ports |
| `/ps` | | Top processes by CPU |
| `/kill <name\|PID>` | | Kill a process |
| `/env <var>` | | Get an environment variable |
| `/color <value>` | | Convert color formats |
| `/sys <action>` | | lock / sleep / shutdown / restart |
| `/clip [term]` | `/cb` | Search clipboard history |
| `/help` | `/?` | Show all available commands |

---

## 🤖 AI Mode — Prefix & Tool Calling

### Triggering AI Mode

| Prefix | Example |
|--------|---------|
| `?` | `? what's eating my disk space?` |
| `ai ` | `ai explain this Cargo.toml` |

Any query starting with either prefix bypasses all local plugins and goes straight to the AI.

### AI Tool Calling

The AI automatically calls plugins as tools and shows which tools it used. These tools are available to the AI:

| Tool | What the AI can do |
|------|--------------------|
| `shell_exec` | Run shell commands (PowerShell on Windows, bash on macOS/Linux) |
| `file_read` | Read file contents with optional line ranges |
| `file_write` | Write / create files (auto-creates directories) |
| `file_edit` | Find-and-replace text in files |
| `code_execute` | Run Python, JavaScript, Rust, Bash, or PowerShell snippets |
| `grep_search` | Regex search through file contents |
| `glob_files` | Find files by glob pattern (e.g. `**/*.rs`) |
| `list_dir` | List directory contents flat or recursive |
| `git_ops` | Run any git subcommand |
| `web_fetch` | Fetch a URL and strip to plain text |
| `web_search` | Search Google / YouTube / GitHub |
| `http_request` | Full HTTP client (GET/POST/PUT/DELETE with headers & body) |
| `sys_info` | CPU, memory, disk, processes, uptime, OS info |
| `todo_memory` | Persistent todos + save/read notes to `~/.omnilauncher/notes/` |

### Example Interactions

```
? what's using port 8080?
  → AI calls shell_exec { command: "lsof -i :8080" }  (macOS/Linux)
  → AI calls shell_exec { command: "netstat -an | findstr :8080" }  (Windows)

? find all TODO comments in src/
  → AI calls grep_search { pattern: "TODO", path: "src" }

? read the first 20 lines of Cargo.toml
  → AI calls file_read { path: "Cargo.toml", end_line: 20 }

? run this python: print(sum(range(100)))
  → AI calls code_execute { language: "python", code: "print(sum(range(100)))" }

? add 'review PR #42' to my todo list
  → AI calls todo_memory { action: "add", text: "review PR #42" }
```

The AI detects your OS automatically and adjusts shell syntax accordingly.

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| **Ctrl+Shift+O** | Toggle launcher window (global hotkey) |
| **Ctrl+,** | Open / close settings panel |
| **Escape** | Clear query and results |
| **Ctrl+Enter** | Force AI mode for any query |
| **Enter** | Execute selected result / send AI message |
| **↑ / ↓** | Navigate results |

---

## 🚀 Getting Started

### Prerequisites

- **Rust** — [rustup.rs](https://rustup.rs)
- **Node.js 18+** and npm/pnpm/yarn
- **Tauri CLI v2** — `cargo install tauri-cli --version "^2"`
- **Linux only** — system libraries:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
                 patchelf libssl-dev libayatana-appindicator3-dev
```

### Build & Run

```bash
git clone https://github.com/your-org/OmniLauncher.git
cd OmniLauncher

make install    # install frontend + Rust dependencies
make dev        # hot-reload dev mode (Vite + Tauri)
make release    # optimised production binary
make bundle     # installer packages (MSI, NSIS, DMG, …)
make test       # run all tests
```

| Target | Description |
|--------|-------------|
| `dev` | Start dev server with hot reload |
| `build` | Build frontend + Tauri (debug) |
| `release` | Build release binary |
| `bundle` | Create platform installers |
| `install` | Install all dependencies |
| `clean` | Remove build artifacts |
| `lint` | Run Clippy |
| `format` | Format Rust + frontend |
| `check` | TypeScript + Rust type checks |
| `test` | Run all tests |

### Configure AI

1. Press **Ctrl+,** to open Settings (or click the ⚙ gear icon)
2. Enter your **API Base URL** (e.g. `https://api.openai.com` or a local Ollama endpoint)
3. Enter your **API Key** (leave empty for local endpoints)
4. Pick a **Model** from the dropdown (auto-fetched from `/v1/models`)
5. Click **Save** — the AI client reconnects immediately

Settings are stored at `~/.config/omnilauncher/settings.json`:

```json
{
  "ai_base_url": "https://api.openai.com",
  "ai_model": "gpt-4o",
  "ai_api_key": "sk-...",
  "theme": "dark",
  "hotkey": "Ctrl+Shift+O",
  "max_results": 10
}
```

---

## 🏗️ Architecture

```
OmniLauncher
├── src/                        React 18 + TypeScript (Vite)
│   ├── App.tsx                 Two-mode UI, chat bubbles, markdown rendering
│   └── components/
│       ├── SearchBar.tsx       Input bar, hint bar, mode indicator
│       ├── ResultList.tsx      Keyboard-navigable result list
│       ├── SettingsPanel.tsx   Inline settings (AI config, theme)
│       └── AIResponsePane.tsx  AI response display
│
└── src-tauri/                  Rust backend
    └── src/
        ├── main.rs             Tauri entry point, 8 command handlers
        ├── lib.rs              create_plugin_manager() — registers all 32 plugins
        ├── settings.rs         Load/save AppSettings (JSON)
        ├── plugins/
        │   ├── mod.rs          Plugin trait, PluginManager, QueryResult
        │   └── *.rs            32 individual plugin implementations
        └── ai/
            ├── client.rs       OpenAI-compatible HTTP client
            └── router.rs       Tool-calling orchestration, ConversationContext
```

**Key design points:**
- All plugin logic is async Rust; `PluginManager::query_all` fans out concurrently
- Each plugin implements both `query()` (keyword mode) and optionally `tool_schema()` + `execute_tool()` (AI mode)
- The `AiClient` works with any OpenAI-compatible endpoint; settings changes recreate it at runtime
- The frontend is a thin React layer — all business logic lives in Rust
- Tauri commands: `search`, `ai_query`, `execute_result`, `slash_preview`, `get_settings`, `save_settings_cmd`, `clear_conversation`, `list_models`

---

## 🗂️ Legacy

The original **WPF/Windows-only** OmniLauncher (v1) lives in `legacy/` for historical reference. It is not built as part of the Tauri v2 project.

---

*Built with Tauri v2 · Rust 2021 · React 18*
