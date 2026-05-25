# OmniLauncher - Complete File Structure & Source Code Review

## Project Overview
OmniLauncher is a desktop application combining a blazing-fast launcher (written in Rust via Tauri) with an AI-powered assistant. It bridges local productivity tools with LLM-powered intelligence for cross-platform desktop automation.

**Architecture:**
- **Frontend**: React/TypeScript (src/) - Dual-mode UI (launcher + AI chat)
- **Backend**: Rust (src-tauri/src/) - Tauri app with 33 plugins and agentic AI router

---

## Frontend Structure (TypeScript/React)

### File: `/src/main.tsx`
**Purpose**: React application entry point
- Renders React app into #root
- Imports styles.css and App component

### File: `/src/App.tsx` (1,084 lines)
**Core Features**:
- **Dual-mode interface**: Launcher mode (quick search) vs AI Chat mode
- **AI Detection**: `isAiPrefix()` detects `?` or `ai ` prefixes
- **Markdown Renderer**: Full markdown support with tables, lists, code blocks
- **State Management**:
  - Query, results, loading, theme, AI mode toggle
  - Conversation history with multi-turn tracking
  - Settings (theme, hotkey, max_results, AI endpoint)
- **Key Functions**:
  - `renderMarkdown()` - Comprehensive markdown→HTML with table support
  - `slashSuggestions()` - Command autocomplete (/run, /open, /app, /find, etc.)
  - `doSearch()` - Backend plugin query
  - `doAiQuery()` - AI conversation
  - `handleExecute()` - Execute launcher results
- **Keyboard Shortcuts**:
  - Esc: Clear search
  - Ctrl+,: Toggle settings
  - Arrow Up/Down: Navigate results
  - Enter: Execute

### File: `/src/components/SearchBar.tsx`
**UI Component**: Main input bar
- Input with leading icon (spinner when loading, ⌕ when idle, ✦ in AI mode)
- Settings button (⚙)
- AI badge indicator
- **Hint bar**: Shows prefixes for core commands (=/>, */f, b, ?, /) + web search (g, yt, gh, etc.)
- Focus management on AI mode toggle

### File: `/src/components/ResultList.tsx`
**UI Component**: Scrollable results
- Keyboard navigation (up/down arrows)
- Mouse hover selection
- Keyboard shortcuts (⌘1-9 for first 9 items)
- Highlight query matches in titles
- Action badges (↵ Open, ↵ Run, ↵ Copy)
- Staggered fade-in animation

### File: `/src/components/AIResponsePane.tsx`
**Legacy Component**: No-op stub
- Chat rendering moved to App.tsx ChatBubble components
- Preserved for backwards compatibility

### File: `/src/components/SettingsPanel.tsx`
**UI Component**: Settings dialog
- AI Provider URL input
- API Key (password field)
- Model dropdown with search/filter
- Fetches available models from endpoint
- Theme selector (Dark Catppuccin Mocha / Light Catppuccin Latte)
- Read-only hotkey display
- Save button with confirmation

### File: `/src/tauri-api.ts`
**Purpose**: Browser-safe Tauri wrapper
- Provides mock implementations for dev mode (when not in Tauri)
- Returns mock data for search, ai_query, get_settings
- Gracefully falls back to real Tauri API if available

### File: `/src/tauri-shim.ts`
**Purpose**: Global Tauri API shim for browser
- Initializes `window.__TAURI__` if not present
- Provides no-op implementations for event listeners
- Prevents "Tauri not available" errors in browser dev mode

### File: `/src/styles.css`
**Styling**: Global styles using Catppuccin Mocha/Latte themes

### Color Scheme (Dark Mode)
- Background: #1E1E2E
- Surface: #313244, #45475A
- Text: #CDD6F4 (main), #6C7086 (sub)
- Accent: #CBA6F7

---

## Backend Structure (Rust)

### Core Application Files

#### `/src-tauri/src/main.rs` (733 lines)
**Main entry point and Tauri setup**

**Key Structures**:
```rust
pub struct AppState {
    pub plugin_manager: Arc<Mutex<PluginManager>>,
    pub ai_client: Mutex<AiClient>,
    pub settings: Arc<Mutex<AppSettings>>,
    pub conversation: Arc<Mutex<ConversationContext>>,
    pub skill_manager: Arc<Mutex<SkillManager>>,
    pub live_server: LiveServer,
    pub live_server_port: u16,
}
```

**Initialization**:
- Loads settings from ~/.config/omnilauncher/settings.json
- Creates plugin manager (33 plugins registered)
- Initializes AI client with loaded settings
- Starts live server on port 1421 for /todo endpoints
- Registers global shortcut (Ctrl+Shift+O to toggle window)
- Sets up tray icon with tooltip

**Tauri Commands**:
1. `search(query)` - Run all plugins against query
2. `ai_query(query)` - AI route with tool calling
3. `execute_result(result)` - Execute launcher action (open URL, shell, etc.)
4. `slash_preview(query)` - Preview for slash commands (/app, /find, /grep, etc.)
5. `get_settings()` - Return current settings
6. `save_settings_cmd(settings)` - Persist settings, recreate AI client
7. `clear_conversation()` - Clear conversation history
8. `list_models(base_url, api_key)` - Fetch available models from API
9. `list_skills()` - List all loaded skills
10. `reload_skills()` - Hot-reload skills
11. `install_skill(source)` - Install from URL or file path
12. `set_window_geometry(height, ai_mode)` - Adjust window size based on content

**Key Features**:
- Debug logging to ~/.config/omnilauncher/omnilauncher.log (if --debug)
- System tray integration (left-click toggles window visibility)
- Window geometry auto-adjustment for AI mode (40% screen width) vs launcher (33% width)
- Fallback geometry if monitor info unavailable
- Retry logic in external command spawning

**Tests**:
- `spawn_external_command_reports_missing_command_failure()` - Verifies error handling

#### `/src-tauri/src/lib.rs` (51 lines)
**Module exports and plugin registration**

**Module Tree**:
- ai (client, errors, router)
- db (migrations)
- guardrails (safety checks)
- live_server (HTTP for /todo)
- plugins (33 implementations)
- settings (load/save)
- skills (skill manager)

**Plugin Registration** (33 total):
- agent_delegate, app_launcher, bash_exec, browser_bookmarks, calculator, clipboard
- code_tools, color_picker, env_vars, file_read, file_search, file_write, git
- glob, grep, hosts, http_client, ls, network, process_manager, shell_plugin, snippets
- sys_info, system_commands, timer, todo, translate, unit_converter, url_opener
- web_fetch, web_search, windows_settings

#### `/src-tauri/src/settings.rs` (55 lines)
**Configuration management**

```rust
pub struct AppSettings {
    pub ai_base_url: String,
    pub ai_model: String,
    pub ai_api_key: String,
    pub theme: String,
    pub hotkey: String,
    pub max_results: usize,
}
```

**Default values**:
- ai_base_url: "http://localhost:5000"
- ai_model: "auto"
- theme: "dark"
- hotkey: "Alt+Space"
- max_results: 10

**Path**: ~/.config/omnilauncher/settings.json

**Functions**:
- `load_settings()` - Read JSON, fallback to defaults
- `save_settings()` - Pretty-print JSON to config dir

#### `/src-tauri/src/guardrails.rs` (192 lines)
**Safety constraints for shell & file operations**

**GuardrailAction**:
- Allow
- Deny(reason)
- Warn(reason)

**Shell Command Checks** (DENY):
1. Pipe to sh/bash (RCE risk): `| sh`, `|sh`, `| bash`, `|bash`
2. Fork bomb: `:()` pattern
3. Write /etc/passwd or /etc/shadow

**Shell Command Checks** (WARN):
1. `git push --force` (history rewrite risk)
2. Write to /etc/, /sys/, /proc/ (OS break risk)

**File Write Checks** (DENY):
1. /etc/, /sys/, /proc/, c:\windows\system32\

**Tests**: Comprehensive test suite (15 tests) covering DENY, WARN, and ALLOW paths

#### `/src-tauri/src/live_server.rs` (149 lines)
**Simple async HTTP server for /todo live view**

```rust
pub struct LiveResponse {
    pub status: &'static str,
    pub content_type: &'static str,
    pub body: String,
}
```

**Routes**:
- `/health` - Returns `{"ok":true}`
- `/todo` - HTML for todo UI
- `/todo/data` - JSON data for todo items

**Features**:
- Non-blocking TCP listener (tokio)
- Route handler registry
- Cache-Control: no-store for no caching
- Proper HTTP/1.1 response encoding

---

### AI Module

#### `/src-tauri/src/ai/mod.rs` (4 lines)
Module declarations

#### `/src-tauri/src/ai/client.rs` (371 lines)
**LLM client with retry & streaming**

```rust
pub struct AiClient {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}
```

**Key Methods**:
- `chat(messages)` - Simple text completion
- `chat_with_tools(messages, tools)` - **WITH RETRY LOGIC**
  - Max 3 attempts
  - Exponential backoff: 2s → 4s → 8s + 0-1s jitter
  - Retries: network errors, rate-limit, 429, 502, 503
  - Non-retries: 400, 401, 403, 404, 422 (auth/client errors)
- `chat_stream(messages, window)` - SSE streaming with Tauri events

**Message Types**:
```rust
pub struct Message {
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
    pub name: Option<String>,
}
```

**Tool Call Format** (OpenAI-compatible):
```rust
pub struct ToolCall {
    pub id: String,
    pub call_type: Option<String>, // "function"
    pub function: FunctionCall,
}

pub struct FunctionCall {
    pub name: String,
    pub arguments: String, // JSON as string
}
```

**API Compatibility**:
- POST {base_url}/v1/chat/completions
- Bearer auth (if api_key set)
- SSE support for streaming

#### `/src-tauri/src/ai/errors.rs` (108 lines)
**Error classification system**

```rust
pub enum ErrorClass {
    Transient,    // Retry (timeout, rate limit, 429, 502, 503)
    Permanent,    // Don't retry (auth, bad request)
    ModelError,   // Invalid tool call, malformed output
    ResourceError,// Token budget exceeded
}
```

**Classification** via `classify_error(msg)`:
- Keywords matched: "timeout", "rate limit", "429", "503", "502"
- Token patterns: "token" + ("limit"|"budget"|"length"|"exceeded")
- Model errors: "invalid tool", "malformed", "function not found"
- Permanent: everything else

#### `/src-tauri/src/ai/router.rs` (500+ lines)
**Main AI orchestration engine**

```rust
pub struct AiResponse {
    pub content: String,
    pub tools_used: Vec<String>,
    pub results: Vec<QueryResult>,
    pub is_ai: bool,
}

pub enum RouteDecision {
    Local,  // No AI latency — instant plugin results
    Ai,     // AI assistance with tools
}
```

**Router::decide()** (Priority order):
1. Input starts with `?` → AI
2. Input starts with `ai ` (case-insensitive) → AI
3. Everything else → Local (plugins only)

**Router::ai_route()** - Agentic loop (up to 10 iterations):

1. **Build System Prompt** with:
   - OS info (detect Windows, macOS, Linux)
   - Shell hints (PowerShell for Windows, bash/zsh for others)
   - Available tool list
   - Markdown output format requirements

2. **Skill Injection** (Hermes pattern):
   - Find relevant skills by trigger, name, tags
   - Inject skill context as user message before query
   - Example: "--- Active Skills ---" + skill body + query

3. **Agentic Loop**:
   ```
   for iteration 0..10:
       - Compress context if >70% token budget (32k tokens)
       - Call AI with tools
       - If tool_calls:
           - Loop detection: track last 3 fingerprints
           - Execute all tools in parallel
           - Append assistant message + tool results
           - Continue to next iteration
       - Else (no tool_calls):
           - Return final content
   ```

4. **Error Handling**:
   - ModelError → Corrective prompt, retry
   - ResourceError → Compress context, retry
   - Permanent → Return error
   - Transient → Already handled by client retry (if here → exhausted)

5. **Loop Detection**:
   - Fingerprint = tool_name|arguments (joined for all calls)
   - If last 3 iterations identical → Stuck, stop

**System Prompt Template** (dynamically filled):
- Lists 15 available tools
- OS-specific shell instructions
- Markdown formatting rules
- Markdown table support
- Bullet/numbered lists

**OS Detection**:
- Windows → PowerShell syntax hints
- macOS → bash/zsh hints
- Linux → bash/zsh hints

---

### Plugin System

#### `/src-tauri/src/plugins/mod.rs` (122 lines)
**Plugin architecture**

```rust
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn keyword(&self) -> Option<&str>; // "/app" for app launcher
    async fn query(&self, q: &Query) -> Vec<QueryResult>;
    fn tool_schema(&self) -> Option<serde_json::Value>;
    async fn execute_tool(&self, _args: serde_json::Value) -> String;
}

pub struct PluginManager {
    pub plugins: Vec<Box<dyn Plugin>>,
}
```

**PluginManager Methods**:
- `register(p)` - Add plugin
- `query_all(raw_query)` - Run all plugins, sort by score, return top 10
- `all_tool_schemas()` - Collect tool schemas for AI
- `execute_tool(name, args)` - Find plugin by name, execute

**Query Matching**:
1. Parse query into terms
2. For each plugin:
   - If keyword set and query doesn't start with it → skip
   - Run plugin.query()
3. Flatten results, sort by score (descending), truncate to 10

#### Plugin Implementations (33 total)

**File Plugins**:
- `file_read.rs` - Read file contents
- `file_write.rs` - Write to files (with guardrails)
- `file_search.rs` - Find files by name/pattern
- `glob.rs` - Glob patterns

**Search Plugins**:
- `grep.rs` - Regex search in files
- `web_search.rs` - Google, YouTube, GitHub, etc.
- `web_fetch.rs` - Fetch URL content

**System Plugins**:
- `bash_exec.rs` - Run shell commands
- `shell_plugin.rs` - Shell execution wrapper
- `code_tools.rs` - Python, JavaScript, PowerShell, Rust execution

**App/Navigation**:
- `app_launcher.rs` - Launch applications
- `url_opener.rs` - Open URLs/files/apps
- `browser_bookmarks.rs` - Search browser bookmarks

**Utilities**:
- `calculator.rs` - Math expressions
- `clipboard.rs` - Clipboard history
- `color_picker.rs` - Color format conversion (hex/rgb/name)
- `unit_converter.rs` - Unit conversions
- `translate.rs` - Text translation

**System Info**:
- `sys_info.rs` - CPU, memory, processes, uptime
- `process_manager.rs` - List/kill processes
- `network.rs` - IP info, ports, connectivity
- `env_vars.rs` - Environment variables
- `windows_settings.rs` - Windows-specific (lock, sleep, shutdown)
- `system_commands.rs` - System actions

**Productivity**:
- `todo.rs` - Todo list management
- `timer.rs` - Timer/stopwatch
- `snippets.rs` - Code snippet storage
- `hosts.rs` - /etc/hosts editing

**Advanced**:
- `git.rs` - Git operations
- `http_client.rs` - HTTP requests
- `agent_delegate.rs` - Delegate to other agents/skills

---

### Database

#### `/src-tauri/src/db/mod.rs` (100 lines)
**SQLite migration runner**

```rust
pub struct Migration {
    pub version: u32,
    pub sql: &'static str,
}
```

**Migrations** (3 total):
1. `001_create_tables.sql` - Initial schema (todos, etc.)
2. `002_add_extra_columns.sql` - Additional columns
3. `003_add_status_to_todos.sql` - Status tracking

**Features**:
- Idempotent execution (silently ignore if column/table exists)
- Track applied migrations in `_migrations` table
- Execute one statement at a time (handle ALTER TABLE safely)

---

### Skills

#### `/src-tauri/src/skills/mod.rs` (364 lines)
**Skill management & loading system**

```rust
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    pub version: String,
    pub triggers: Vec<String>,
    pub tags: Vec<String>,
    pub tools_hint: Vec<String>,
    pub path: PathBuf,
}

pub struct Skill {
    pub meta: SkillMeta,
    pub body: String,
}
```

**Skill File Format** (SKILL.md with frontmatter):
```yaml
---
name: web-summarizer
description: Fetch and summarize web pages
version: 1.0.0
triggers: [summarize, tldr, summary]
tags: [web, reading]
tools: [web_fetch]
---

When the user asks to summarize a URL, do the following...
```

**SkillManager Methods**:
- `load_all()` - Load bundled + user skills
- `load_from_dir(dir)` - Scan dir for SKILL.md files
- `find_relevant(query)` - Match by trigger/name/tags
- `list_meta()` - Return all skill metadata
- `get_by_name(name)` - Get specific skill
- `install_from_url(url)` - Download, parse, install
- `install_from_path(path)` - Copy local file
- `reload()` - Hot-reload without restart

**Skill Directories**:
- Bundled: assets/skills/ (relative to binary)
- User: ~/.config/omnilauncher/skills/

**Skill Injection** (in router.rs):
- Relevant skills injected as context before user query
- Format: "--- Active Skills ---\n<skill body>\n--- End Skills ---"
- Allows custom AI instructions per query

**Tests** (3 tests):
- Parse frontmatter correctly
- Find skills by trigger
- Ignore irrelevant skills

---

## Key Architectural Patterns

### 1. Dual-Mode UI Architecture
```
Input Query
    ↓
isAiPrefix(query)?
    ├→ No → Plugin System (instant)
    │         └→ Results (searchable, clickable)
    └→ Yes → AI Route (LLM + Tools)
             └→ Conversation (streaming, multi-turn)
```

### 2. Agentic Loop (10 max iterations)
```
User Query
    ↓
Build System Prompt + Context
    ↓
Inject Relevant Skills
    ↓
FOR iteration 0..10:
    ├─ Compress context if >70% token budget
    ├─ Call AI with available tools
    ├─ Parse tool_calls
    ├─ Detect infinite loops (3 identical fingerprints)
    ├─ Execute all tools
    ├─ Append results to context
    └─ Continue if more tool calls needed
    
Final Response → Frontend
```

### 3. Plugin Query Pipeline
```
search_query
    ↓
For each plugin:
    ├─ Check keyword (skip if mismatch)
    ├─ Parse query into terms
    ├─ Plugin.query(terms) → Vec<QueryResult>
    ↓
Sort by score (descending)
    ↓
Truncate to 10
    ↓
Return to frontend
```

### 4. Skill Activation Pattern (Hermes-like)
```
Router detects relevant skills
    ↓
For each relevant skill:
    ├─ Extract skill body
    ├─ Build context message: "--- Active Skill: {name} ---\n{body}"
    ↓
Inject context message before user query
    ↓
AI processes with skill context in scope
```

### 5. Error Recovery Strategy
```
AI API Call
    ├─ Success → Continue
    ├─ ModelError → Corrective prompt + retry
    ├─ ResourceError → Compress context + retry
    ├─ Transient (retried by client) → Fail
    └─ Permanent → Return error
```

---

## Data Flow Summary

### Launcher Query Flow
```
User types in SearchBar
    ↓
onChange → debounce 100ms
    ↓
App.doSearch(query)
    ↓
invoke("search", {query})
    ↓
main.rs: search()
    ↓
PluginManager.query_all(query)
    ↓
ResultList renders top 10
    ↓
User selects item
    ↓
execute_result(result)
    ↓
Platform-specific handler (explorer, open, xdg-open, etc.)
```

### AI Chat Flow
```
User types query with "?" or "ai " prefix
    ↓
isAiPrefix() = true
    ↓
setAiModeEnabled(true)
    ↓
handleSubmit() → doAiQuery(query)
    ↓
invoke("ai_query", {query})
    ↓
main.rs: ai_query()
    ├─ Add user to ConversationContext
    ├─ Router.ai_route()
    │   ├─ Build system prompt
    │   ├─ Inject relevant skills
    │   ├─ Agentic loop (up to 10 iterations)
    │   │   ├─ Call AI client with tools
    │   │   ├─ Execute tools
    │   │   └─ Continue if tool calls
    │   └─ Return final response
    ├─ Add assistant to ConversationContext
    └─ Return AiResponse
    ↓
ChatBubble renders response
    ↓
User sends follow-up → repeat
```

---

## Configuration & Deployment

### Settings File
**Location**: ~/.config/omnilauncher/settings.json

**Schema**:
```json
{
  "ai_base_url": "http://localhost:5000",
  "ai_model": "auto",
  "ai_api_key": "sk-...",
  "theme": "dark",
  "hotkey": "Alt+Space",
  "max_results": 10
}
```

### Global Shortcut
- **Default**: Ctrl+Shift+O
- **Action**: Toggle window visibility
- **Fallback**: Tray icon left-click

### Debug Logging
- **File**: 
  - Windows: %TEMP%/omnilauncher.log
  - Unix: ~/.config/omnilauncher/omnilauncher.log
- **Enable**: Launch with `--debug` flag
- **Level**: Trace

---

## Security & Guardrails

### Shell Command Restrictions
**DENY** (hard block):
- Pipe to sh/bash (RCE)
- Fork bomb patterns
- Write to /etc/passwd or /etc/shadow

**WARN** (user confirmation needed):
- git push --force
- Write to system dirs (/etc/, /sys/, /proc/)

### File Write Restrictions
**DENY**:
- /etc/, /sys/, /proc/ (Unix)
- c:\windows\system32\ (Windows)

---

## File Tree Summary

```
/data/tools/OmniLauncher/
├── src/                          # React/TypeScript frontend
│   ├── App.tsx                  # Main app (1,084 lines)
│   ├── main.tsx                 # React entry
│   ├── styles.css               # Theme colors
│   ├── tauri-api.ts             # Tauri wrapper
│   ├── tauri-shim.ts            # Browser shim
│   └── components/
│       ├── SearchBar.tsx        # Input + hint bar
│       ├── ResultList.tsx       # Results display
│       ├── SettingsPanel.tsx    # Settings dialog
│       └── AIResponsePane.tsx   # Legacy (no-op)
│
└── src-tauri/src/               # Rust backend
    ├── main.rs                  # Tauri app (733 lines)
    ├── lib.rs                   # Module exports
    ├── settings.rs              # Config management
    ├── guardrails.rs            # Safety checks (192 lines)
    ├── live_server.rs           # HTTP for /todo
    │
    ├── ai/
    │   ├── mod.rs
    │   ├── client.rs            # LLM client + retry (371 lines)
    │   ├── errors.rs            # Error classification (108 lines)
    │   └── router.rs            # Agentic orchestration (500+ lines)
    │
    ├── plugins/
    │   ├── mod.rs               # Plugin trait & manager
    │   ├── agent_delegate.rs
    │   ├── app_launcher.rs
    │   ├── bash_exec.rs
    │   ├── browser_bookmarks.rs
    │   ├── calculator.rs
    │   ├── clipboard.rs
    │   ├── code_tools.rs
    │   ├── color_picker.rs
    │   ├── env_vars.rs
    │   ├── file_read.rs
    │   ├── file_search.rs
    │   ├── file_write.rs
    │   ├── git.rs
    │   ├── glob.rs
    │   ├── grep.rs
    │   ├── hosts.rs
    │   ├── http_client.rs
    │   ├── ls.rs
    │   ├── network.rs
    │   ├── process_manager.rs
    │   ├── shell_plugin.rs
    │   ├── snippets.rs
    │   ├── sys_info.rs
    │   ├── system_commands.rs
    │   ├── timer.rs
    │   ├── todo.rs
    │   ├── translate.rs
    │   ├── unit_converter.rs
    │   ├── url_opener.rs
    │   ├── web_fetch.rs
    │   ├── web_search.rs
    │   └── windows_settings.rs
    │
    ├── db/
    │   └── mod.rs               # SQLite migrations (100 lines)
    │
    └── skills/
        └── mod.rs               # Skill manager (364 lines)
```

---

## Code Quality Observations

### Strengths
✅ Well-organized modular architecture
✅ Comprehensive error handling with classification
✅ Safety guardrails for dangerous operations
✅ Retry logic with exponential backoff for AI API calls
✅ Loop detection for agentic iterations
✅ Skill injection pattern for extensible AI behavior
✅ Context compression for long conversations
✅ Theme support with Catppuccin colors
✅ Full Markdown rendering in UI
✅ Cross-platform support (Windows/Mac/Linux)
✅ Hot-reload for skills
✅ OAuth/Bearer auth support

### Areas for Review
⚠️ Large router.rs file (500+ lines) - consider breaking into state machine module
⚠️ Plugin execution without output size limits - potential for large responses
⚠️ Skill body directly injected without sanitization - potential XSS if rendered
⚠️ No rate limiting on plugin execution within agentic loop
⚠️ Conversation context not persisted (lost on app restart)
⚠️ Limited test coverage for plugin system
⚠️ No input validation on slash commands
⚠️ Markdown renderer vulnerable to deeply nested lists/tables (DOS potential)
⚠️ Settings stored as plain JSON (API key not encrypted)

---

## Testing Coverage

**Existing Tests**:
- Guardrails: 15 comprehensive tests
- Error classification: 6 tests
- Skills parsing: 3 tests
- Main.rs: 1 test (spawn_external_command)

**Recommended Test Areas**:
- Plugin integration tests
- Router decision logic
- Agentic loop termination
- Markdown edge cases
- Skill injection
- Conversation context compression

