# OmniLauncher - Complete Source Files Manifest

## Frontend Source Files (TypeScript/React)

### Directory: `src/`

| File | Lines | Purpose |
|------|-------|---------|
| **main.tsx** | 10 | React entry point; renders App component into #root DOM element |
| **App.tsx** | 1,084 | **CORE**: Dual-mode launcher/AI chat UI; state management; markdown renderer; keyboard shortcuts |
| **styles.css** | - | Global theme styles (Catppuccin Mocha/Latte colors) |
| **tauri-api.ts** | 58 | Browser-safe Tauri wrapper with mock fallbacks for dev mode |
| **tauri-shim.ts** | 39 | Global Tauri API initialization shim; prevents "not available" errors |

### Directory: `src/components/`

| File | Lines | Purpose |
|------|-------|---------|
| **SearchBar.tsx** | 298 | Input bar with spinner/icon, settings button, AI badge, hint bar (command prefixes) |
| **ResultList.tsx** | 210 | Scrollable results list; keyboard navigation; mouse hover; query highlighting |
| **SettingsPanel.tsx** | 242 | Settings dialog: AI endpoint, API key, model dropdown, theme, hotkey display |
| **AIResponsePane.tsx** | 29 | **Legacy no-op stub** (chat rendering moved to App.tsx ChatBubble) |

---

## Backend Source Files (Rust)

### Directory: `src-tauri/src/`

#### Core Application

| File | Lines | Purpose |
|------|-------|---------|
| **main.rs** | 733 | **ENTRY POINT**: Tauri setup, AppState, 12 Tauri commands, global shortcut, tray icon |
| **lib.rs** | 51 | Module exports; registers 33 plugins; exports AI/settings/skills modules |
| **settings.rs** | 55 | AppSettings struct (ai_base_url, ai_model, ai_api_key, theme, hotkey, max_results) |
| **guardrails.rs** | 192 | Safety checks: shell commands (RCE/fork bomb), file writes (system paths); 15 tests |
| **live_server.rs** | 149 | Async HTTP server (tokio); /health, /todo, /todo/data routes; HTTP/1.1 encoding |

#### AI Module (`ai/`)

| File | Lines | Purpose |
|------|-------|---------|
| **ai/mod.rs** | 4 | Module re-exports (client, errors, router) |
| **ai/client.rs** | 371 | **LLM CLIENT**: OpenAI-compatible API calls; 3-attempt retry with backoff; streaming support |
| **ai/errors.rs** | 108 | Error classification: Transient/Permanent/ModelError/ResourceError; 6 tests |
| **ai/router.rs** | 500+ | **AGENTIC ORCHESTRATION**: Router::decide(), Router::ai_route(), 10-iteration loop, skill injection, loop detection |

#### Plugin System (`plugins/`)

| File | Lines | Purpose |
|------|-------|---------|
| **plugins/mod.rs** | 122 | Plugin trait definition; PluginManager (query_all, execute_tool, tool_schemas) |
| **plugins/agent_delegate.rs** | ? | Delegate queries to other agents |
| **plugins/app_launcher.rs** | ? | Launch applications (Windows/Mac/Linux) |
| **plugins/bash_exec.rs** | ? | Execute shell commands with guardrails |
| **plugins/browser_bookmarks.rs** | ? | Search browser bookmarks (Chrome, Firefox, Safari) |
| **plugins/calculator.rs** | ? | Parse/evaluate math expressions |
| **plugins/clipboard.rs** | ? | Search clipboard history |
| **plugins/code_tools.rs** | ? | Execute Python, JavaScript, PowerShell, Rust, Bash code |
| **plugins/color_picker.rs** | ? | Convert color formats (hex, rgb, names) |
| **plugins/env_vars.rs** | ? | Get environment variables |
| **plugins/file_read.rs** | ? | Read file contents |
| **plugins/file_search.rs** | ? | Search files by name/pattern |
| **plugins/file_write.rs** | ? | Write files (guardrailed) |
| **plugins/git.rs** | ? | Git operations (log, branch, diff, etc.) |
| **plugins/glob.rs** | ? | Glob pattern file matching |
| **plugins/grep.rs** | ? | Regex file content search |
| **plugins/hosts.rs** | ? | Edit /etc/hosts or Windows hosts file |
| **plugins/http_client.rs** | ? | HTTP requests (GET/POST/PUT/DELETE) |
| **plugins/ls.rs** | ? | List directory contents |
| **plugins/network.rs** | ? | Network info (IP, ports, connectivity) |
| **plugins/process_manager.rs** | ? | List/kill processes; CPU usage |
| **plugins/shell_plugin.rs** | ? | Shell command wrapper |
| **plugins/snippets.rs** | ? | Code snippet management |
| **plugins/sys_info.rs** | ? | System information (CPU, memory, uptime) |
| **plugins/system_commands.rs** | ? | System actions (lock, sleep, shutdown) |
| **plugins/timer.rs** | ? | Timer/stopwatch functionality |
| **plugins/todo.rs** | ? | Todo list management |
| **plugins/translate.rs** | ? | Text translation |
| **plugins/unit_converter.rs** | ? | Unit conversion (distance, weight, temperature, etc.) |
| **plugins/url_opener.rs** | ? | Open URLs/files/apps (platform-specific) |
| **plugins/web_fetch.rs** | ? | Fetch and parse web page content |
| **plugins/web_search.rs** | ? | Web search (Google, YouTube, GitHub, Wikipedia, etc.) |
| **plugins/windows_settings.rs** | ? | Windows-specific settings (lock, sleep, shutdown) |

#### Database (`db/`)

| File | Lines | Purpose |
|------|-------|---------|
| **db/mod.rs** | 100 | SQLite migration runner; idempotent schema changes; 3 migrations tracked |

#### Skills (`skills/`)

| File | Lines | Purpose |
|------|-------|---------|
| **skills/mod.rs** | 364 | SkillManager: load SKILL.md files; parse frontmatter; skill injection; hot-reload; 3 tests |

---

## Summary Statistics

### Frontend
- **Total TypeScript/TSX files**: 8
- **Total lines (approx)**: 1,980
- **Key file**: App.tsx (1,084 lines - 55% of frontend code)
- **Components**: 4 (SearchBar, ResultList, SettingsPanel, AIResponsePane)
- **Patterns**: Dual-mode UI, markdown rendering, state management, keyboard navigation

### Backend (Rust)
- **Core files**: 5 (main, lib, settings, guardrails, live_server)
- **AI module**: 4 files (client, errors, router, mod)
- **Plugin system**: 34 files (mod + 33 plugin implementations)
- **Database**: 1 file (migrations)
- **Skills**: 1 file (manager)
- **Total modules**: 45 .rs files
- **Key file**: router.rs (500+ lines - agentic orchestration)
- **Total lines (approx)**: 7,000+

### Architecture
- **Platform**: Tauri (Rust backend + React frontend)
- **Plugins**: 33 total (system, file, web, productivity, utility)
- **AI Features**: Tool calling, retry logic, streaming, skill injection, loop detection
- **Safety**: Guardrails for shell/file operations
- **Databases**: SQLite with migration system
- **Skills**: Hot-loadable markdown files with frontmatter

---

## Code Organization Patterns

### 1. **Plugin Architecture**
```
Plugin trait (query, execute_tool, tool_schema)
    ↓
33 plugin implementations
    ↓
PluginManager (register, query_all, execute_tool, tool_schemas)
    ↓
Frontend invokes via Tauri command
```

### 2. **AI Router Pipeline**
```
Router::decide() [Local vs Ai]
    ↓
Router::ai_route() [System prompt + skills + agentic loop]
    ↓
AiClient::chat_with_tools() [API call + retry]
    ↓
Error classification + recovery
    ↓
Loop detection
    ↓
Final response to UI
```

### 3. **Frontend State Management**
```
Query input
    ↓
isAiPrefix? [Determine mode]
    ↓
AI mode → Conversation history + chat rendering
    ↓
Launcher mode → Plugin results + execution
```

### 4. **Settings & Config**
```
~/.config/omnilauncher/settings.json
    ↓
AppSettings (struct)
    ↓
Loaded at startup
    ↓
Updated via SettingsPanel
    ↓
Persisted to disk
```

### 5. **Skills System**
```
assets/skills/ [bundled]
    +
~/.config/omnilauncher/skills/ [user]
    ↓
Load SKILL.md files
    ↓
Parse frontmatter (name, triggers, tags, tools)
    ↓
Find relevant by query matching
    ↓
Inject context before AI query
```

---

## Data Structures (Key Types)

### Frontend (TypeScript)
```typescript
interface QueryResult {
  id: string;
  title: string;
  subtitle?: string;
  icon?: string;
  score: number;
  action_type: string;
  action_data: string;
}

interface AiResponse {
  content: string;
  tools_used: string[];
  results: QueryResult[];
  is_ai: boolean;
}

interface ConversationTurn {
  role: "user" | "assistant";
  content: string;
  tools_used?: string[];
  isStreaming?: boolean;
}

interface AppSettings {
  ai_base_url: string;
  ai_model: string;
  ai_api_key: string;
  theme: string;
  hotkey: string;
  max_results: number;
}
```

### Backend (Rust)
```rust
pub struct AppState {
  plugin_manager: Arc<Mutex<PluginManager>>,
  ai_client: Mutex<AiClient>,
  settings: Arc<Mutex<AppSettings>>,
  conversation: Arc<Mutex<ConversationContext>>,
  skill_manager: Arc<Mutex<SkillManager>>,
  live_server: LiveServer,
  live_server_port: u16,
}

pub struct PluginManager {
  plugins: Vec<Box<dyn Plugin>>,
}

pub struct Message {
  role: String,
  content: Option<String>,
  tool_calls: Option<Vec<ToolCall>>,
  tool_call_id: Option<String>,
  name: Option<String>,
}

pub struct Skill {
  meta: SkillMeta,
  body: String,
}
```

---

## Testing Coverage

### Existing Tests
- **guardrails.rs**: 15 tests (DENY patterns, WARN patterns, ALLOW cases)
- **errors.rs**: 6 tests (error classification)
- **skills/mod.rs**: 3 tests (parsing, relevance matching)
- **main.rs**: 1 test (external command failure)
- **Total**: 25 tests

### Test Recommendations
1. Plugin integration tests (query/execute)
2. Router decision logic (Local vs AI)
3. Agentic loop termination conditions
4. Markdown edge cases (nesting, tables)
5. Skill injection correctness
6. Settings persistence
7. Error recovery strategies

---

## Performance Considerations

### Launcher Mode (Plugin-only)
- **Latency**: <100ms (local execution)
- **Results**: Top 10 (sorted by score)
- **Debounce**: 100ms on input

### AI Mode
- **Latency**: 1-5s (LLM call) + tool execution
- **Retries**: 3 attempts with backoff (2s, 4s, 8s)
- **Max iterations**: 10 agentic loops
- **Token budget**: 32k (70% threshold for compression)
- **Streaming**: SSE with Tauri events

### UI Responsiveness
- **Window geometry**: Dynamically adjusted (40% width AI, 33% width launcher)
- **Animations**: Fade-in, pulse, spin (CSS keyframes)
- **Focus management**: Auto-focus on window show
- **Debouncing**: 100ms on search input

---

## Integration Points

### Frontend → Backend
1. `search(query)` → PluginManager
2. `ai_query(query)` → Router.ai_route()
3. `execute_result(result)` → Platform handler
4. `get_settings()` → AppSettings
5. `save_settings_cmd(settings)` → Persistence
6. `list_models(base_url, api_key)` → LLM endpoint
7. `list_skills()` / `reload_skills()` / `install_skill()` → SkillManager
8. `set_window_geometry(height, ai_mode)` → Window sizing

### Backend → LLM API
- POST /v1/chat/completions
- Bearer auth (if api_key set)
- Tool calling with OpenAI format
- Streaming support (SSE)

### Backend → Local System
- Shell execution (guardrailed)
- File I/O (guardrailed)
- Process management
- Network queries
- App launching

---

## Build & Deployment

### Frontend Build
```bash
npm run build  # TypeScript → JavaScript (bundled)
```

### Backend Build
```bash
cargo build --release  # Rust → binary
```

### Package
```bash
tauri build  # Creates app bundle (dmg/msi/deb)
```

### Configuration
- Settings: ~/.config/omnilauncher/settings.json
- Skills: ~/.config/omnilauncher/skills/
- Debug log: ~/.config/omnilauncher/omnilauncher.log (or %TEMP%/omnilauncher.log on Windows)

### Global Shortcut
- Keybinding: Ctrl+Shift+O (registered at startup)
- Action: Toggle window visibility
- Fallback: Tray icon left-click

---

## Notable Implementation Details

✅ **Strengths**:
- Modular plugin system (33 independent plugins)
- Sophisticated error recovery (classify_error pattern)
- Loop detection (3-fingerprint history)
- Context compression (sliding window >70% threshold)
- Skill injection (Hermes pattern for extensible AI)
- Cross-platform support (Windows/Mac/Linux detection)
- Guardrails (shell/file safety checks)
- Retry logic (exponential backoff + jitter)
- Hot-reload (skills without restart)
- Theme support (Catppuccin colors)

⚠️ **Edge Cases / TODOs**:
- router.rs could benefit from state machine refactor
- Plugin output size limits not enforced
- Skill body not HTML-sanitized before injection
- No rate limiting within agentic loop
- Conversation history not persisted to disk
- Settings API key stored as plain JSON
- Markdown renderer susceptible to DOS (deeply nested structures)
- Limited plugin test coverage

