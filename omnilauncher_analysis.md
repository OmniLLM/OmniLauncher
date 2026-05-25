# OmniLauncher Codebase Analysis

**Project Size:** ~5,944 lines of Rust + TypeScript code  
**Language:** Rust (backend) + React/TypeScript (frontend)  
**Framework:** Tauri v2 (cross-platform desktop)  
**Date Analyzed:** 2026-05-24

---

## 1. Overall Directory Structure

### Top-Level Structure
```
OmniLauncher/
├── src-tauri/               # Rust backend (Tauri)
│   ├── src/
│   │   ├── main.rs         # Tauri app entry, commands, state management
│   │   ├── lib.rs          # Library exports, plugin manager setup
│   │   ├── settings.rs     # Configuration (AI URL, model, API key, theme)
│   │   ├── ai/
│   │   │   ├── mod.rs
│   │   │   ├── client.rs   # Claude/OpenAI API client
│   │   │   └── router.rs   # AI routing logic, conversation context (800 lines)
│   │   └── plugins/        # 24 plugin modules
│   ├── Cargo.toml
│   ├── build.rs
│   └── tests/
│       └── plugin_tests.rs # 80+ test cases
├── src/                    # React frontend
│   ├── App.tsx            # Main UI component (680 lines)
│   └── components/        # UI subcomponents
├── package.json
├── vite.config.ts
├── tsconfig.json
├── README.md
└── Makefile
```

---

## 2. High-Level Architecture

```
User Input → Frontend UI (React/App.tsx)
              ↓
         Detect AI prefix? (? or ai )
         /          \
    Local Mode      AI Mode
        ↓              ↓
  Plugin Search   AI Router
        ↓              ↓
  Results UI    Tool Collection
               ↓
           Claude API Call
               ↓
           Tool Execution
               ↓
           Response Formatting
               ↓
           Chat Display
```

---

## 3. src-tauri/src/ai/router.rs (800 lines)

**Purpose:** Routes queries to local plugins or AI, manages conversation context, handles 20+ slash commands.

### Key Types
```rust
pub struct AiResponse {
    pub content: String,           // Formatted response
    pub tools_used: Vec<String>,   // Which tools were called
    pub results: Vec<QueryResult>, // Local search results
    pub is_ai: bool,              // Is this AI-generated?
}

pub struct ConversationContext {
    pub messages: Vec<Message>,    // Full conversation history
    pub max_turns: usize,         // Default: 10
}

pub enum RouteDecision {
    Local,  // Use plugins only
    Ai,     // Use AI with tools
}
```

### Routing Logic
```rust
Router::decide(input: &str) -> RouteDecision
```
- If input starts with `?` or `ai ` (case-insensitive) → **Ai**
- Otherwise → **Local**

### AI Tool Use Flow
```rust
Router::ai_route(plugin_manager, ai_client, context) -> AiResponse
```
1. Collect all plugin tool schemas
2. Build system prompt (OS-aware: Windows/macOS/Linux)
3. Call AI with tool_choice="auto"
4. Execute any tool calls via plugins
5. **Follow-up AI call** to format tool results as Markdown
6. Return final response

### 20+ Slash Commands (Instant, No AI)
```
/run <cmd>     - Shell execution
/open <file>   - Open files/URLs
/app <name>    - App launcher
/find <pattern> - File search
/grep <regex> - Content search
/cat <file>    - Read file
/ls [path]     - List directory
/git [cmd]     - Git operations
/calc <expr>   - Calculator
/todo [text]   - Todo list
/web <query>   - Web search
/ip            - Public IP
/ports         - Listening ports
/ps            - Top processes
/kill <pid>    - Kill process
/env <var>     - Environment variable
/color <hex>   - Color conversion
/sys <action>  - System commands
/clip [term]   - Clipboard history
/help [cmd]    - Help system
```

Each command:
- Executes instantly (no AI latency)
- Returns formatted result
- Can preview before execution

### Tests
- ✅ `test_local_routes()` - Non-AI queries
- ✅ `test_ai_routes()` - `?` and `ai ` prefixes
- ✅ `test_strip_prefix()` - Prefix removal

---

## 4. src-tauri/src/main.rs (350 lines)

**Purpose:** Tauri app initialization, command handlers, state management.

### AppState
```rust
pub struct AppState {
    pub plugin_manager: Arc<Mutex<PluginManager>>,
    pub ai_client: Mutex<AiClient>,
    pub settings: Arc<Mutex<AppSettings>>,
    pub conversation: Arc<Mutex<ConversationContext>>,
}
```

### Global Hotkey
- **Ctrl+Shift+O**: Toggle window visibility

### Exported Tauri Commands

| Command | Purpose |
|---------|---------|
| `search(query)` | Plugin search (launcher mode) |
| `ai_query(query)` | AI chat (add to context, call router) |
| `clear_conversation()` | Reset chat history |
| `slash_preview(query)` | Preview results for slash commands |
| `execute_result(result)` | Execute action (open, shell, copy, etc.) |
| `list_models(url, key)` | Get available models from API |
| `get_settings()` | Return current settings |
| `save_settings_cmd(settings)` | Persist settings to disk |

### Command Implementations
- **search**: `plugin_manager.query_all(query)` → top 10 results
- **ai_query**: Adds user msg to context → calls Router::ai_route() → adds response to context
- **slash_preview**: Special handling for `/web` (hardcoded Google/YouTube/GitHub), `/kill` (enumerate processes)
- **execute_result**: Handles action_type (url, shell, open, copy)
- **list_models**: Calls `/v1/models` endpoint, returns model IDs
- **save_settings_cmd**: Updates in-memory settings + recreates AiClient + persists JSON

---

## 5. src-tauri/src/lib.rs (46 lines)

**Purpose:** Central library entry point, plugin registration.

### Modules
```rust
pub mod ai;
pub mod plugins;
pub mod settings;
```

### Plugin Registration
```rust
pub fn create_plugin_manager() -> PluginManager {
    // 24 plugins registered:
    agent_delegate, app_launcher, bash_exec, browser_bookmarks, calculator,
    clipboard, code_tools (CodeExec + Patch), color_picker, env_vars,
    file_read, file_search, file_write, git, glob, grep, hosts,
    http_client, ls, network, process_manager, shell_plugin,
    snippets, sys_info, system_commands, timer, todo, translate,
    unit_converter, url_opener, web_fetch, web_search, windows_settings
}
```

---

## 6. src-tauri/src/plugins/mod.rs (122 lines)

**Purpose:** Plugin system design.

### Plugin Trait
```rust
#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;                              // Unique ID
    fn description(&self) -> &str;                       // Help text
    fn keyword(&self) -> Option<&str>;                   // Prefix (e.g., "=" for calc)
    async fn query(&self, q: &Query) -> Vec<QueryResult>; // Local search
    fn tool_schema(&self) -> Option<serde_json::Value>;   // For AI tools
    async fn execute_tool(&self, args: Value) -> String; // AI execution
}
```

### PluginManager
```rust
pub struct PluginManager {
    pub plugins: Vec<Box<dyn Plugin>>,
}

// Methods:
pub async fn query_all(raw: &str) -> Vec<QueryResult>     // Top 10 sorted by score
pub fn all_tool_schemas() -> Vec<serde_json::Value>       // For AI tool_choice="auto"
pub async fn execute_tool(name: &str, args: Value) -> String
```

### 24 Registered Plugins

| Plugin | Keyword | Purpose |
|--------|---------|---------|
| agent_delegate | - | Delegate to AI |
| app_launcher | - | Find & launch apps |
| bash_exec | > | Shell commands |
| browser_bookmarks | bm | Search bookmarks |
| calculator | = | Math expressions |
| clipboard | cb | Clipboard history |
| code_tools | - | Python/JS/Rust exec + file patches |
| color_picker | color | Hex/RGB/HSL conversion |
| env_vars | env | Environment variables |
| file_read | - | Read file (with line range) |
| file_search | f | Find files by name |
| file_write | - | Create/edit files |
| git | - | Git commands |
| glob | - | Glob patterns |
| grep | grep | File content search |
| hosts | hosts | Edit system hosts |
| http_client | - | HTTP requests |
| ls | - | List directory |
| network | net | IP, DNS, whois |
| process_manager | - | List/kill processes |
| shell_plugin | > | Shell execution |
| snippets | snip | Code snippets |
| sys_info | sysinfo | OS/CPU/memory/disk |
| system_commands | sys | Lock/sleep/shutdown/restart |
| timer | timer | Set timers |
| todo | - | Persistent todo list |
| translate | tr | Google Translate |
| unit_converter | unit | Distance/weight/temp |
| url_opener | - | Open URLs |
| web_fetch | - | Fetch & parse web |
| web_search | g | Google/YouTube/GitHub |
| windows_settings | settings | Windows settings |

---

## 7. src-tauri/src/ai/client.rs (196 lines)

**Purpose:** HTTP client for Claude/OpenAI-compatible APIs.

### Data Structures
```rust
pub struct Message {
    pub role: String,       // "system", "user", "assistant", "tool"
    pub content: String,
}

pub struct ToolCall {
    pub id: String,
    pub function: FunctionCall,
}

pub struct FunctionCall {
    pub name: String,       // Tool name
    pub arguments: String,  // JSON string
}

pub struct ChatResponse {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

pub struct AiClient {
    pub base_url: String,   // e.g., "http://localhost:5000"
    pub api_key: String,    // Bearer token
    pub model: String,      // e.g., "claude-3-sonnet"
}
```

### Main Methods
```rust
pub async fn chat(messages) -> Result<String>
pub async fn chat_with_tools(messages, tools) -> Result<ChatResponse>
pub async fn chat_stream(messages, window) -> Result<()>  // Streaming (unused)
```

### API Call Details
- **Endpoint**: `{base_url}/v1/chat/completions`
- **Method**: POST
- **Tool choice**: `"auto"`
- **Timeout**: 30 seconds
- **Auth**: Bearer token (if api_key provided)
- **Response**: Parses choices[0].message for content & tool_calls

---

## 8. src-tauri/src/settings.rs (55 lines)

**Purpose:** Configuration management.

### AppSettings
```rust
pub struct AppSettings {
    pub ai_base_url: String,    // Default: "http://localhost:5000"
    pub ai_model: String,       // Default: "auto"
    pub ai_api_key: String,     // Default: ""
    pub theme: String,          // Default: "dark"
    pub hotkey: String,         // Default: "Alt+Space"
    pub max_results: usize,     // Default: 10
}
```

### Functions
- `settings_path()` → `~/.config/omnilauncher/settings.json`
- `load_settings()` → Parse JSON, fallback to Default
- `save_settings(settings)` → Pretty-print JSON to disk

---

## 9. src/App.tsx (683 lines)

**Purpose:** Main React UI with dual modes (launcher + AI chat).

### Layout Modes

**Launcher Mode** (input does NOT start with `?` or `ai `):
- Compact 64px height (grows with results)
- Max 520px with results
- Shows SearchBar + ResultList + Settings

**AI Chat Mode** (input starts with `?` or `ai `):
- Fixed 560px height
- Top bar: "✦ AI Chat" + "New conversation" button
- Scrollable chat history
- SearchBar at bottom

### Key State
```typescript
const [query, setQuery] = useState('')
const [results, setResults] = useState<QueryResult[]>([])
const [loading, setLoading] = useState(false)
const [showSettings, setShowSettings] = useState(false)
const [theme, setTheme] = useState<'dark' | 'light'>('dark')
const [conversationHistory, setConversationHistory] = useState<ConversationTurn[]>([])
```

### Theme Colors
**Dark** (Catppuccin Mocha):
- bg: #1E1E2E, surface: #313244, accent: #CBA6F7 (purple)

**Light** (Catppuccin Latte):
- bg: #EFF1F5, surface: #CCD0DA, accent: #8839EF (purple)

### Functions
- `isAiPrefix()` - Detects `?` or `ai ` prefix
- `doSearch()` - Debounced plugin search
- `doAiQuery()` - Send to AI, add to history
- `handleExecute()` - Run action (copy, open, shell, etc.)
- `handleNewChat()` - Clear conversation
- `renderMarkdown()` - Convert markdown to HTML

### Markdown Support
- Code blocks with language highlighting
- Headers, lists, tables
- Inline formatting (bold, italic, code, links)
- HTML escaping for safety

### Components
- **SearchBar**: Input field with slash command hints
- **ResultList**: Clickable results with icons & scores
- **SettingsPanel**: Configure AI URL, model, API key, theme
- **ChatBubble**: User/assistant messages with tool chips

### Key Effects
- Auto-scroll chat to bottom
- Focus input on window focus (with retries)
- Ctrl+, toggles settings
- Escape clears search
- Debounced search (150ms)

---

## 10. Cargo.toml Dependencies

```toml
tauri = "2"                              # Desktop framework
tauri-plugin-global-shortcut = "2"      # Ctrl+Shift+O hotkey
serde = { version = "1", features = ["derive"] }
serde_json = "1"                        # JSON
tokio = { version = "1", features = ["full"] }  # Async
reqwest = { version = "0.12", features = ["json", "stream"] }  # HTTP
futures-util = "0.3"                    # Async utils
async-trait = "0.1"                     # Async traits
dirs = "5"                              # Home dirs
walkdir = "2"                           # Directory traversal
regex = "1"                             # Pattern matching
glob = "0.3"                            # Glob patterns
sysinfo = "0.30"                        # System info
```

---

## 11. Tests (516 lines, 80+ cases)

### Test Categories
| Category | Tests |
|----------|-------|
| Plugin Manager | 3 |
| Calculator | 4 |
| Shell Execution | 3 |
| File Operations | 13 |
| Grep/Glob | 4 |
| Directory Listing | 2 |
| Git | 1 |
| Todo | 3 |
| Color Picker | 3 |
| System Info | 2 |
| Web & HTTP | 5 |
| Code Execution | 2 |
| App Launcher | 1 |
| Windows Settings | 1 |
| Hosts | 1 |
| Web Fetch | 2 |

### ⚠️ Outdated Tests
`test_nl_positive()` and `test_nl_negative()` reference `Router::is_natural_language()` which does NOT exist in router.rs.

---

## 12. Data Flow Examples

### Example 1: Launcher Search
```
User types: "chrome"
    ↓ invoke("search", {query: "chrome"})
    ↓ plugin_manager.query_all("chrome")
    ↓ Each plugin's query() called
    ↓ Top 10 sorted by score
    ↓ Frontend displays results
    ↓ User clicks
    ↓ invoke("execute_result", {result})
    ↓ Backend executes action_type
```

### Example 2: AI Query
```
User types: "? what files are in src?"
    ↓ Detects "?" prefix
    ↓ invoke("ai_query", {query: "? what files..."})
    ↓ Add to conversation context
    ↓ Router::ai_route()
    ├─ Collect tool schemas (24 plugins)
    ├─ Build OS-aware system prompt
    ├─ POST to Claude API
    ├─ Claude returns tool_calls (e.g., glob_files)
    ├─ Execute: glob_files({pattern: "src/**"})
    ├─ Follow-up AI call for formatting
    └─ Return formatted response
    ↓ Frontend shows in chat
```

### Example 3: Slash Command
```
User types: "/run npm test"
    ↓ invoke("slash_preview", {query: "/run npm test"})
    ↓ Router::slash_command()
    ├─ Match "/run"
    ├─ Execute shell_exec("npm test")
    └─ Return output
    ↓ Frontend displays result
```

---

## 13. Key Architectural Decisions

1. **Dual UI Modes**
   - Fast launcher (plugins) vs. conversational (AI)
   - Triggered by query prefix

2. **Plugin System**
   - 24 plugins for instant results
   - Tool schemas for AI integration
   - Async-trait for polymorphism

3. **Conversation Context**
   - Multi-turn history (max 10 turns)
   - Trimmed to preserve token budget
   - Includes system prompt

4. **OpenAI-Compatible API**
   - Supports Claude, GPT, local LLMs
   - Standard `/v1/chat/completions` endpoint
   - Tool choice: "auto"

5. **Cross-Platform**
   - OS detection in system prompt
   - Different shell syntax per OS
   - Adaptive commands (xdg-open vs open vs start)

6. **Slash Commands**
   - 20+ instant commands (no AI)
   - Full help system
   - Separate from AI routing

7. **Frontend-Backend Split**
   - Tauri IPC for communication
   - Tokio for async Rust backend
   - React for responsive UI

---

## 14. Known Issues

1. **Outdated Tests**: `test_nl_positive()` references non-existent `Router::is_natural_language()`
2. **Streaming Unused**: `chat_stream()` implemented but not used in frontend
3. **No Caching**: Expensive operations (web search, file system) not cached
4. **No Permission System**: All tools available to any AI prompt
5. **Error Handling**: Some tools return raw error strings instead of structured errors

---

## 15. Extension Points

1. **New Plugins**: Create `src-tauri/src/plugins/new.rs` + implement `Plugin` trait + register in lib.rs
2. **New Slash Commands**: Add case in `router.rs` `slash_command()` + update help
3. **UI Customization**: Modify DARK_COLORS/LIGHT_COLORS in App.tsx
4. **New OS Support**: Update `get_os_info()` in router.rs
5. **Different AI Model**: Settings.ai_base_url is already configurable (supports any OpenAI-compatible API)

---

## Summary

**OmniLauncher** is a sophisticated dual-mode desktop launcher:

- **Launcher Mode**: 24 plugins provide instant local search (app launch, file search, calculator, etc.)
- **AI Chat Mode**: Claude/compatible APIs with full tool access for answering questions, writing code, searching files, etc.
- **Architecture**: Tauri (Rust backend + React frontend) with plugin-based extensibility
- **Smart Routing**: Detects intent from query prefix (`?` or `ai `) to route local vs. AI
- **Cross-Platform**: Windows/macOS/Linux with OS-aware shell commands
- **Conversation Context**: Multi-turn chat with message history trimming
- **20+ Slash Commands**: Instant commands for common tasks (no AI latency)

**Key Strength**: Clean separation of concerns with async/await, proper error handling, and extensible plugin architecture.

