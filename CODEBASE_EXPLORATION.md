# OmniLauncher Codebase Exploration

## Project Overview
**OmniLauncher** is a Rust-based Tauri desktop application that functions as an intelligent launcher/productivity tool. It uses a plugin-based architecture where different tools and commands are implemented as plugins, with optional AI integration via Claude API.

**Key Technologies:**
- Backend: Rust + Tauri 2
- Frontend: React 18 + TypeScript + Vite
- Database: SQLite (rusqlite)
- Async Runtime: Tokio
- API Calls: Reqwest

---

## Directory Structure

```
OmniLauncher/
├── src-tauri/                    # Rust backend (Tauri)
│   ├── src/
│   │   ├── main.rs              # Tauri app entry point, command handlers, window mgmt
│   │   ├── lib.rs               # Library exports, create_plugin_manager()
│   │   ├── plugins/
│   │   │   ├── mod.rs           # Plugin trait, PluginManager, QueryResult definitions
│   │   │   ├── calculator.rs
│   │   │   ├── web_search.rs
│   │   │   ├── clipboard.rs
│   │   │   ├── bash_exec.rs
│   │   │   ├── code_tools.rs
│   │   │   ├── file_read.rs
│   │   │   ├── file_write.rs
│   │   │   ├── git.rs
│   │   │   ├── glob.rs
│   │   │   ├── grep.rs
│   │   │   ├── http_client.rs
│   │   │   ├── ls.rs
│   │   │   ├── network.rs
│   │   │   ├── process_manager.rs
│   │   │   ├── shell_plugin.rs
│   │   │   ├── snippet.rs
│   │   │   ├── timer.rs
│   │   │   ├── todo.rs
│   │   │   ├── translate.rs
│   │   │   ├── unit_converter.rs
│   │   │   ├── url_opener.rs
│   │   │   ├── web_fetch.rs
│   │   │   ├── windows_settings.rs
│   │   │   ├── screenshot.rs
│   │   │   ├── script_runner.rs
│   │   │   └── selection.rs
│   │   ├── ai/                  # AI routing and client
│   │   │   ├── client.rs        # AiClient - OpenAI-compatible API calls
│   │   │   ├── router.rs        # Router - AI query routing + tool calling
│   │   │   ├── errors.rs
│   │   │   └── mod.rs
│   │   ├── skills/              # External skills system
│   │   ├── db/                  # Database layer
│   │   ├── guardrails.rs        # Safety/validation
│   │   └── settings.rs          # AppSettings struct, load/save
│   ├── Cargo.toml              # Rust dependencies
│   └── build.rs
├── src/                         # React frontend
│   ├── App.tsx                 # Main app component, layout, state management
│   └── components/
│       ├── SearchBar.tsx       # Search input + hint bar
│       ├── ResultList.tsx      # Search results display
│       ├── SettingsPanel.tsx   # Settings UI
│       └── AIResponsePane.tsx
├── package.json               # Frontend dependencies
├── tsconfig.json
├── vite.config.ts
└── ...
```

---

## Core Backend Architecture

### 1. Plugin System (`src-tauri/src/plugins/mod.rs`)

**Core Traits and Structs:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub id: String,                    // Unique identifier
    pub title: String,                 // Display title
    pub subtitle: Option<String>,      // Secondary text
    pub icon: Option<String>,          // Emoji or icon
    pub score: i32,                    // Relevance score for sorting
    pub action_type: String,           // "url", "open", "shell", "copy", etc.
    pub action_data: String,           // Data to execute with action
}

pub struct Query {
    pub raw: String,                   // Original input
    pub terms: Vec<String>,            // Split by whitespace
}

#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;                              // Plugin identifier
    fn description(&self) -> &str;                       // Human description
    fn keyword(&self) -> Option<&str>;                   // Optional trigger prefix (e.g., "=" for calc)
    async fn query(&self, q: &Query) -> Vec<QueryResult>; // Main search method
    fn tool_schema(&self) -> Option<serde_json::Value> { None }  // Optional Claude tool schema (JSON)
    async fn execute_tool(&self, _args: serde_json::Value) -> String { String::new() } // Tool execution
}

pub struct PluginManager {
    pub plugins: Vec<Box<dyn Plugin>>,
}

impl PluginManager {
    pub fn new() -> Self { ... }
    pub fn register(&mut self, p: Box<dyn Plugin>) { ... }
    pub async fn query_all(&self, raw: &str) -> Vec<QueryResult> { ... }  // Query all plugins
    pub fn all_tool_schemas(&self) -> Vec<serde_json::Value> { ... }      // Collect tool schemas
    pub async fn execute_tool(&self, name: &str, args: serde_json::Value) -> String { ... }
}
```

**Query Flow:**
1. Frontend sends search query
2. `query_all()` iterates through all plugins
3. Each plugin checks if keyword matches
4. Results are collected, sorted by score, limited to top 10
5. Results returned to frontend for display

**Plugin Registration** (`src-tauri/src/lib.rs`):
```rust
pub fn create_plugin_manager() -> PluginManager {
    let mut pm = PluginManager::new();
    pm.register(Box::new(plugins::calculator::CalculatorPlugin));
    pm.register(Box::new(plugins::web_search::WebSearchPlugin));
    // ... 30+ more plugins
    pm
}
```

### 2. Plugin Example: Calculator

**File:** `src-tauri/src/plugins/calculator.rs`

```rust
pub struct CalculatorPlugin;

#[async_trait]
impl Plugin for CalculatorPlugin {
    fn name(&self) -> &str { "calculator" }
    fn description(&self) -> &str { "Evaluate mathematical expressions" }
    fn keyword(&self) -> Option<&str> { Some("=") }  // Triggered by "=" prefix

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let expr = q.raw.strip_prefix('=').unwrap_or(&q.raw).trim();
        match evaluate(expr) {
            Some(result) => vec![QueryResult {
                id: format!("calc:{}", expr),
                title: format!("{} = {}", expr, result),
                subtitle: Some("Press Enter to copy result".to_string()),
                icon: Some("🧮".to_string()),
                score: 100,
                action_type: "copy".to_string(),
                action_data: result.to_string(),
            }],
            None => vec![],
        }
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(json!({
            "type": "function",
            "function": {
                "name": "calculator",
                "description": "Evaluate a math expression",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "expression": { "type": "string" }
                    },
                    "required": ["expression"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let expr = args["expression"].as_str().unwrap_or("");
        match evaluate(expr) {
            Some(r) => r.to_string(),
            None => "Could not evaluate expression".to_string(),
        }
    }
}
```

### 3. Web Search Plugin Pattern

**File:** `src-tauri/src/plugins/web_search.rs`

Shows advanced pattern with multiple prefixes:
- `g <query>` → Google
- `yt <query>` → YouTube
- `gh <query>` → GitHub
- `wiki <query>` → Wikipedia
- etc.

Uses URL template substitution and fallback search for bare queries.

### 4. Tauri App State (`src-tauri/src/main.rs`)

```rust
pub struct AppState {
    pub plugin_manager: Arc<Mutex<omnilauncher_lib::PluginManager>>,
    pub ai_client: Mutex<AiClient>,
    pub settings: Arc<Mutex<AppSettings>>,
    pub conversation: Arc<Mutex<ConversationContext>>,
    pub skill_manager: Arc<Mutex<SkillManager>>,
    pub live_server: LiveServer,
    pub live_server_port: u16,
}
```

**Key Tauri Commands:**
- `search(query)` → `Vec<QueryResult>` - Plugin search
- `ai_query(query)` → `AiResponse` - AI chat with tool calling
- `execute_result(result)` - Execute action (open URL, run shell, etc.)
- `slash_preview(query)` - Special "/" command handling
- `get_settings()` / `save_settings_cmd(settings)` - Settings management
- `list_models(base_url, api_key)` - Fetch available AI models
- `list_skills()` / `reload_skills()` / `install_skill(source)` - Skill management

### 5. AI Integration (`src-tauri/src/ai/`)

**AiClient** handles OpenAI-compatible API calls:
- Takes base_url, api_key, model
- Sends tool schemas to Claude
- Receives tool calls back
- Iteratively executes tools and returns to Claude

**Router** decides: plain launcher search vs AI chat
- Plain prefixes: `? ai ` → Use AI
- Otherwise → Use launcher plugins

### 6. Settings (`src-tauri/src/settings.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub ai_base_url: String,        // Claude API or compatible endpoint
    pub ai_api_key: String,         // API key
    pub ai_model: String,           // e.g., "claude-3-5-sonnet-20241022"
    pub theme: String,              // "dark" or "light"
    pub hotkey: String,             // Global hotkey (Ctrl+Shift+O)
    pub max_results: number,        // Max launcher results
}
```

---

## Frontend Architecture

### 1. Main App Component (`src/App.tsx`)

**State Management:**
- `query` - Current search/input text
- `results` - Plugin search results to display
- `aiModeEnabled` - Whether in AI chat mode
- `conversationHistory` - Chat history turns
- `showSettings` - Settings panel visibility
- `theme` - "dark" or "light"

**Key Features:**
1. **Dual Mode:**
   - Launcher mode: Search plugins, show results list
   - AI mode: Chat with Claude, show conversation history

2. **Mode Detection:**
   ```typescript
   export function isAiPrefix(input: string): boolean {
     const t = input.trim();
     return t.startsWith("?") || t.toLowerCase().startsWith("ai ");
   }
   ```

3. **Slash Commands:**
   - `/help` - Show all commands
   - `/new` - Clear conversation
   - `/clear` - Clear conversation
   - `/app`, `/find`, `/grep`, `/calc`, `/todo`, `/web`, etc.

4. **Markdown Rendering:**
   - Custom `renderMarkdown()` function supports:
     - Code blocks, inline code
     - Bold, italic, strikethrough
     - Headers, lists, tables
     - Links (with href validation)

5. **Keyboard Shortcuts:**
   - `Enter` - Execute result / submit query
   - `Arrow Up/Down` - Navigate results
   - `Escape` - Clear query
   - `Ctrl+,` - Toggle settings
   - `Ctrl+Shift+O` - Global hotkey to toggle window

6. **Dynamic Geometry:**
   - Compact (56px) when empty
   - Expanded when results shown
   - Large (560px) in AI chat mode
   - Centered on screen with monitor awareness

### 2. SearchBar Component (`src/components/SearchBar.tsx`)

**Features:**
- Leading icon: spinner when loading, "✦" for AI, "⌕" for launcher
- AI badge when "?" prefix detected
- Settings button (Ctrl+,)
- Hint bar for empty queries showing:
  - Core prefixes: `=`, `>`, `*`, `b`, `?`, `/`
  - Web search: `g`, `yt`, `gh`, `wiki`, `maps`, `so`, `ddg`, etc.

**CSS-in-JS Styling:**
- Smooth transitions
- Focus ring with accent color
- Light/dark theme support (Catppuccin colors)

### 3. ResultList Component (`src/components/ResultList.tsx`)

**Features:**
- Keyboard navigation (arrow keys)
- Mouse hover/click selection
- Query highlight in titles
- Keyboard hints (⌘1-⌘9 for first 9 results)
- Action badges (`↵ Open`, `↵ Run`, `↵ Copy`, etc.)
- Staggered fade-in animation

### 4. SettingsPanel Component (`src/components/SettingsPanel.tsx`)

**Form Fields:**
- AI Provider URL (input)
- API Key (password input)
- Model (searchable dropdown, auto-fetches from endpoint)
- Theme (select: Dark/Light)
- Hotkey (read-only display)

**Features:**
- Fetches available models from endpoint
- Save button with success feedback ("✓ Saved")
- Error handling for model fetch failures

### 5. Color Themes

**Dark (Catppuccin Mocha):**
```typescript
const DARK_COLORS = {
  bg: "#1E1E2E",
  surface: "#313244",
  surface2: "#45475A",
  text: "#CDD6F4",
  accent: "#CBA6F7",  // purple
  accentDim: "#9B76C7",
  sub: "#6C7086",
  userBubble: "#CBA6F7",
  userBubbleText: "#1E1E2E",
  aiBubble: "#313244",
  aiText: "#CDD6F4",
};
```

**Light (Catppuccin Latte):**
Similar structure with light palette.

---

## Existing Plugins Summary

The codebase includes 30+ plugins:

| Plugin | Keyword | Purpose |
|--------|---------|---------|
| calculator | `=` | Math expressions |
| web_search | `g`, `yt`, `gh`, `wiki`, etc. | Web search |
| bash_exec | `>` | Shell commands |
| app_launcher | - | Launch desktop apps |
| file_read | - | Read files |
| file_write | - | Write files |
| file_search | `*`, `/f` | Find files by name |
| glob | - | Glob patterns |
| grep | - | Search file contents |
| git | - | Git commands |
| clipboard | `cb`, `/clip` | Clipboard history |
| code_tools | - | Code execution, patching |
| color_picker | - | Color format conversion |
| env_vars | - | Environment variables |
| http_client | - | HTTP requests |
| ls | - | List directories |
| network | - | Network info (IP, ports) |
| process_manager | - | List/manage processes |
| shell_plugin | - | Shell integration |
| screenshot | - | Take screenshots |
| script_runner | - | Run scripts |
| selection | - | X11 selection/clipboard (Linux) |
| snippets | - | Code snippets |
| sys_info | - | System information |
| system_commands | - | System actions (lock, sleep, shutdown) |
| timer | - | Timers |
| todo | - | Todo list (with live HTML UI) |
| translate | - | Language translation |
| unit_converter | - | Unit conversions |
| url_opener | - | Open URLs |
| windows_settings | - | Windows-specific settings |
| browser_bookmarks | `b`, `bm` | Browser bookmarks |
| agent_delegate | - | Claude tool calling |

---

## Frontend Dependencies

**package.json:**
```json
{
  "dependencies": {
    "@tauri-apps/api": "^2",
    "react": "^18",
    "react-dom": "^18"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2",
    "@types/react": "^18",
    "@types/react-dom": "^18",
    "@vitejs/plugin-react": "^6.0.2",
    "typescript": "^5",
    "vite": "^8.0.14"
  }
}
```

---

## Backend Dependencies

**Key Dependencies (Cargo.toml):**
```toml
tauri = "2"                         # Desktop framework
serde, serde_json = "1"             # Serialization
tokio = "1"                         # Async runtime
reqwest = "0.12"                    # HTTP client
async-trait = "0.1"                 # Async trait methods
rusqlite = "0.31"                   # SQLite
chrono = "0.4"                      # Date/time
walkdir = "2"                       # Directory traversal
regex = "1"                         # Regular expressions
glob = "0.3"                        # Glob patterns
sysinfo = "0.30"                    # System info
```

---

## Query Processing Flow

```
User Input
    ↓
Frontend handleQueryChange()
    ├─ If "/" prefix → Show slash suggestions (no backend call)
    ├─ If "?" or "ai " → AI mode
    └─ Debounce (100ms) → invoke("search", {query})
         ↓
Backend search() command
    ↓
PluginManager::query_all(raw)
    ├─ Iterate plugins
    ├─ Check keyword match (if has keyword)
    ├─ Call plugin.query()
    ├─ Collect all results
    ├─ Sort by score (descending)
    └─ Limit to top 10
         ↓
Return Vec<QueryResult>
    ↓
Frontend displays in ResultList
```

---

## Action Execution Flow

```
User selects result (Enter or click)
    ↓
Frontend handleExecute(result)
    ├─ If "copy" → navigator.clipboard.writeText()
    ├─ If "slash_complete" → Update input with suggestion
    └─ invoke("execute_result", {result})
         ↓
Backend execute_result() command
    ├─ Match action_type:
    │   ├─ "url" → spawn xdg-open/open/cmd (platform-specific)
    │   ├─ "shell" → spawn shell with command
    │   ├─ "open" → spawn file manager
    │   └─ "todo_*" → execute tool on todo plugin
    └─ Return success bool
         ↓
Frontend updates UI (if needed)
```

---

## AI Chat Flow

```
User types with "?" prefix or in AI mode
    ↓
Frontend handleSubmit(value, forceAi)
    ├─ invoke("ai_query", {query})
    └─ Add user turn to conversationHistory
         ↓
Backend ai_query() command
    ├─ Add to conversation context
    ├─ Get plugin manager, AI client, conversation, skill manager
    ├─ Call Router::ai_route()
    │   ├─ Collect all tool schemas from plugins
    │   ├─ Send to Claude with conversation history
    │   ├─ Claude may call tools → iterate
    │   └─ Return final response
    ├─ Add assistant turn to context
    └─ Return AiResponse { content, tools_used, results }
         ↓
Frontend updates conversationHistory with assistant response
    ├─ Render markdown content
    ├─ Show tool usage chips
    └─ Auto-scroll to bottom
```

---

## Key Design Patterns

1. **Plugin Trait Pattern**: All tools implement Plugin trait with async query()
2. **Tool Schema Pattern**: Plugins provide JSON schemas for Claude tool calling
3. **Keyword-based Routing**: Plugins can have optional keywords for fast lookup
4. **Score-based Sorting**: Results ranked by relevance score
5. **Debounced Search**: Frontend debounces plugin search (100ms)
6. **Async/Await**: All I/O operations are non-blocking (Tokio)
7. **Theme System**: CSS-in-JS with light/dark variants
8. **Global Hotkey**: Tauri native hotkey registration
9. **Tray Icon**: System tray for quick access
10. **Live Server**: Built-in HTTP server for features like todo live editing

---

## File Reading Reference

**Key files read:**
- ✅ src-tauri/Cargo.toml (55 lines)
- ✅ src-tauri/src/lib.rs (54 lines)
- ✅ src-tauri/src/main.rs (739 lines)
- ✅ src-tauri/src/plugins/mod.rs (125 lines)
- ✅ src-tauri/src/plugins/calculator.rs (222 lines)
- ✅ src-tauri/src/plugins/web_search.rs (partial - complex plugin example)
- ✅ src/App.tsx (1096 lines - main frontend)
- ✅ src/components/SearchBar.tsx (298 lines)
- ✅ src/components/ResultList.tsx (210 lines)
- ✅ src/components/SettingsPanel.tsx (242 lines)
- ✅ package.json (24 lines)

---

## Summary

**OmniLauncher** is a well-architected, plugin-based productivity launcher:

- **30+ Plugins** covering file search, web search, code execution, system commands, etc.
- **AI Integration** with Claude for intelligent tool calling
- **Dual-mode Interface**: Fast launcher for common tasks + AI chat for complex queries
- **Cross-platform**: Linux, macOS, Windows support
- **Modern Tech Stack**: Rust backend, React frontend, Tauri framework
- **Extensible**: New plugins can be added by implementing Plugin trait
- **Rich UX**: Keyboard shortcuts, animations, themes, live preview, markdown rendering

The codebase is production-ready and follows Rust best practices (async/await, trait patterns, error handling).
