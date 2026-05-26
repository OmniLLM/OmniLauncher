# OmniLauncher Codebase Exploration - Complete Index

## 📋 Documents Created

This exploration generated three comprehensive documents:

### 1. **CODEBASE_EXPLORATION.md** (Recommended: Read First)
- **Length**: ~800 lines
- **Content**: Detailed architecture breakdown with code examples
- **Sections**:
  - Project overview & tech stack
  - Complete directory structure
  - Core backend architecture (Plugin system, Tauri app state, AI integration)
  - Frontend architecture (Main app, components, themes, keywords)
  - Existing 30+ plugins summary
  - Dependencies (frontend & backend)
  - Query processing flow
  - Action execution flow
  - AI chat flow with tool calling
  - Key design patterns
  - File reading reference

**Best for**: Understanding the overall architecture, learning how plugins work, understanding data flow

---

### 2. **ARCHITECTURE_DIAGRAMS.md** (Recommended: Read Second)
- **Length**: ~400 lines
- **Content**: ASCII diagrams and visual representations
- **Sections**:
  - High-level system architecture diagram
  - Plugin system architecture
  - Query processing flow diagram
  - Example plugin query (calculator)
  - AI chat flow with tool calling (step-by-step)
  - Frontend component hierarchy
  - Data flow: Search to execution

**Best for**: Visual learners, understanding relationships between components, following data flow

---

### 3. **EXPLORATION_SUMMARY.txt** (Recommended: Read Third)
- **Length**: ~300 lines
- **Content**: Executive summary and quick reference
- **Sections**:
  - Files examined checklist
  - Core architecture insights (4 main topics)
  - Key data structures (QueryResult, AppSettings, ConversationTurn)
  - Plugin interface (complete)
  - Tauri commands reference
  - Frontend state management
  - UI hierarchy
  - Complete plugins list (34 items)
  - Step-by-step query flow
  - Color schemes
  - Key patterns & best practices
  - Next steps for developers

**Best for**: Quick reference, onboarding new developers, understanding specific components

---

## 🎯 How to Use These Documents

### For Quick Understanding (15 minutes)
1. Read EXPLORATION_SUMMARY.txt sections:
   - Core Architecture Insights
   - Key Data Structures
   - Plugin Interface
   - Query Flow

### For Complete Understanding (1 hour)
1. Read CODEBASE_EXPLORATION.md completely
2. Study ARCHITECTURE_DIAGRAMS.md for visual understanding
3. Reference EXPLORATION_SUMMARY.txt as needed

### For Implementation Tasks

#### Adding a New Plugin
1. Study "Plugin Interface (Complete)" in EXPLORATION_SUMMARY.txt
2. Read calculator.rs example in CODEBASE_EXPLORATION.md
3. Review "Plugin System Architecture" in ARCHITECTURE_DIAGRAMS.md
4. Look at src-tauri/src/plugins/mod.rs for registration pattern

#### Modifying Frontend
1. Study "Main App Component" in CODEBASE_EXPLORATION.md
2. Review "Frontend Component Hierarchy" in ARCHITECTURE_DIAGRAMS.md
3. Check App.tsx state management in EXPLORATION_SUMMARY.txt

#### Understanding AI Integration
1. Read "AI Integration" section in CODEBASE_EXPLORATION.md
2. Study "AI Chat Flow with Tool Calling" in ARCHITECTURE_DIAGRAMS.md
3. Review AiResponse structure in EXPLORATION_SUMMARY.txt

#### Debugging a Feature
1. Find the feature in the Complete Plugins List
2. Review its query flow in step-by-step section
3. Check Action Types in EXPLORATION_SUMMARY.txt

---

## 📂 Files Examined

### Backend (Rust)
- ✅ `src-tauri/Cargo.toml` (55 lines)
- ✅ `src-tauri/src/lib.rs` (54 lines)
- ✅ `src-tauri/src/main.rs` (739 lines)
- ✅ `src-tauri/src/plugins/mod.rs` (125 lines)
- ✅ `src-tauri/src/plugins/calculator.rs` (222 lines)
- ✅ `src-tauri/src/plugins/web_search.rs` (partial, 100+ lines)
- ✅ 30+ additional plugin files (directory listing)

**Total Backend Code Examined**: ~1,500+ lines

### Frontend (React/TypeScript)
- ✅ `src/App.tsx` (1,096 lines)
- ✅ `src/components/SearchBar.tsx` (298 lines)
- ✅ `src/components/ResultList.tsx` (210 lines)
- ✅ `src/components/SettingsPanel.tsx` (242 lines)
- ✅ `src/components/AIResponsePane.tsx` (stub)
- ✅ `package.json` (24 lines)

**Total Frontend Code Examined**: ~2,000+ lines

**Total Code Examined**: ~3,500+ lines

---

## 🔍 Key Findings Summary

### Architecture Pattern: Plugin System
- **30+ plugins** implementing a trait-based architecture
- **PluginManager** handles plugin discovery, querying, and tool calling
- Each plugin provides: `query()`, `tool_schema()`, `execute_tool()`
- **Keyword-based routing**: Optional prefix triggers (e.g., "=" for calculator)

### Frontend Pattern: Dual Mode
- **Launcher mode**: Fast plugin search with instant results
- **AI mode**: Chat with Claude, tool calling integrated
- **Dynamic window sizing**: 56px (compact) → 520px (expanded) → 560px (chat)
- **Keyboard-first**: Arrow navigation, Enter to execute, Escape to cancel

### Backend Pattern: Async/Await
- **Tokio runtime** for non-blocking I/O
- **Arc<Mutex<T>>** for thread-safe shared state
- **Tauri IPC** for frontend-backend communication
- **Global hotkey** registration (Ctrl+Shift+O)

### AI Pattern: Tool Calling Loop
1. Plugins provide tool schemas to Claude
2. Claude determines which tools to use
3. Backend executes tools locally
4. Results sent back to Claude
5. Claude generates final response

---

## 🚀 Quick Start Guides

### To Add a New Plugin
```
1. File: src-tauri/src/plugins/my_plugin.rs
2. Implement Plugin trait with:
   - name() → plugin identifier
   - description() → user-friendly text
   - keyword() → optional trigger (e.g., "=" for calculator)
   - query() → search logic, returns Vec<QueryResult>
   - tool_schema() [optional] → JSON schema for Claude
   - execute_tool() [optional] → Claude tool calling
3. Register in src-tauri/src/lib.rs::create_plugin_manager()
```

### To Modify the Frontend
```
1. Edit src/App.tsx for main logic/state
2. Edit src/components/{Component}.tsx for UI
3. Update DARK_COLORS/LIGHT_COLORS for themes
4. Test keyboard shortcuts (arrows, enter, escape)
5. Verify window geometry updates
```

### To Debug a Search Query
```
1. Frontend: Check handleQueryChange() in App.tsx
2. Query dispatch: Verify invoke("search", {query})
3. Backend: Check search() command in main.rs
4. Plugin matching: Review PluginManager::query_all()
5. Result rendering: Check ResultList component
```

---

## 📊 Architecture Quick Reference

### Data Flow: Input → Output
```
User Input
  ↓
Frontend: handleQueryChange() / Debounce 100ms
  ↓
invoke("search", {query})
  ↓
Backend: search() command
  ↓
PluginManager::query_all()
  ↓
Plugin.query() × N plugins
  ↓
Sort by score, limit to 10
  ↓
Return Vec<QueryResult>
  ↓
Frontend: setResults() → ResultList renders
  ↓
User selects → handleExecute()
  ↓
invoke("execute_result", {result})
  ↓
Backend: execute by action_type
  ↓
Result: Browser open / Command run / File open / etc.
```

### Key State Variables (Frontend)
```
query: String                    // Current input
results: QueryResult[]          // Search results
aiModeEnabled: boolean          // Launcher vs Chat
conversationHistory: Turn[]     // Chat history
loading: boolean                // Search in progress
theme: "dark" | "light"        // Current theme
showSettings: boolean           // Settings visibility
settings: AppSettings | null    // App configuration
```

### Plugin Interface
```rust
trait Plugin {
  fn name() → &str                              // "calculator"
  fn description() → &str                       // "Evaluate expressions"
  fn keyword() → Option<&str>                   // Some("=")
  async fn query(&Query) → Vec<QueryResult>     // Main logic
  fn tool_schema() → Option<Value>              // For Claude
  async fn execute_tool(Value) → String         // Tool call
}
```

---

## 🎓 Learning Path

### Beginner (Just want to know what it does)
1. Read: EXPLORATION_SUMMARY.txt - "Core Architecture Insights"
2. Skim: ARCHITECTURE_DIAGRAMS.md - High-level diagrams
3. Done! You understand the basics

### Intermediate (Want to modify something)
1. Read: CODEBASE_EXPLORATION.md - Entire document
2. Study: ARCHITECTURE_DIAGRAMS.md - All diagrams
3. Reference: EXPLORATION_SUMMARY.txt - When needed
4. Ready to code!

### Advanced (Want to extend architecture)
1. Deep dive: All 3 documents
2. Read actual source code: Start with files listed in "Files Examined"
3. Study: Plugin implementations (calculator.rs, web_search.rs)
4. Review: AI router and client integration
5. Ready to design new features!

---

## 💡 Key Insights

1. **Plugin system is the core**: Everything is a plugin
2. **Frontend is reactive**: UI updates based on mode and content
3. **Backend is async**: All I/O operations are non-blocking
4. **AI integration is modular**: Plugins provide tools to Claude
5. **UX is keyboard-first**: Designed for power users
6. **Window is dynamic**: Resizes based on content and mode
7. **Settings are persisted**: Across app sessions
8. **Colors follow Catppuccin**: Professional design system

---

## 📞 Questions to Answer with These Docs

| Question | Document | Section |
|----------|----------|---------|
| How do plugins work? | CODEBASE_EXPLORATION.md | Plugin System |
| How does the frontend communicate with backend? | ARCHITECTURE_DIAGRAMS.md | Query Processing Flow |
| What happens when I type a search? | EXPLORATION_SUMMARY.txt | Query Flow (Step-by-step) |
| How does AI tool calling work? | ARCHITECTURE_DIAGRAMS.md | AI Chat Flow with Tool Calling |
| What are all the available plugins? | EXPLORATION_SUMMARY.txt | Complete Plugins List |
| How to add a new plugin? | EXPLORATION_SUMMARY.txt | Next Steps for Developers |
| What is the window sizing logic? | CODEBASE_EXPLORATION.md | Frontend Architecture |
| How are themes implemented? | EXPLORATION_SUMMARY.txt | Color Schemes |
| What Tauri commands are available? | EXPLORATION_SUMMARY.txt | Tauri Commands |
| How is state managed? | CODEBASE_EXPLORATION.md | Frontend Architecture / App State |

---

## ✅ Verification Checklist

If you've successfully understood OmniLauncher, you should be able to:

- [ ] Explain what a Plugin is and how it works
- [ ] Trace the data flow from user input to search results
- [ ] Identify which plugin handles a specific command (e.g., "=" for calculator)
- [ ] Understand why some searches need debouncing
- [ ] Explain how AI tool calling integrates with plugins
- [ ] Know the difference between launcher mode and AI mode
- [ ] Understand window geometry and why it changes size
- [ ] Identify all Tauri commands and their purposes
- [ ] Know how to add a new plugin
- [ ] Understand the theme system
- [ ] Identify keyboard shortcuts and their functions
- [ ] Explain the action_type field in QueryResult

---

## 📝 Notes

- All documentation is **readable without code editor** (pure markdown/text)
- ASCII diagrams are **compatible with any terminal/editor**
- Code examples are **extracted directly from source**
- File line counts are **accurate as of exploration date (May 26, 2026)**
- Plugin count is **34 total** (may change with future releases)

---

## 🔗 Document Links

- [Full Codebase Exploration](./CODEBASE_EXPLORATION.md)
- [Architecture Diagrams](./ARCHITECTURE_DIAGRAMS.md)
- [Exploration Summary](./EXPLORATION_SUMMARY.txt)

---

**Exploration Date**: May 26, 2026  
**Project Version**: 2.0.0  
**Explorer**: Claude Sonnet 4  
**Total Lines Examined**: 3,500+

