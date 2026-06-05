//! Single source of truth for launcher input rules that the frontend needs to
//! evaluate synchronously on every keystroke (which UI chrome to show: AI mode,
//! launcher results, or slash-command autocomplete).
//!
//! Historically these rules were hand-copied into the React layer
//! (`src/utils/aiPrefix.ts`, `SLASH_COMMANDS` in `App.tsx`) and drifted from the
//! authoritative backend. The frontend now fetches this config once at startup
//! via the `get_launcher_config` command and evaluates the cached data locally,
//! so there is no async round-trip per keystroke and no duplicated definition.

use serde::Serialize;

/// Prefixes that route a query to the AI instead of local plugins.
/// This is the authority consumed by `Router::decide` / `Router::strip_ai_prefix`.
pub const AI_PREFIXES: &[&str] = &["?", "ai "];

/// Aliases (without the leading slash) that open the Plugin Manager panel.
pub const PLUGIN_MANAGER_ALIASES: &[&str] = &["plugins", "pm"];

/// Slash inputs that reset / clear the current AI conversation.
pub const RESET_COMMANDS: &[&str] = &["/new", "/clear"];

/// Inputs that show the help command list.
pub const HELP_COMMANDS: &[&str] = &["/help", "/?", "help"];

/// A single slash command shown in the launcher's autocomplete palette.
#[derive(Debug, Clone, Serialize)]
pub struct SlashCommandDef {
    pub name: &'static str,
    pub shortcut: Option<&'static str>,
    pub description: &'static str,
    pub usage: &'static str,
}

/// Canonical slash-command catalog. Derived from the real match arms in
/// `ai::router::Router::slash_command` (operational commands) plus the
/// launcher-level navigation commands handled in the frontend (`/plugins`,
/// `/skills`, `/new`, `/clear`). This is the data the frontend renders as
/// autocomplete suggestions and the help list.
pub const SLASH_COMMANDS: &[SlashCommandDef] = &[
    // ── Launcher navigation (handled in the UI) ──────────────────────────────
    SlashCommandDef {
        name: "/plugins",
        shortcut: Some("/pm"),
        description: "Open external plugin manager",
        usage: "/plugins",
    },
    SlashCommandDef {
        name: "/skills",
        shortcut: None,
        description: "Open skill manager (install, view, delete skills)",
        usage: "/skills",
    },
    SlashCommandDef {
        name: "/new",
        shortcut: None,
        description: "Start a new AI conversation",
        usage: "/new",
    },
    SlashCommandDef {
        name: "/clear",
        shortcut: None,
        description: "Clear the current AI conversation",
        usage: "/clear",
    },
    SlashCommandDef {
        name: "/help",
        shortcut: Some("/?"),
        description: "Show all available commands",
        usage: "/help",
    },
    // ── Operational commands (handled by Router::slash_command) ───────────────
    SlashCommandDef {
        name: "/run",
        shortcut: Some("/r"),
        description: "Execute a shell command",
        usage: "/run <command>",
    },
    SlashCommandDef {
        name: "/open",
        shortcut: Some("/o"),
        description: "Open app, file, or URL",
        usage: "/open <target>",
    },
    SlashCommandDef {
        name: "/app",
        shortcut: Some("/a"),
        description: "Search & launch applications",
        usage: "/app <query>",
    },
    SlashCommandDef {
        name: "/find",
        shortcut: Some("/f"),
        description: "Search files by name",
        usage: "/find <name>",
    },
    SlashCommandDef {
        name: "/grep",
        shortcut: Some("/g"),
        description: "Search file contents with regex",
        usage: "/grep <pattern> [path]",
    },
    SlashCommandDef {
        name: "/cat",
        shortcut: None,
        description: "Read and display a file",
        usage: "/cat <file>",
    },
    SlashCommandDef {
        name: "/ls",
        shortcut: None,
        description: "List directory contents",
        usage: "/ls [path]",
    },
    SlashCommandDef {
        name: "/git",
        shortcut: None,
        description: "Run git command (default: status)",
        usage: "/git [subcmd]",
    },
    SlashCommandDef {
        name: "/calc",
        shortcut: Some("/c"),
        description: "Quick calculator",
        usage: "/calc <expr>",
    },
    SlashCommandDef {
        name: "/todo",
        shortcut: Some("/t"),
        description: "List todos or add one",
        usage: "/todo [text]",
    },
    SlashCommandDef {
        name: "/web",
        shortcut: Some("/w"),
        description: "Web search (Google)",
        usage: "/web <query>",
    },
    SlashCommandDef {
        name: "/ip",
        shortcut: None,
        description: "Show public IP address",
        usage: "/ip",
    },
    SlashCommandDef {
        name: "/ports",
        shortcut: None,
        description: "Show listening network ports",
        usage: "/ports",
    },
    SlashCommandDef {
        name: "/ps",
        shortcut: None,
        description: "Top processes by CPU usage",
        usage: "/ps",
    },
    SlashCommandDef {
        name: "/kill",
        shortcut: None,
        description: "Kill a process",
        usage: "/kill <name/pid>",
    },
    SlashCommandDef {
        name: "/env",
        shortcut: None,
        description: "Get environment variable value",
        usage: "/env <var>",
    },
    SlashCommandDef {
        name: "/color",
        shortcut: None,
        description: "Convert color formats (hex/rgb/hsl)",
        usage: "/color <value>",
    },
    SlashCommandDef {
        name: "/sys",
        shortcut: None,
        description: "System: lock/sleep/shutdown/restart",
        usage: "/sys <cmd>",
    },
    SlashCommandDef {
        name: "/clip",
        shortcut: Some("/cb"),
        description: "Search clipboard history",
        usage: "/clip [term]",
    },
    SlashCommandDef {
        name: "/skill",
        shortcut: None,
        description: "Manage skills (list/view/install/delete)",
        usage: "/skill <subcmd>",
    },
];

/// The full launcher rule-set, serialized to the frontend in one call.
#[derive(Debug, Clone, Serialize)]
pub struct LauncherConfig {
    pub ai_prefixes: Vec<String>,
    pub plugin_manager_aliases: Vec<String>,
    pub reset_commands: Vec<String>,
    pub help_commands: Vec<String>,
    pub slash_commands: Vec<SlashCommandDef>,
}

impl LauncherConfig {
    pub fn current() -> Self {
        Self {
            ai_prefixes: AI_PREFIXES.iter().map(|s| s.to_string()).collect(),
            plugin_manager_aliases: PLUGIN_MANAGER_ALIASES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            reset_commands: RESET_COMMANDS.iter().map(|s| s.to_string()).collect(),
            help_commands: HELP_COMMANDS.iter().map(|s| s.to_string()).collect(),
            slash_commands: SLASH_COMMANDS.to_vec(),
        }
    }
}

/// Returns true when `input` begins with an AI-routing prefix. Shared by
/// `Router::decide` so the command, the router, and the frontend agree.
pub fn has_ai_prefix(input: &str) -> bool {
    let trimmed = input.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    AI_PREFIXES.iter().any(|p| {
        if *p == "?" {
            trimmed.starts_with('?')
        } else {
            lower.starts_with(*p)
        }
    })
}
