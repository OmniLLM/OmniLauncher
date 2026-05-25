# OmniLauncher: Complete Architecture & File Analysis

## 📋 Project Overview

**OmniLauncher** is an AI-native, cross-platform launcher application built with:
- **Frontend**: React + TypeScript + Vite
- **Backend**: Rust + Tauri v2 (native desktop framework)
- **AI Integration**: OpenAI-compatible API client with agentic tool calling
- **Version**: 2.0.0
- **Platforms**: Windows, macOS, Linux

### Core Purpose
A keyboard-driven launcher (Ctrl+Shift+O) that operates in two modes:
1. **Launcher Mode** (default): Instant local results from 30+ plugins
2. **AI Chat Mode** (`?` or `ai ` prefix): AI assistant with tool-calling capabilities

---

## 🏗️ Architecture Overview

```
OmniLauncher
├── src/                          # Frontend (React/TypeScript)
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── main.rs              # Tauri app entry point & commands
│   │   ├── lib.rs               # Library root, plugin registration
│   │   ├── plugins/             # 30+ plugin implementations
│   │   ├── ai/                  # AI client & routing logic
│   │   ├── skills/              # Skill system (SKILL.md files)
│   │   ├── settings.rs          # Settings persistence
│   │   └── guardrails.rs        # Safety checks
│   ├── Cargo.toml               # Rust dependencies
│   └── build.rs                 # Tauri build script
├── assets/                       # Bundled skills & static assets
├── package.json                 # Node dependencies
└── Makefile                      # Build commands
```

---

## 📁 Key Files & Their Contents

### **src-tauri/Cargo.toml** (41 lines)
Dependencies for Rust backend:
- tauri v2, tokio v1, reqwest v0.12
- serde/serde_json, async-trait, dirs, walkdir, regex, sysinfo
- Optimized release profile: LTO, strip symbols

### **src-tauri/src/main.rs** (390 lines)
**Tauri App Entry Point**:
- AppState struct: manages plugins, AI client, settings, conversation context, skill manager
- Window setup: resize to 50% screen, center, global shortcut Ctrl+Shift+O
- Tauri Commands (RPC):
  - `search(query)` → local plugin results
  - `ai_query(query)` → AI with tool calling
  - `execute_result(result)` → execute action (shell/URL/app)
  - `slash_preview(query)` → preview `/command` results
  - `list_models(base_url, api_key)` → fetch available models
  - `get_settings()` / `save_settings_cmd(settings)`
  - `clear_conversation()`
  - `list_skills()` / `reload_skills()` / `install_skill(source)`

### **src-tauri/src/lib.rs** (49 lines)
**Module exports** and plugin registration:
```rust
pub fn create_plugin_manager() -> PluginManager {
    // Registers 30+ plugins:
    agent_delegate, app_launcher, bash_exec, browser_bookmarks,
    calculator, clipboard, code_tools, color_picker, env_vars,
    file_read, file_search, file_write, git, glob, grep, hosts,
    http_client, ls, network, process_manager, shell_plugin,
    snippets, sys_info, system_commands, timer, todo, translate,
    unit_converter, url_opener, web_fetch, web_search, windows_settings
}
```

### **src-tauri/src/settings.rs** (55 lines)
**Application Settings** (stored in `~/.config/omnilauncher/settings.json`):
```rust
pub struct AppSettings {
    pub ai_base_url: String,      // e.g., "http://localhost:5000"
    pub ai_model: String,         // e.g., "gpt-4", "claude-opus"
    pub ai_api_key: String,       // Bearer token
    pub theme: String,            // "dark"
    pub hotkey: String,           // "Alt+Space" (unused, hardcoded to Ctrl+Shift+O)
    pub max_results: usize,       // 10
}
```

### **src-tauri/src/plugins/mod.rs** (122 lines)
**Core Plugin System**:
```rust
#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn keyword(&self) -> Option<&str>;           // e.g., Some("= ")
    async fn query(&self, q: &Query) -> Vec<QueryResult>;
    fn tool_schema(&self) -> Option<serde_json::Value>;  // OpenAI format
    async fn execute_tool(&self, args: serde_json::Value) -> String;
}

pub struct QueryResult {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
    pub score: i32,           // Higher = ranked first
    pub action_type: String,  // "shell", "url", "open", "copy"
    pub action_data: String,  // Command/URL/path to execute
}
```

### **src-tauri/src/plugins/agent_delegate.rs** (118 lines)
**Delegate to External AI Agents**: @claude, @codex, @omnicode, @opencode
- Pattern: `@agent_name prompt text`
- Executes: `agent_name -p "prompt text"`
- 60-second timeout per execution

### **src-tauri/src/plugins/bash_exec.rs** (100+ lines)
**Execute Shell Commands** (keyword `> `)
- Windows: PowerShell
- macOS/Linux: bash/sh
- Tool name: `shell_exec`
- Parameters: command, working_dir (optional)

### **src-tauri/src/plugins/file_read.rs** (75 lines)
**Read File Contents**
- Tool name: `file_read`
- Parameters: path, start_line (1-based, optional), end_line (optional)
- Output: 8000 char limit with line numbers

### **src-tauri/src/ai/router.rs** (1,151 lines)
**THE BRAIN OF OMNILAUNCHER**: Routing, agentic loop, skill injection, context management

**Key Structures**:
```rust
pub enum RouteDecision { Local, Ai }  // Local plugins vs AI

pub struct AiResponse {
    pub content: String,
    pub tools_used: Vec<String>,      // Tool/skill badges
    pub results: Vec<QueryResult>,
    pub is_ai: bool,
}

pub struct ConversationContext {
    pub messages: Vec<Message>,
    pub max_turns: usize,             // Default 10
}
```

**Key Methods**:

1. **`decide(input) -> RouteDecision`**
   - Input starts with `?` or `ai ` → Ai mode
   - Otherwise → Local mode

2. **`route(input, ...)` → `AiResponse`**
   - Main entry point
   - Routes to local plugins or AI

3. **`ai_route(query, ...)` → `AiResponse`** (Agentic Loop, 6 iterations max)
   - Find relevant skills by trigger/tag match
   - Inject skill Markdown as system message
   - Call AI with tools
   - If tool_calls returned:
     - Loop detection: track 3 most recent fingerprints
     - Execute tools via PluginManager
     - Append results to context
     - Continue loop
   - If no tool_calls: break with response
   - Context compression: if tokens > 70% of 32k budget, keep only last 6 messages

**Slash Commands** (instant, no AI):
- `/run` - Execute shell
- `/open` - Open app/file/URL
- `/app` - Fuzzy launch app
- `/find` - Find files
- `/grep` - Search files
- `/cat` - Read file
- `/ls` - List directory
- `/git` - Git command
- `/calc` - Calculator
- `/todo` - Todo list
- `/web` - Web search
- `/ip`, `/ports`, `/ps`, `/kill` - System info
- `/env` - Environment variable
- `/color` - Color converter
- `/sys` - Shutdown/sleep/lock
- `/clip` - Clipboard history
- `/help` - Help
- `/skill` - Skill management

### **src-tauri/src/ai/client.rs** (270 lines)
**OpenAI-Compatible HTTP Client**:
```rust
pub struct AiClient {
    pub base_url: String,     // e.g., "http://localhost:5000"
    pub api_key: String,
    pub model: String,
}
```

**Methods**:
- `chat(messages) -> Result<String>` - Simple chat
- `chat_with_tools(messages, tools) -> Result<ChatResponse>` - With tool calling
- `chat_stream(messages, window)` - Streaming (test stub)

**Retry Logic**:
- Max 3 attempts
- Exponential backoff: 2s, 4s (+ 0-1s jitter)
- Retries: rate limit, 429, 502, 503, network errors
- Fails fast: 400, 401, 403, 404, 422 (auth errors)

**Data Structures**:
```rust
pub struct Message {
    pub role: String,        // "system", "user", "assistant", "tool"
    pub content: String,
}

pub struct ToolCall {
    pub id: String,
    pub function: FunctionCall,
}

pub struct ChatResponse {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}
```

### **src-tauri/src/ai/errors.rs** (72 lines)
**Error Classification**:
```rust
pub enum ErrorClass {
    Transient,      // Timeout, rate limit → retry
    Permanent,      // Auth error, bad request → fail
    ModelError,     // Invalid tool call → corrective message
    ResourceError,  // Token limit → compress context & retry
}
```

Used by router to guide recovery strategy.

### **src-tauri/src/guardrails.rs** (177 lines)
**Safety Checks**:
- **Shell Commands**: Deny pipe-to-sh, fork bomb, /etc/passwd writes
  Warn on git push --force, /etc/ writes
- **File Operations**: Deny writes to /etc/, /sys/, /proc/, C:\Windows\System32\

(Defined but not actively enforced in current implementation)

### **src-tauri/src/skills/mod.rs** (366 lines)
**Skill System**: SKILL.md-based instruction injection

**Format** (Markdown with YAML frontmatter):
```markdown
---
name: web-summarizer
description: Fetch and summarize web pages
version: 1.0.0
triggers: [summarize, tldr, summary]
tags: [web, reading]
tools: [web_fetch]
---

When the user asks to summarize a URL...
```

**Data Structures**:
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
    pub body: String,      // Markdown instructions
}

pub struct SkillManager {
    skills: Vec<Skill>,
}
```

**SkillManager Methods**:
- `load_all()` - Load bundled + user skills (user overrides bundled)
- `find_relevant(query)` - Match by trigger/tag/name
- `install_from_url(url)` - Download & install via curl
- `install_from_path(path)` - Copy local SKILL.md
- `reload()` - Hot-reload all skills

**Skill Directory**: `~/.config/omnilauncher/skills/<name>/SKILL.md`

---

## 🔌 Plugin Ecosystem (30+ Plugins)

| Plugin | Keyword | Purpose |
|--------|---------|---------|
| agent_delegate | @agent | Delegate to external agents |
| app_launcher | - | Fuzzy search & launch apps |
| bash_exec / shell_plugin | > | Execute shell commands |
| browser_bookmarks | bm | Chrome/Edge bookmarks |
| calculator | = | Math expressions |
| clipboard | cb | Clipboard history |
| code_tools | - | Code execution (Python, JS, Rust, Bash) |
| color_picker | color | Hex/RGB/HSL conversion |
| env_vars | env | Environment variables |
| file_read | - | Read files (AI tool) |
| file_search | f | Find files by name |
| file_write | - | Write files (AI tool) |
| git | git | Git operations |
| glob | - | Glob matching (AI tool) |
| grep | grep | Regex search (AI tool) |
| hosts | hosts | /etc/hosts file |
| http_client | - | HTTP requests (AI tool) |
| ls | - | List directories (AI tool) |
| network | net | IP, ping, DNS, ports, WiFi |
| process_manager | ps | List/kill processes |
| snippets | snip | Text snippets |
| sys_info | - | System info (AI tool) |
| system_commands | sys | Lock/sleep/shutdown |
| timer | timer | Countdown timers |
| todo | todo | Todo list (AI tool) |
| translate | - | Language translation |
| unit_converter | conv | Unit conversions |
| url_opener | - | Open URLs |
| web_fetch | - | Fetch & strip URLs (AI tool) |
| web_search | g | Google/YouTube/GitHub |
| windows_settings | settings | Windows Settings shortcuts |

---

## 🎯 Data Flows

### **Launcher Mode (Local)**
```
User Query
  ↓
search(query) [Tauri command]
  ↓
PluginManager::query_all(query)
  ├─ Each plugin filters by keyword
  ├─ Collects QueryResult from matches
  └─ Sorts by score, returns top 10
  ↓
Frontend displays results
  ↓
User selects result
  ↓
execute_result(result) [Tauri command]
  ↓
std::process::Command::new() executes action
```

### **AI Chat Mode**
```
User Query (? or ai prefix)
  ↓
ai_query(query) [Tauri command]
  ↓
ConversationContext::add_user(query)
  ↓
Router::ai_route(query, plugin_manager, ai_client, context, skill_manager)
  ↓
[Find relevant skills by trigger/tag matching]
  ↓
[Inject skill Markdown as system message]
  ↓
[Agentic Loop: Up to 6 iterations]
  ├─ AiClient::chat_with_tools(messages, tools)
  ├─ If tool_calls returned:
  │  ├─ Loop detection (fingerprint tracking)
  │  ├─ Execute each tool via PluginManager
  │  ├─ Append tool results to context
  │  └─ Continue to next iteration
  └─ If no tool_calls: break with response
  ↓
[Context compression if needed: keep last 6 messages]
  ↓
[If final_content empty: ask AI to summarize tool results]
  ↓
ConversationContext::add_assistant(response)
  ↓
Return AiResponse { content, tools_used, results, is_ai: true }
  ↓
Frontend displays chat bubbles + tool chips
```

---

## 💡 Key Design Patterns

### **Trait-Based Plugin System**
- Plugins implement `Plugin` trait
- Each plugin: query() for launcher, tool_schema() for AI, execute_tool() for both
- PluginManager orchestrates all plugins

### **Async/Await + Tokio**
- All I/O (HTTP, file, shell) is async
- `#[async_trait]` for trait methods
- Mutex-wrapped AppState for thread-safe shared access

### **OpenAI-Compatible API**
- Any model/provider supporting `/v1/chat/completions`
- Tool calling via `tool_choice: "auto"`
- Configurable base_url + api_key in settings

### **Context Management**
- Sliding-window compression
- Max 10 turns (20 messages) normally
- If tokens > 70% of 32k budget: keep only last 6 messages

### **Skill Injection**
- Find skills matching query triggers/tags
- Inject Markdown body as system context before user query
- User can install custom skills from URLs/paths

### **Error Classification**
- Transient (retry) vs Permanent (fail) vs ModelError (correct) vs ResourceError (compress)
- Guides recovery strategy in agentic loop

### **Loop Detection**
- Tracks last 3 tool-call fingerprints
- If identical 3x in a row: break to prevent infinite recursion

---

## 📝 Code Statistics

| File | Lines | Purpose |
|------|-------|---------|
| main.rs | 390 | Tauri app, commands, window |
| lib.rs | 49 | Module root, plugin registration |
| settings.rs | 55 | AppSettings struct, persistence |
| plugins/mod.rs | 122 | Plugin trait, PluginManager |
| plugins/agent_delegate.rs | 118 | Agent delegation |
| plugins/bash_exec.rs | 100+ | Shell execution |
| plugins/file_read.rs | 75 | File reading |
| ai/router.rs | 1,151 | Routing, agentic loop, skills |
| ai/client.rs | 270 | HTTP client, retry logic |
| ai/errors.rs | 72 | Error classification |
| guardrails.rs | 177 | Safety checks |
| skills/mod.rs | 366 | Skill manager, parsing |
| Cargo.toml | 41 | Dependencies |
| build.rs | 3 | Tauri build |

**Total Core Rust**: ~3,000 lines of code

---

## 🔮 Extensibility

- **Custom Skills**: Add SKILL.md files to `~/.config/omnilauncher/skills/<name>/`
- **Custom Plugins**: Implement `Plugin` trait, register in `lib.rs::create_plugin_manager()`
- **Custom AI Provider**: Change `ai_base_url` + `ai_model` in settings
- **Custom UI**: React frontend fully customizable
- **Guardrails**: Can be activated/enforced in plugins
- **Streaming**: `chat_stream()` ready (currently test stub)

---

## 🚀 Startup Sequence

1. **main.rs::run()**
   - Load settings from `~/.config/omnilauncher/settings.json`
   - Create AiClient with settings
   - Create SkillManager, load all skills
   - Create AppState (PluginManager, etc.)
   - Register Tauri commands
   - Set up global shortcut Ctrl+Shift+O
   - Resize and center window

2. **SkillManager::load_all()**
   - Create `~/.config/omnilauncher/skills/` if missing
   - Load bundled skills from `assets/skills/`
   - Load user skills, user overrides bundled
   - Parse each SKILL.md for metadata + body

3. **PluginManager registration**
   - All 30+ plugins instantiated and added
   - Ready to respond to queries

4. **Frontend**
   - React app loads
   - Listens for Tauri commands
   - Keyboard shortcuts active

---

## 🎓 Most Complex Sections

1. **Agentic Loop** (router.rs, lines 283-440)
   - Tool calling orchestration
   - Loop detection
   - Context compression
   - Error recovery

2. **Retry Logic** (client.rs, lines 76-128)
   - Exponential backoff + jitter
   - Selective retry based on error class

3. **Skill Injection** (router.rs, lines 240-264)
   - Find relevant skills
   - Inject Markdown as system context
   - Position before user's actual query

4. **Plugin System** (plugins/mod.rs)
   - Query routing to multiple plugins
   - Result collection and sorting
   - Tool schema aggregation

---

This is the complete architecture of OmniLauncher. All files have been read and analyzed comprehensively.
