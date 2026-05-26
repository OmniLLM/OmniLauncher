# OmniLauncher Architecture Diagrams

## High-Level System Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     OMNILAUNCHER APPLICATION                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                  FRONTEND (React 18)                    │   │
│  │  ┌────────────────────────────────────────────────────┐ │   │
│  │  │  App.tsx                                           │ │   │
│  │  │  - Dual mode: Launcher / AI Chat                  │ │   │
│  │  │  - State: query, results, conversationHistory     │ │   │
│  │  │  - Dynamic window geometry                        │ │   │
│  │  └────────────────────────────────────────────────────┘ │   │
│  │                                                          │   │
│  │  ┌────────────────┐  ┌────────────────┐  ┌────────────┐ │   │
│  │  │  SearchBar     │  │  ResultList    │  │ SettingsUI │ │   │
│  │  │  - Input field │  │  - Navigation  │  │ - Config   │ │   │
│  │  │  - Hint bar    │  │  - Selection   │  │ - Themes   │ │   │
│  │  └────────────────┘  └────────────────┘  └────────────┘ │   │
│  │                                                          │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              ↕ (Tauri IPC)                      │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              BACKEND (Rust + Tauri 2)                   │   │
│  │  ┌────────────────────────────────────────────────────┐ │   │
│  │  │  AppState                                          │ │   │
│  │  │  - PluginManager (30+ plugins)                     │ │   │
│  │  │  - AiClient (OpenAI-compatible)                    │ │   │
│  │  │  - ConversationContext                            │ │   │
│  │  │  - SkillManager                                    │ │   │
│  │  │  - Settings (persisted)                           │ │   │
│  │  └────────────────────────────────────────────────────┘ │   │
│  │                                                          │   │
│  │  ┌────────────────────────────────────────────────────┐ │   │
│  │  │  Tauri Commands                                    │ │   │
│  │  │  • search(query) → Vec<QueryResult>              │ │   │
│  │  │  • ai_query(query) → AiResponse                  │ │   │
│  │  │  • execute_result(result) → bool                 │ │   │
│  │  │  • slash_preview(query) → Vec<QueryResult>       │ │   │
│  │  │  • get_settings() / save_settings_cmd()          │ │   │
│  │  │  • list_models() / reload_skills()               │ │   │
│  │  └────────────────────────────────────────────────────┘ │   │
│  │                                                          │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
         ↓                              ↓                    ↓
    ┌────────┐                  ┌──────────────┐    ┌────────────────┐
    │ Claude │                  │   Desktop    │    │   Filesystem   │
    │  API   │                  │  Environment │    │   + SQLite DB  │
    └────────┘                  └──────────────┘    └────────────────┘
```

## Plugin System Architecture

```
┌────────────────────────────────────────────────────────────┐
│                  PLUGIN SYSTEM                             │
├────────────────────────────────────────────────────────────┤
│                                                            │
│  Plugin Trait (async_trait)                              │
│  ┌──────────────────────────────────────────────────────┐ │
│  │ name() → &str                                        │ │
│  │ description() → &str                                 │ │
│  │ keyword() → Option<&str>   // "=", ">", "g", etc.  │ │
│  │ query(&Query) → Vec<QueryResult>     // main search │ │
│  │ tool_schema() → Option<JSON>         // for Claude  │ │
│  │ execute_tool(args) → String          // tool call   │ │
│  └──────────────────────────────────────────────────────┘ │
│                           ↑                                 │
│                    Implemented by:                         │
│  ┌─────────────────────────────────────────────────────┐  │
│  │ 30+ Plugin Types:                                   │  │
│  │                                                      │  │
│  │ • CalculatorPlugin (= expr)                        │  │
│  │ • WebSearchPlugin (g, yt, gh, wiki, ...)          │  │
│  │ • BashExecPlugin (> command)                       │  │
│  │ • AppLauncherPlugin                                │  │
│  │ • FileSearchPlugin (* filename)                    │  │
│  │ • GrepPlugin (search files)                        │  │
│  │ • GitPlugin                                        │  │
│  │ • ClipboardPlugin (cb history)                     │  │
│  │ • TimerPlugin                                      │  │
│  │ • TodoPlugin (with live HTML UI)                  │  │
│  │ • ScreenshotPlugin                                 │  │
│  │ • ... and 18+ more                                 │  │
│  │                                                      │  │
│  └─────────────────────────────────────────────────────┘  │
│                                                            │
│  PluginManager                                            │
│  ┌──────────────────────────────────────────────────────┐ │
│  │ plugins: Vec<Box<dyn Plugin>>                        │ │
│  │                                                       │ │
│  │ register(p: Box<dyn Plugin>)                         │ │
│  │ query_all(raw: &str) → Vec<QueryResult>            │ │
│  │   - Iterate each plugin                            │ │
│  │   - Check keyword match                            │ │
│  │   - Collect results                                │ │
│  │   - Sort by score                                  │ │
│  │   - Limit to top 10                                │ │
│  │                                                       │ │
│  │ all_tool_schemas() → Vec<JSON>                      │ │
│  │ execute_tool(name, args) → String                   │ │
│  └──────────────────────────────────────────────────────┘ │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

## Query Processing Flow

```
┌─────────────────────┐
│  User types input   │
│  e.g., "calc 2+2"   │
└──────────┬──────────┘
           │
           ↓
┌─────────────────────────────────┐
│ Frontend: handleQueryChange()    │
│ - Parse input                   │
│ - Check for "/" prefix → skip   │
│ - Check for "?" / "ai" → AI mode│
│ - Debounce 100ms                │
└──────────┬──────────────────────┘
           │
           ↓
┌─────────────────────────────────┐
│ invoke("search", {query})       │
│ Tauri IPC Call                  │
└──────────┬──────────────────────┘
           │
           ↓
┌──────────────────────────────────────────┐
│ Backend: search() command handler        │
│ state: AppState                          │
│ query: String                            │
└──────────┬───────────────────────────────┘
           │
           ↓
┌──────────────────────────────────────────┐
│ PluginManager::query_all(query)          │
│                                          │
│ For each plugin:                         │
│   if keyword matches OR no keyword:      │
│     results += plugin.query(&Query)      │
│                                          │
│ Sort results by score (descending)       │
│ Limit to top 10                          │
└──────────┬───────────────────────────────┘
           │
           ↓
┌──────────────────────────────────────────┐
│ Return Vec<QueryResult> via Tauri IPC    │
└──────────┬───────────────────────────────┘
           │
           ↓
┌──────────────────────────────────────────┐
│ Frontend: setResults(res)                │
│ ResultList renders results with:         │
│ - Icons                                  │
│ - Titles with query highlight           │
│ - Subtitles                              │
│ - Action badges                          │
│ - Keyboard shortcuts                     │
└──────────────────────────────────────────┘
```

## Example Plugin Query

```
Input: "calc 15 + 25"

Frontend:
  query = "calc 15 + 25"
  → debounce 100ms
  → invoke("search", {query: "calc 15 + 25"})

Backend:
  PluginManager::query_all("calc 15 + 25")
    ├─ CalculatorPlugin.keyword() = Some("=")
    │   "calc 15 + 25" doesn't start with "="
    │   → skip
    │
    ├─ WebSearchPlugin.keyword() = None
    │   → run query()
    │   → no match (no "g ", "yt ", etc. prefix)
    │   → returns []
    │
    ├─ ...other plugins (no keyword or no match)...
    │   → returns []
    │
    └─ Results: []

  Problem: Plugin keyword is "=" but user typed "calc"!
           
  Solution would be: "= 15 + 25"

  PluginManager::query_all("= 15 + 25")
    ├─ CalculatorPlugin.keyword() = Some("=")
    │   "= 15 + 25" starts with "="
    │   → CalculatorPlugin.query(Query {
    │       raw: "= 15 + 25",
    │       terms: ["=", "15", "+", "25"]
    │     })
    │   → evaluate("15 + 25") = 40.0
    │   → returns [QueryResult {
    │       id: "calc:15 + 25",
    │       title: "15 + 25 = 40",
    │       subtitle: "Press Enter to copy result",
    │       icon: "🧮",
    │       score: 100,
    │       action_type: "copy",
    │       action_data: "40"
    │     }]

  Final results: [QueryResult { title: "15 + 25 = 40", ... }]

Frontend:
  Renders:
    🧮  15 + 25 = 40
        Press Enter to copy result              ↵ Copy  ⌘1

User presses Enter:
  → handleExecute(result)
  → result.action_type === "copy"
  → navigator.clipboard.writeText("40")
  → Success!
```

## AI Chat Flow with Tool Calling

```
┌──────────────────────────────────────┐
│ User in AI mode                      │
│ Input: "? what's 25% of 200?"        │
└──────────┬───────────────────────────┘
           │
           ↓
┌──────────────────────────────────────┐
│ Frontend: handleSubmit()             │
│ - Detect "?" prefix                  │
│ - Add to conversationHistory (user)  │
│ - invoke("ai_query", {query})        │
│ - Show loading state                 │
└──────────┬───────────────────────────┘
           │
           ↓
┌──────────────────────────────────────────────────┐
│ Backend: ai_query() command                      │
│ - Add user message to ConversationContext        │
│ - Get PluginManager + AiClient                   │
│ - Call Router::ai_route()                        │
└──────────┬─────────────────────────────────────────┘
           │
           ↓
┌────────────────────────────────────────────────────────┐
│ Router::ai_route()                                     │
│                                                        │
│ 1. Collect all plugin tool schemas                    │
│    - calculator schema { name: "calculator", ... }   │
│    - web_search schema { name: "web_search", ... }   │
│    - ... 30+ more                                     │
│                                                        │
│ 2. Build Claude system prompt + tools               │
│                                                        │
│ 3. Send to Claude API:                              │
│    POST https://api.anthropic.com/v1/messages       │
│    {                                                 │
│      model: "claude-3-5-sonnet-...",               │
│      system: [...with tool schemas...],             │
│      messages: [                                     │
│        {role: "user", content: "...history..."},    │
│        {role: "user", content: "25% of 200?"}       │
│      ],                                              │
│      tools: [tool_schemas]                          │
│    }                                                 │
│                                                        │
│ 4. Claude responds with tool_use block:             │
│    - Recognizes need for calculator tool            │
│    - Returns: toolUse {                             │
│        name: "calculator",                          │
│        input: { expression: "200 * 0.25" }         │
│      }                                               │
│                                                        │
│ 5. Execute tool locally:                            │
│    PluginManager::execute_tool("calculator", ...)   │
│    → CalculatorPlugin::execute_tool()               │
│    → evaluate("200 * 0.25")                         │
│    → Returns "50"                                    │
│                                                        │
│ 6. Send back to Claude:                             │
│    messages: [                                       │
│      {role: "user", content: "25% of 200?"},       │
│      {role: "assistant", content: "", tool_use: ...},
│      {role: "user", content: "Tool result: 50"}    │
│    ]                                                 │
│                                                        │
│ 7. Claude generates final response:                 │
│    "25% of 200 is 50."                              │
│                                                        │
│ 8. Return AiResponse {                              │
│      content: "25% of 200 is 50.",                  │
│      tools_used: ["calculator"],                    │
│      results: [...]                                 │
│    }                                                 │
└──────────┬──────────────────────────────────────────────┘
           │
           ↓
┌──────────────────────────────────────────────────────┐
│ Backend returns to Frontend via Tauri                │
└──────────┬───────────────────────────────────────────┘
           │
           ↓
┌──────────────────────────────────────────────────────┐
│ Frontend: setConversationHistory()                   │
│ - Add assistant response to history                 │
│ - Render:                                            │
│   - User bubble: "what's 25% of 200?"              │
│   - Tool chips: [calculator]                        │
│   - AI bubble: "25% of 200 is 50."                  │
│ - Auto-scroll to bottom                             │
└──────────────────────────────────────────────────────┘
```

## Frontend Component Hierarchy

```
┌──────────────────────────────────────────────────────┐
│  App.tsx                                             │
│  - State management                                  │
│  - Event listeners (hotkey, selection, etc.)         │
│  - Main layout (launcher vs AI mode)                 │
├──────────────────────────────────────────────────────┤
│                    │                                  │
│        ┌───────────┼───────────┐                     │
│        │           │           │                     │
│   ┌────▼─────┐ ┌───▼────┐ ┌───▼────────────┐       │
│   │SearchBar │ │Results │ │ Settings Panel │       │
│   └──────────┘ │List    │ └────────────────┘       │
│                │        │                           │
│   ┌──────────┐ └────────┘   ┌────────────────┐     │
│   │Chat Area │              │AI Response     │     │
│   │(in AI    │              │Pane            │     │
│   │mode)     │              └────────────────┘     │
│   └──────────┘                                      │
│                                                      │
│   SearchBar features:                               │
│   - Input field with ref forwarding                 │
│   - Leading icon (spinner/AI/search)                │
│   - Hint bar (prefixes for empty query)             │
│   - Settings button                                 │
│                                                      │
│   ResultList features:                              │
│   - Keyboard navigation                             │
│   - Mouse selection                                 │
│   - Title highlight                                 │
│   - Staggered fade-in                               │
│   - Icon + title + subtitle                         │
│   - Action badge                                    │
│                                                      │
│   SettingsPanel features:                           │
│   - API provider URL input                          │
│   - API key input (password)                        │
│   - Model dropdown (searchable, auto-fetches)       │
│   - Theme selector                                  │
│   - Hotkey display (read-only)                      │
│   - Save button with feedback                       │
│                                                      │
└──────────────────────────────────────────────────────┘
```

## Data Flow: Search to Execution

```
┌──────────────────────────────────────────────────────────────┐
│ SEARCH AND EXECUTION DATA FLOW                               │
├──────────────────────────────────────────────────────────────┤
│                                                               │
│  User Input                                                  │
│     │                                                         │
│     ├─ Type "* readme"                                       │
│     │  ├─ handleQueryChange() → doSearch(q)                 │
│     │  ├─ invoke("search", {query: "* readme"})             │
│     │  └─ Backend returns:                                  │
│     │     Vec[                                               │
│     │       {id: "file1", title: "README.md", ...},         │
│     │       {id: "file2", title: "readme.txt", ...}         │
│     │     ]                                                  │
│     │  └─ setResults(res) → ResultList renders              │
│     │  └─ User sees 2 file results                          │
│     │                                                         │
│     └─ User presses Enter (on "README.md")                  │
│        ├─ handleExecute(result)                             │
│        ├─ action_type = "open"                              │
│        ├─ invoke("execute_result", {result})                │
│        └─ Backend:                                          │
│           ├─ spawn_external_command(                        │
│           │   "xdg-open"  // Linux                          │
│           │   "/home/user/README.md"                        │
│           │ )                                                │
│           └─ Return true                                    │
│        └─ Frontend: onExecute completed                     │
│           ├─ Window shrinks to compact mode                 │
│           ├─ Query cleared                                  │
│           └─ User sees file manager with file selected      │
│                                                               │
│  Query Result Structure:                                     │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ QueryResult {                                          │ │
│  │   id: String,              // "file:path/to/readme"   │ │
│  │   title: String,           // "README.md"             │ │
│  │   subtitle: Option,        // "/path/to"              │ │
│  │   icon: Option,            // "📄"                    │ │
│  │   score: i32,              // 95                      │ │
│  │   action_type: String,     // "open"                  │ │
│  │   action_data: String,     // "/path/to/readme.md"    │ │
│  │ }                                                      │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                               │
│  Action Types:                                               │
│  - "url" → Open in browser                                  │
│  - "shell" → Run command                                    │
│  - "open" → Open file/folder                                │
│  - "copy" → Copy to clipboard                               │
│  - "open_app" → Launch application                          │
│  - "help_command" → Show help                               │
│  - "slash_complete" → Auto-complete slash command           │
│  - "todo_*" → Todo-specific actions                         │
│                                                               │
└──────────────────────────────────────────────────────────────┘
```

