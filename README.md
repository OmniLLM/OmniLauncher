# OmniLauncher 2.0

A cross-platform, AI-native application launcher built with **Tauri v2** + **Rust** backend and **React + TypeScript** frontend.

> Legacy Windows/WPF version is preserved in [`legacy/`](./legacy/)

---

## ✨ Features

- **Plugin System** — extensible Rust trait-based plugin architecture
- **App Launcher** — indexes installed apps (Linux `.desktop`, macOS `.app`, Windows Start Menu)
- **Web Search** — prefix `g ` for Google, `yt ` for YouTube, `gh ` for GitHub
- **Calculator** — prefix `= ` to evaluate math expressions (`= (2+3)*4^2`)
- **File Search** — prefix `f ` or `open ` to find files in your home directory
- **Shell Runner** — prefix `> ` to run any shell command
- **System Commands** — prefix `sys ` for lock/sleep/shutdown/restart
- **AI Router** — natural language queries automatically routed to an OpenAI-compatible LLM with tool calling
- **Catppuccin Mocha** dark theme (+ light theme toggle)
- **Keyboard-first** — arrow keys, Enter, Escape, Ctrl+,

---

## 🚀 Getting Started

### Prerequisites

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Node 18+
# macOS: brew install node
# Linux: sudo apt install nodejs npm

# Linux system deps (Ubuntu/Debian)
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
  libssl-dev libappindicator3-dev libglib2.0-dev
```

### Build & Run

```bash
# Install frontend deps
npm install

# Development (hot reload)
npm run tauri dev

# Production build
npm run tauri build
```

### Run tests (Rust only, no GUI required)

```bash
source "$HOME/.cargo/env"
cargo test --manifest-path src-tauri/Cargo.toml
```

---

## 🔌 Plugins

| Prefix | Plugin | Example |
|--------|--------|---------|
| `g `  | Google Search | `g rust async await` |
| `yt ` | YouTube Search | `yt lofi hip hop` |
| `gh ` | GitHub Search | `gh tauri examples` |
| `= `  | Calculator | `= sqrt(16) + 2^8` |
| `f `  | File Search | `f resume.pdf` |
| `open ` | File Search | `open config.toml` |
| `> `  | Shell Command | `> ls -la ~/Documents` |
| `sys ` | System Commands | `sys shutdown` |
| *(none)* | App Launcher + Google fallback | `firefox` |

---

## 🤖 AI Features

Natural language queries are automatically detected and routed to an OpenAI-compatible API:

- **Detection heuristic**: query > 20 chars, OR contains NL verbs (find, show, what, how, etc.)
- **Tool calling**: AI can invoke plugins (file_search, web_search, calculator, shell)
- **Force AI mode**: press `Ctrl+Enter` from any query
- **Streaming**: client supports streaming responses

Configure in Settings (Ctrl+,):
- AI Provider URL (default: `http://localhost:5000`)
- Model name (default: `auto`)
- API key (optional)

Works with: OpenAI, Ollama, LM Studio, Nous Hermes, and any OpenAI-compatible endpoint.

---

## ⚙️ Settings

Settings are saved to `~/.config/omnilauncher/settings.json`:

```json
{
  "ai_base_url": "http://localhost:5000",
  "ai_model": "auto",
  "ai_api_key": "",
  "theme": "dark",
  "hotkey": "Alt+Space",
  "max_results": 10
}
```

---

## 🖥️ Cross-Platform

| Platform | App Indexing | Hotkey | Shell |
|----------|-------------|--------|-------|
| Linux | `.desktop` files | Alt+Space | `sh -c` |
| macOS | `/Applications/*.app` | Alt+Space | `sh -c` |
| Windows | Start Menu `.lnk` | Alt+Space | `cmd /C` |

---

## 📁 Project Structure

```
OmniLauncher/
├── src/                    # React + TypeScript frontend
│   ├── App.tsx
│   └── components/
│       ├── SearchBar.tsx
│       ├── ResultList.tsx
│       ├── AIResponsePane.tsx
│       └── SettingsPanel.tsx
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── lib.rs
│   │   ├── main.rs
│   │   ├── settings.rs
│   │   ├── plugins/
│   │   │   ├── mod.rs      # Plugin trait + PluginManager
│   │   │   ├── web_search.rs
│   │   │   ├── calculator.rs
│   │   │   ├── file_search.rs
│   │   │   ├── app_launcher.rs
│   │   │   ├── system_commands.rs
│   │   │   └── shell_plugin.rs
│   │   └── ai/
│   │       ├── mod.rs
│   │       ├── client.rs   # OpenAI-compatible HTTP client
│   │       └── router.rs   # NL detection + tool dispatch
│   ├── tests/
│   │   └── plugin_tests.rs
│   └── Cargo.toml
├── legacy/                 # Original WPF/Windows app
├── package.json
├── vite.config.ts
└── index.html
```

---

## 📄 License

MIT — see [LICENSE](LICENSE)
