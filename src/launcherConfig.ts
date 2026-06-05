/**
 * Launcher input rules — fetched once from the backend and evaluated
 * synchronously on every keystroke.
 *
 * The backend (`omnilauncher_lib::launcher_config`) is the single source of
 * truth for AI prefixes and the slash-command catalog. We cache the config in a
 * module-level variable so the per-keystroke predicates below stay synchronous
 * (no async round-trip while typing). Until `loadLauncherConfig()` resolves, the
 * predicates use the same defaults the backend ships, so first paint is correct.
 */
import { invoke } from "./lib/runtime";

/** Result shape consumed by ResultList — mirrors the backend QueryResult. */
export interface LauncherResult {
  id: string;
  title: string;
  subtitle?: string;
  icon?: string;
  score: number;
  action_type: string;
  action_data: string;
  source?: string;
}

export interface SlashCommand {
  name: string;
  shortcut?: string | null;
  description: string;
  usage: string;
}

interface LauncherConfig {
  ai_prefixes: string[];
  plugin_manager_aliases: string[];
  reset_commands: string[];
  help_commands: string[];
  slash_commands: SlashCommand[];
}

// Defaults mirror the backend constants so predicates are correct before the
// async config load completes (and as a fallback if it ever fails).
const DEFAULT_CONFIG: LauncherConfig = {
  ai_prefixes: ["?", "ai "],
  plugin_manager_aliases: ["plugins", "pm"],
  reset_commands: ["/new", "/clear"],
  help_commands: ["/help", "/?", "help"],
  slash_commands: [
    {
      name: "/plugins",
      shortcut: "/pm",
      description: "Open external plugin manager",
      usage: "/plugins",
    },
    {
      name: "/skills",
      shortcut: null,
      description: "Open skill manager (install, view, delete skills)",
      usage: "/skills",
    },
    {
      name: "/new",
      shortcut: null,
      description: "Start a new AI conversation",
      usage: "/new",
    },
    {
      name: "/clear",
      shortcut: null,
      description: "Clear the current AI conversation",
      usage: "/clear",
    },
    {
      name: "/help",
      shortcut: "/?",
      description: "Show all available commands",
      usage: "/help",
    },
  ],
};

let config: LauncherConfig = DEFAULT_CONFIG;

/** Fetch the launcher config from the backend once and cache it. Call on mount. */
export async function loadLauncherConfig(): Promise<void> {
  try {
    const cfg = await invoke<LauncherConfig>("get_launcher_config");
    if (cfg && Array.isArray(cfg.slash_commands)) {
      config = cfg;
    }
  } catch {
    // Keep DEFAULT_CONFIG — the hardcoded defaults match the backend ship values.
  }
}

// ─── Synchronous predicates ─────────────────────────────────────────────────

export function isAiPrefix(input: string): boolean {
  const trimmed = input.trimStart();
  const lower = trimmed.toLowerCase();
  return config.ai_prefixes.some((p) =>
    p === "?" ? trimmed.startsWith("?") : lower.startsWith(p),
  );
}

/** In-progress slash prefix: starts with "/" and no space yet. */
export function isSlashPrefix(input: string): boolean {
  return input.startsWith("/") && !input.includes(" ");
}

export function isPluginManagerQuery(input: string): boolean {
  const t = input.trim().toLowerCase();
  return config.plugin_manager_aliases.some(
    (alias) => t === alias || t.startsWith(`${alias} `),
  );
}

export function isConversationResetCommand(input: string): boolean {
  const t = input.trim().toLowerCase();
  return config.reset_commands.includes(t);
}

export function isHelpQuery(input: string): boolean {
  const t = input.trim().toLowerCase();
  return config.help_commands.includes(t);
}

export function isHelpHintQuery(input: string): boolean {
  return input.trim().toLowerCase() === "help";
}

// ─── Catalog-derived result lists ────────────────────────────────────────────

function toResult(
  sc: SlashCommand,
  idPrefix: string,
  actionType: string,
): LauncherResult {
  return {
    id: `${idPrefix}-${sc.name}`,
    title: sc.shortcut ? `${sc.name}  ${sc.shortcut}` : sc.name,
    subtitle: `${sc.description} · ${sc.usage}`,
    icon: "⌘",
    score: 1,
    action_type: actionType,
    action_data: `${sc.name} `,
  };
}

/** Slash commands whose name/shortcut match the in-progress query prefix. */
export function slashSuggestions(query: string): LauncherResult[] {
  const lower = query.toLowerCase();
  return config.slash_commands
    .filter(
      (sc) =>
        sc.name.toLowerCase().startsWith(lower) ||
        (sc.shortcut && sc.shortcut.toLowerCase().startsWith(lower)),
    )
    .map((sc) => ({
      ...toResult(sc, "slash", "slash_complete"),
      action_data: `${sc.name} `,
    }));
}

/** Full catalog rendered as the /help result list. */
export function helpResults(): LauncherResult[] {
  return config.slash_commands.map((sc) =>
    toResult(sc, "help", "help_command"),
  );
}
