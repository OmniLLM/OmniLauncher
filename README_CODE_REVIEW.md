# OmniLauncher - Code Review Documentation

## 📋 Overview

This directory contains comprehensive documentation of the OmniLauncher codebase. The project is a **desktop launcher + AI assistant** built with Tauri (Rust backend) and React (TypeScript frontend).

**Key Numbers**:
- 📝 **8 frontend files** (TypeScript/React) → ~2,000 lines
- 🦀 **45 backend files** (Rust) → ~7,000 lines
- 🔌 **33 plugins** (modular system)
- 🤖 **Agentic AI loop** (10 max iterations with tool calling)
- 🛡️ **Safety guardrails** (shell/file operation checks)
- ⚙️ **Test suite** (25 existing tests)

---

## 📚 Documentation Files

### 1. **CODE_REVIEW_SUMMARY.md** (24 KB)
**Comprehensive code review and architecture overview**

Contains:
- ✅ Project architecture (Tauri + Tauri setup)
- ✅ Frontend structure (React components, state management)
- ✅ Backend modules (AI, plugins, database, skills)
- ✅ Key architectural patterns (dual-mode UI, agentic loop, plugin system)
- ✅ Data flows (launcher vs AI chat)
- ✅ Configuration & deployment
- ✅ Security & guardrails
- ✅ Code quality observations (strengths & areas for review)
- ✅ Testing coverage recommendations

**Best for**: Understanding overall architecture and code patterns

### 2. **SOURCE_FILES_MANIFEST.md** (12 KB)
**Complete file-by-file breakdown**

Contains:
- 📂 Frontend files (main.tsx, App.tsx, components)
- 📂 Backend files organized by module
- 📊 Summary statistics (file counts, lines of code)
- 📋 Data structures (TypeScript interfaces, Rust structs)
- 🧪 Testing coverage (25 existing tests + recommendations)
- ⚡ Performance considerations
- 🔗 Integration points
- 📦 Build & deployment

**Best for**: Quick reference, finding specific files, understanding data structures

---

## 🎯 Quick Navigation

### Frontend (`src/`)
| Component | Lines | Purpose |
|-----------|-------|---------|
| **App.tsx** | 1,084 | Core dual-mode launcher/AI UI |
| SearchBar.tsx | 298 | Input bar + hints |
| ResultList.tsx | 210 | Results display |
| SettingsPanel.tsx | 242 | Settings dialog |

### Backend (`src-tauri/src/`)
| Module | Files | Purpose |
|--------|-------|---------|
| **main.rs** | 1 | Tauri entry point, 12 commands |
| **ai/** | 4 | LLM client, router, errors, mod |
| **plugins/** | 34 | 33 plugin implementations |
| **skills/** | 1 | Skill manager (hot-reload) |
| **db/** | 1 | SQLite migrations |

---

## 🏗️ Architecture Highlights

### Dual-Mode Operation
```
User Input
    ↓
Prefix Detection (? or "ai ")
    ├→ No → Launcher Mode (instant, <100ms)
    │       └→ Plugin system (33 independent plugins)
    └→ Yes → AI Mode (1-5s, LLM + tool calling)
             └→ Agentic loop (up to 10 iterations)
```

### Plugin System (33 total)
- **File**: file_read, file_write, file_search, glob, grep
- **Web**: web_search, web_fetch
- **System**: bash_exec, shell, code_tools, process_manager
- **App**: app_launcher, url_opener, browser_bookmarks
- **Utility**: calculator, clipboard, color_picker, timer, todo
- **Info**: sys_info, network, env_vars, git

### AI Orchestration
1. Build system prompt (OS-aware)
2. Inject relevant skills (Hermes pattern)
3. Agentic loop: call AI → execute tools → continue
4. Loop detection (3-fingerprint history)
5. Context compression (>70% token budget threshold)
6. Error recovery (classify + retry)

### Safety Features
- ✅ Shell command guardrails (RCE, fork bomb, etc.)
- ✅ File write restrictions (/etc/, /sys/, /proc/)
- ✅ Retry logic with exponential backoff
- ✅ Tool output safety (no size limits)
- ⚠️ API key stored in plain JSON

---

## 🔍 Code Quality Summary

### Strengths ✅
- Well-organized modular architecture
- Comprehensive error handling with classification
- Loop detection for agentic iterations
- Context compression for long conversations
- Skill injection for extensible AI behavior
- Cross-platform support (Windows/Mac/Linux)
- Guardrails for dangerous operations
- Retry logic with exponential backoff
- Hot-reload capabilities

### Areas for Improvement ⚠️
- **Large files**: router.rs (500+ lines) could be refactored
- **Output limits**: No size limits on plugin results
- **Security**: API key stored as plain text
- **Persistence**: Conversation history lost on app restart
- **Test coverage**: Limited plugin system tests
- **Validation**: No input sanitization on slash commands
- **DOS potential**: Markdown renderer vulnerable to deep nesting

---

## 🧪 Testing

### Current Coverage (25 tests)
- guardrails.rs: 15 tests
- errors.rs: 6 tests
- skills/mod.rs: 3 tests
- main.rs: 1 test

### Recommended Additions
1. Plugin integration tests
2. Router decision logic tests
3. Agentic loop termination tests
4. Markdown edge case tests
5. Skill injection tests
6. Settings persistence tests

---

## 🚀 Getting Started with Code Review

### For Architecture Review
1. Start with **CODE_REVIEW_SUMMARY.md** "Key Architectural Patterns" section
2. Review data flow diagrams
3. Examine main.rs (733 lines)
4. Review router.rs (agentic loop implementation)

### For File-Level Review
1. Start with **SOURCE_FILES_MANIFEST.md** "Summary Statistics"
2. Pick a module (e.g., plugins/, ai/)
3. Review specific files
4. Check related tests

### For Security Review
1. See guardrails.rs (192 lines, 15 tests)
2. Review file_write.rs and bash_exec.rs
3. Check error handling in router.rs
4. Review API key storage (settings.rs)

### For Feature Understanding
1. Check App.tsx for UI logic (1,084 lines)
2. Review SearchBar.tsx and ResultList.tsx for interactions
3. Check SettingsPanel.tsx for configuration
4. Review router.rs for AI logic

---

## 📦 Key Files by Size

### Frontend (Top files)
1. App.tsx - 1,084 lines ⭐
2. SearchBar.tsx - 298 lines
3. SettingsPanel.tsx - 242 lines
4. ResultList.tsx - 210 lines

### Backend (Top modules)
1. router.rs - 500+ lines ⭐
2. main.rs - 733 lines ⭐
3. client.rs - 371 lines
4. skills/mod.rs - 364 lines
5. guardrails.rs - 192 lines

---

## 🔗 Integration Points

### Frontend → Backend
- `search(query)` → PluginManager
- `ai_query(query)` → Router::ai_route()
- `execute_result(result)` → Platform handler
- `get_settings()` → AppSettings
- Skill management commands

### Backend → LLM
- POST /v1/chat/completions (OpenAI-compatible)
- Tool calling with function schemas
- Streaming support (SSE)

### Backend → System
- Shell execution (bash/PowerShell/zsh)
- File I/O with guardrails
- Process management
- Network queries
- App launching

---

## 💡 Key Concepts

### Query Result
```typescript
{
  id: string,
  title: string,
  icon?: string,
  score: number,          // For sorting
  action_type: string,    // "open", "shell", "url", etc.
  action_data: string     // Command/URL/path
}
```

### Tool Schema
Plugins expose tool schemas for AI:
- Function name
- Description
- Input schema (JSON)
- Output format

### Skill Injection
Relevant skills injected before AI query:
```
--- Active Skills ---
<skill_body>
--- End Skills ---
<user_query>
```

### Error Classification
- **Transient**: Retry (timeout, rate limit, 429, 502, 503)
- **Permanent**: Don't retry (auth, bad request)
- **ModelError**: Corrective prompt + retry
- **ResourceError**: Compress context + retry

---

## 📊 Architecture Diagram

```
┌─────────────────────────────────────────────────────────┐
│                    React Frontend (TypeScript)          │
│  SearchBar → ResultList | ChatBubble (dual mode)        │
└────────────────────────┬────────────────────────────────┘
                         │ Tauri Commands
                         ↓
┌─────────────────────────────────────────────────────────┐
│               Tauri Backend (Rust)                      │
│                                                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │ Router::decide()                                │   │
│  │ ├→ Local (plugins) → instant                    │   │
│  │ └→ Ai (LLM) → agentic loop                      │   │
│  └─────────────────────────────────────────────────┘   │
│           ↓                                             │
│  ┌──────────────────────────────────┐  ┌────────────┐ │
│  │ PluginManager (33 plugins)       │  │ AiClient   │ │
│  │ ├ File ops                       │  │ ├ Retry    │ │
│  │ ├ Shell/Code                     │  │ ├ Tools    │ │
│  │ ├ Web (search, fetch)            │  │ └ Streams  │ │
│  │ ├ System (process, net, etc.)    │  └────────────┘ │
│  │ └ Utilities                      │                  │
│  └──────────────────────────────────┘                  │
│           ↓                                             │
│  ┌──────────────────────────────────┐                  │
│  │ SkillManager (hot-reload)        │                  │
│  │ ├ Bundled skills (assets/)       │                  │
│  │ └ User skills (~/.config/)       │                  │
│  └──────────────────────────────────┘                  │
└─────────────────────────────────────────────────────────┘
         ↓                                     ↓
    LLM API (OpenAI-compatible)          Local System
    (if AI mode enabled)                  (file/shell/process)
```

---

## 🎓 Understanding the Codebase

### Entry Points
- **Frontend**: `src/main.tsx` (10 lines) → `App.tsx` (1,084 lines)
- **Backend**: `src-tauri/src/main.rs` (733 lines) → `fn run()`

### Key Decision Points
- **Router**: `Router::decide()` (Local vs AI)
- **Prefix detection**: `isAiPrefix()` (in App.tsx)
- **Plugin matching**: `PluginManager::query_all()` (keyword-based)
- **Error classification**: `classify_error()` (retry strategy)

### Extension Points
- **Add plugin**: Implement `Plugin` trait in plugins/your_plugin.rs
- **Add skill**: Create SKILL.md in ~/.config/omnilauncher/skills/
- **Add Tauri command**: Add to invoke_handler! in main.rs

---

## 📞 For More Information

See the detailed documents:
- **CODE_REVIEW_SUMMARY.md** - Comprehensive architecture & code patterns
- **SOURCE_FILES_MANIFEST.md** - File-by-file breakdown & statistics

---

**Generated**: 2026-05-25  
**Codebase Size**: ~9,000 lines (frontend + backend)  
**Test Coverage**: 25 tests, 15 security checks  
**Architecture**: Tauri (Rust) + React (TypeScript)
