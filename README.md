# OmniLauncher 2.0

> An AI-native, cross-platform application launcher — type anything, press **Ctrl+Shift+O**.

![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-blue)
![Rust](https://img.shields.io/badge/rust-2021_edition-orange)
![Tauri](https://img.shields.io/badge/tauri-v2-brightgreen)
![Version](https://img.shields.io/badge/version-2.0.0-informational)

---

## What is OmniLauncher?

OmniLauncher is an AI-native, cross-platform launcher built with Tauri v2 and Rust. Hit **Ctrl+Shift+O** to summon the bar, then type — short keyword queries are dispatched instantly to the right plugin, while natural-language input is routed to an AI that calls the same plugins as tools and returns the answer. One interface, every action.

---

## Features

### Plugins

| Emoji | Plugin | Trigger / Prefix | What it does |
|-------|--------|-----------------|--------------|
| 🔍 | **Web Search** | `g <query>` | Search Google |
| ▶️ | **Web Search** | `yt <query>` | Search YouTube |
| 🐙 | **Web Search** | `gh <query>` | Search GitHub |
| 🔍 | **Web Search** | *(bare query)* | Fallback Google search (score 30) |
| 🧮 | **Calculator** | `= <expr>` | Evaluate math expressions (`+`, `-`, `*`, `/`, `^`, parentheses); copies result |
| 📁📄 | **File Search** | `f <name>` or `open <name>` | Walk home dir (depth 5) for matching files/folders |
| 🚀 | **App Launcher** | *(bare query)* | Fuzzy match installed apps (`.desktop` on Linux, `.app` on macOS, Start Menu `.lnk` on Windows) |
| ⏻ | **System Commands** | `sys <cmd>` | `lock`, `sleep`, `shutdown`, `restart` — cross-platform shell commands |
| 💻 | **Shell** | `> <command>` | Run any shell command directly |
| 📋 | **Clipboard** | `cb <term>` | Search clipboard history (last 50 entries, deduped) |

### AI Features

- Natural-language queries are detected automatically and routed to the AI
- The AI calls plugins as **OpenAI-compatible function/tool calls**
- Multi-turn conversation with up to **10 turns** of context (`ConversationContext`)
- Works with any OpenAI-compatible endpoint (local or remote)
- Settings persist to `~/.config/omnilauncher/settings.json` and can be updated at runtime

---

## How It Works

OmniLauncher operates in two modes:

```
User input
    │
    ├─ Keyword query ──► PluginManager.query_all()
    │                        • Match plugin keyword prefix (e.g. "= ", "> ", "sys ", "cb")
    │                        • Or no-prefix plugins (app_launcher, web_search fallback)
    │                        • Results sorted by score, top 10 returned
    │
    └─ Natural language ─► AI Router
                              • Collect tool schemas from all plugins
                              • POST to /v1/chat/completions with tools
                              • Execute any returned tool_calls via PluginManager
                              • Return combined response to frontend
```

Each plugin implements the `Plugin` trait which exposes both a `query()` method (for keyword mode) and an optional `tool_schema()` + `execute_tool()` pair (for AI function-calling mode). The `PluginManager` collects all schemas via `all_tool_schemas()` and dispatches tool calls by matching `plugin.name()`.

Conversation history is maintained in `ConversationContext`, which stores `user`, `assistant`, and `tool` role messages and trims to the last `max_turns * 2` messages automatically.

---

## AI Native

### Natural Language Detection

`Router::is_natural_language()` applies a simple heuristic (no external model needed):

| Condition | Examples |
|-----------|---------|
| Contains English question/action words | `what`, `how`, `why`, `when`, `where`, `who`, `find`, `show`, `open`, `search`, `list`, `get` |
| Contains Chinese keywords | `帮`, `找`, `打开`, `搜索`, `显示`, `什么`, `怎么` |
| 4 or more whitespace-separated words | `send me the latest report` |
| Contains `?` or `？` | `where is my notes file?` |

Single-word inputs always bypass AI and go directly to keyword routing.

### Tool Calling Flow

1. Query classified as natural language → `Router::ai_route()` is invoked
2. All plugin `tool_schema()` values are collected and sent with the request
3. The AI responds with `tool_calls` (OpenAI function-calling format)
4. Each tool call is dispatched: `PluginManager::execute_tool(name, args)`
5. Tool results are merged into the final response content
6. Response returned to frontend via `ai_query` Tauri command

### Multi-Turn Context

`ConversationContext` (default `max_turns = 10`) stores the rolling conversation. Each `ai_query` Tauri command appends the user message before routing and the assistant reply after, so follow-up questions have full context. Call `clear_conversation` to reset.

---

## Getting Started

### Prerequisites

- **Rust** — install via [rustup.rs](https://rustup.rs)
- **Node.js 18+** and npm/pnpm/yarn
- **Tauri CLI v2** — `cargo install tauri-cli --version "^2"`
- **Linux only** — additional system libraries:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
                 patchelf libssl-dev libayatana-appindicator3-dev
```

### Clone & Run

```bash
git clone https://github.com/your-org/OmniLauncher.git
cd OmniLauncher

# Install dependencies
make install

# Development (hot-reload)
make dev

# Production build
make release

# Create installer packages
make bundle
```

### Run Tests

```bash
make test
```

### All Targets

```bash
make help
```

| Target | Description |
|--------|-------------|
| `dev` | Start dev server with hot reload (Vite + Tauri) |
| `build` | Build frontend + Tauri (debug) |
| `build-frontend` | Build frontend only (Vite) |
| `release` | Build release binary with optimizations |
| `bundle` | Create installer packages (MSI, NSIS, DMG, etc.) |
| `install` | Install frontend + Rust dependencies |
| `clean` | Remove build artifacts |
| `lint` | Run Clippy (Rust linter) |
| `format` | Format Rust + frontend code |
| `check` | Run TypeScript + Rust type checks |
| `test` | Run all tests |

---

## Configuration

Settings are stored at:

```
~/.config/omnilauncher/settings.json
```

| Field | Default | Description |
|-------|---------|-------------|
| `ai_base_url` | `"http://localhost:5000"` | Base URL of the OpenAI-compatible API |
| `ai_model` | `"auto"` | Model name passed to the API |
| `ai_api_key` | `""` | API key (Bearer token); leave empty for local endpoints |
| `theme` | `"dark"` | UI theme (`"dark"` or `"light"`) |
| `hotkey` | `"Alt+Space"` | Display label for the global hotkey |
| `max_results` | `10` | Maximum plugin results returned per query |

Example `settings.json`:

```json
{
  "ai_base_url": "https://api.openai.com",
  "ai_model": "gpt-4o",
  "ai_api_key": "sk-...",
  "theme": "dark",
  "hotkey": "Alt+Space",
  "max_results": 10
}
```

Settings can also be updated at runtime through the UI; changes are persisted immediately and the AI client is recreated with the new configuration.

---

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| **Ctrl+Shift+O** | Toggle launcher window (global) |
| **Ctrl+,** | Open settings panel |
| **Escape** | Clear query and results |
| **Ctrl+Enter** | Force AI mode for any query |
| **Enter** | Execute selected result / submit query |
| **↑/↓** | Navigate results |

---

## Architecture

```
OmniLauncher 2.0
├── src/                        React 18 + TypeScript frontend (Vite)
│   └── ...                     Communicates via @tauri-apps/api invoke()
│
└── src-tauri/                  Rust backend
    ├── src/
    │   ├── main.rs             Tauri entry point; registers 6 commands
    │   ├── settings.rs         Load/save AppSettings (JSON)
    │   ├── plugins/
    │   │   ├── mod.rs          Plugin trait, PluginManager, QueryResult
    │   │   ├── app_launcher.rs Platform-native app discovery
    │   │   ├── calculator.rs   Zero-dep recursive-descent math parser
    │   │   ├── clipboard.rs    In-memory clipboard history (50 entries)
    │   │   ├── file_search.rs  WalkDir home dir (max depth 5)
    │   │   ├── shell_plugin.rs Execute arbitrary shell commands
    │   │   ├── system_commands.rs Lock/sleep/shutdown/restart
    │   │   └── web_search.rs   Google / YouTube / GitHub URL builder
    │   └── ai/
    │       ├── client.rs       OpenAI-compatible HTTP client + SSE streaming
    │       └── router.rs       NL detection, tool-calling orchestration, ConversationContext
    └── Cargo.toml              Tauri v2, reqwest 0.12, tokio, walkdir, serde
```

**Key design points:**
- All plugin logic runs in async Rust; `PluginManager::query_all` fans out queries concurrently.
- The `AiClient` connects to any OpenAI-compatible endpoint; settings changes recreate the client at runtime.
- The frontend is a thin React layer — all business logic lives in Rust.
- Tauri commands: `search`, `ai_query`, `execute_result`, `get_settings`, `save_settings_cmd`, `clear_conversation`

---

## Legacy

The original **WPF/Windows-only** OmniLauncher (v1) is preserved in the `legacy/` directory for historical reference. It is not built as part of the Tauri v2 project.

---

*OmniLauncher 2.0 — built with Tauri v2 · Rust 2021 · React 18*
