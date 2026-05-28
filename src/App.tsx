import { useState, useEffect, useCallback, useRef } from "react";
import { renderMarkdown } from "./utils/markdown";
import { isAiPrefix } from "./utils/aiPrefix";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import SearchBar from "./components/SearchBar";
import ResultList from "./components/ResultList";
import FavoritesList from "./components/FavoritesList";
import SettingsWindow from "./components/SettingsWindow";
import PluginManager from "./components/PluginManager";
import SkillManager from "./components/SkillManager";

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

interface AiSessionInfo {
  id: number;
  title: string;
  created_at: string;
  last_active_at: string;
  message_count: number;
}

interface AppSettings {
  ai_base_url: string;
  ai_model: string;
  ai_api_key: string;
  theme: string;
  hotkey: string;
  max_results: number;
  background_url: string;
}

type ThemeMode = "dark" | "light" | "system";
type ResolvedTheme = "dark" | "light";

function getSystemTheme(): ResolvedTheme {
  if (
    typeof window !== "undefined" &&
    window.matchMedia &&
    window.matchMedia("(prefers-color-scheme: dark)").matches
  ) {
    return "dark";
  }
  return "light";
}

function parseThemeMode(theme: string): ThemeMode {
  if (theme === "dark" || theme === "light" || theme === "system") {
    return theme;
  }
  return "system";
}

/**
 * Detect if the user typed an explicit AI prefix.
 * Mirrors Router::decide() in router.rs.
 *
 * Prefixes: `?`  or  `ai ` (case-insensitive)
 *
 * Everything else → local plugins (instant, no AI latency).
 */
/** Returns true when the query should trigger the Plugin Manager shortcut. */
function isPluginManagerQuery(input: string): boolean {
  const t = input.trim().toLowerCase();
  return t === "plugins" || t === "pm" || t.startsWith("pm ");
}

/** Synthetic result that opens the Plugin Manager panel. */
function pluginManagerResult(): QueryResult {
  return {
    id: "builtin:plugin-manager",
    title: "Manage Plugins",
    subtitle: "Install, list, and remove external plugins",
    icon: "🔌",
    score: 100,
    action_type: "open_plugin_manager",
    action_data: "",
  };
}

/**
 * Returns true when the input looks like an in-progress slash command prefix
 * (starts with "/" but has no space yet — the user is still typing the name).
 */
function isSlashPrefix(input: string): boolean {
  return input.startsWith("/") && !input.includes(" ");
}

/** Convert matching SlashCommands to QueryResults for display in ResultList. */
function slashSuggestions(query: string): QueryResult[] {
  const lower = query.toLowerCase();
  return SLASH_COMMANDS.filter(
    (sc) =>
      sc.cmd.toLowerCase().startsWith(lower) ||
      (sc.shortcut && sc.shortcut.toLowerCase().startsWith(lower)),
  ).map((sc) => ({
    id: `slash-${sc.cmd}`,
    title: sc.shortcut ? `${sc.cmd}  ${sc.shortcut}` : sc.cmd,
    subtitle: `${sc.description} · ${sc.usage}`,
    icon: "⌘",
    score: 1,
    action_type: "slash_complete",
    action_data: sc.cmd + " ",
  }));
}

function isConversationResetCommand(input: string): boolean {
  const t = input.trim().toLowerCase();
  return t === "/new" || t === "/clear";
}

function isHelpQuery(input: string): boolean {
  const t = input.trim().toLowerCase();
  return t === "/help" || t === "/?";
}

function isHelpHintQuery(input: string): boolean {
  return input.trim().toLowerCase() === "help";
}

const DARK_COLORS = {
  bg: "#202124",
  surface: "#303134",
  surface2: "#3c3f43",
  text: "#e8eaed",
  accent: "#8ab4f8",
  accentDim: "#aecbfa",
  sub: "#9aa0a6",
  userBubble: "#1a73e8",
  userBubbleText: "#ffffff",
  aiBubble: "#303134",
  aiText: "#e8eaed",
};

const LIGHT_COLORS = {
  bg: "#EFF1F5",
  surface: "#CCD0DA",
  surface2: "#BCC0CC",
  text: "#4C4F69",
  accent: "#8839EF",
  accentDim: "#6E28CF",
  sub: "#9CA0B0",
  userBubble: "#8839EF",
  userBubbleText: "#EFF1F5",
  aiBubble: "#CCD0DA",
  aiText: "#4C4F69",
};

// ─── Slash commands ───────────────────────────────────────────────────────────

interface SlashCommand {
  cmd: string;
  shortcut?: string;
  description: string;
  usage: string;
  examples: string[];
}

const SLASH_COMMANDS: SlashCommand[] = [
  {
    cmd: "/plugins",
    shortcut: "/pm",
    description: "Open external plugin manager",
    usage: "/plugins",
    examples: ["/plugins", "/pm"],
  },
  {
    cmd: "/skills",
    description: "Open skill manager (install, view, delete skills)",
    usage: "/skills",
    examples: ["/skills"],
  },
  {
    cmd: "/new",
    description: "Start a new AI conversation",
    usage: "/new",
    examples: ["/new"],
  },
  {
    cmd: "/clear",
    description: "Clear the current AI conversation",
    usage: "/clear",
    examples: ["/clear"],
  },
  {
    cmd: "/help",
    shortcut: "/?",
    description: "Show all available commands",
    usage: "/help",
    examples: ["/help"],
  },
];

const HELP_RESULTS: QueryResult[] = SLASH_COMMANDS.map((command) => ({
  id: `help-${command.cmd}`,
  title: command.shortcut ? `${command.cmd}  ${command.shortcut}` : command.cmd,
  subtitle: `${command.description} · ${command.usage}`,
  icon: "⌘",
  score: 1,
  action_type: "help_command",
  action_data: `${command.cmd} `,
}));

// ─── App ──────────────────────────────────────────────────────────────────────

export default function App() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<QueryResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [aiModeEnabled, setAiModeEnabled] = useState(false);
  const [showPluginManager, setShowPluginManager] = useState(false);
  const [showSkillManager, setShowSkillManager] = useState(false);
  const [isHintBarExpanded, setIsHintBarExpanded] = useState(false);
  const [theme, setTheme] = useState<ThemeMode>("system");
  const [systemTheme, setSystemTheme] = useState<ResolvedTheme>(
    getSystemTheme(),
  );
  const [backgroundUrl, setBackgroundUrl] = useState<string>(
    "https://blz-contentstack-images.akamaized.net/v3/assets/bltf408a0557f4e4998/blt27903959c912debc/69fba009d002ee6d7deb5875/shop_carousel_ow_26_s2_mythicskin_desktop.webp?imwidth=1568&imdensity=1",
  );
  const [conversationHistory, setConversationHistory] = useState<
    ConversationTurn[]
  >([]);
  const [sessions, setSessions] = useState<AiSessionInfo[]>([]);
  const [currentSessionId, setCurrentSessionId] = useState<number | null>(null);
  const [showSessionPicker, setShowSessionPicker] = useState(false);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const chatScrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement | HTMLTextAreaElement>(null);
  const [inputHistory, setInputHistory] = useState<string[]>([]);
  const [historyIdx, setHistoryIdx] = useState(-1);
  const inputHistoryRef = useRef<string[]>([]);
  const pendingQueueRef = useRef<string[]>([]);

  const [favorites, setFavorites] = useState<string[]>(() => {
    try {
      const stored = localStorage.getItem("omni-favorites");
      return stored ? JSON.parse(stored) : [];
    } catch { return []; }
  });
  const [showCheatSheet, setShowCheatSheet] = useState(false);
  const [exportToast, setExportToast] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

  const resolvedTheme: ResolvedTheme =
    theme === "system" ? systemTheme : theme;
  const colors = resolvedTheme === "dark" ? DARK_COLORS : LIGHT_COLORS;

  const handleThemeToggle = useCallback(async () => {
    const next: ThemeMode = resolvedTheme === "dark" ? "light" : "dark";
    setTheme(next);
    // Persist to backend settings so it survives restarts
    try {
      const current = await invoke<AppSettings>("get_settings");
      await invoke("save_settings_cmd", { settings: { ...current, theme: next } });
    } catch {
      // non-fatal — the in-memory theme change already happened
    }
  }, [resolvedTheme]);

  const focusInput = useCallback((select = false) => {
    inputRef.current?.focus();
    if (select) inputRef.current?.select();
  }, []);

  const refreshSessions = useCallback(async () => {
    try {
      const [list, cur] = await Promise.all([
        invoke<AiSessionInfo[]>("list_ai_sessions"),
        invoke<number>("current_ai_session"),
      ]);
      setSessions(list || []);
      setCurrentSessionId(cur || null);
    } catch (e) {
      console.error("refreshSessions error:", e);
    }
  }, []);

  const isAiMode = aiModeEnabled || isAiPrefix(query);

  // Load settings on mount
  useEffect(() => {
    invoke<AppSettings>("get_settings")
      .then((s) => {
        setSettings(s);
        setTheme(parseThemeMode(s.theme));
        if (s.background_url) setBackgroundUrl(s.background_url);
      })
      .catch(() => {});
  }, []);

  // Load AI sessions on mount and rehydrate the active session's transcript.
  useEffect(() => {
    (async () => {
      try {
        const cur = await invoke<number>("current_ai_session");
        setCurrentSessionId(cur || null);
        if (cur) {
          const msgs = await invoke<Array<{ role: string; content: string }>>(
            "switch_ai_session",
            { sessionId: cur },
          );
          const turns: ConversationTurn[] = (msgs || [])
            .filter((m) => m.role === "user" || m.role === "assistant")
            .map((m) => ({
              role: m.role as "user" | "assistant",
              content: m.content,
            }));
          setConversationHistory(turns);
        }
        const list = await invoke<AiSessionInfo[]>("list_ai_sessions");
        setSessions(list || []);
      } catch (e) {
        console.error("session bootstrap error:", e);
      }
    })();
  }, []);

  // Listen for settings changes from the standalone settings window
  useEffect(() => {
    const unlisten = listen<AppSettings>("omnilauncher://settings-saved", (e) => {
      setTheme(parseThemeMode(e.payload.theme));
      setBackgroundUrl(e.payload.background_url ?? "");
      setSettings(e.payload);
      setShowSettings(false);
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);

  useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = (event: MediaQueryListEvent) => {
      setSystemTheme(event.matches ? "dark" : "light");
    };

    setSystemTheme(media.matches ? "dark" : "light");
    media.addEventListener("change", handleChange);
    return () => media.removeEventListener("change", handleChange);
  }, []);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", resolvedTheme);
  }, [resolvedTheme]);

  const doSearch = useCallback(async (q: string) => {
    if (isHelpQuery(q)) {
      setResults(HELP_RESULTS);
      return;
    }

    if (isConversationResetCommand(q)) {
      setResults([]);
      return;
    }

    if (!q.trim() || isAiPrefix(q) || isHelpHintQuery(q)) {
      setResults([]);
      return;
    }

    // Plugin Manager shortcut
    if (isPluginManagerQuery(q)) {
      setResults([pluginManagerResult()]);
      return;
    }

    // Slash prefix without a space → show autocomplete suggestions, no backend call
    if (isSlashPrefix(q)) {
      setResults(slashSuggestions(q));
      return;
    }

    try {
      const res = await invoke<QueryResult[]>("search", { query: q });
      setResults(res);
    } catch {
      setResults([]);
    }
  }, []);

  // Focus input on mount
  useEffect(() => {
    focusInput();
  }, [focusInput]);

  // Re-focus input when the native window is shown or focused
  useEffect(() => {
    const focusVisibleInput = () => {
      focusInput();
      setTimeout(() => focusInput(true), 50);
      setTimeout(() => focusInput(), 150);
    };

    let unlistenFocus: (() => void) | undefined;
    let unlistenShown: (() => void) | undefined;

    listen<string>("omnilauncher://shown", (event) => {
      const selection = event.payload ?? "";
      if (selection.trim()) {
        // Auto-populate with selected text from the previously focused app.
        // The selection plugin will detect the "__sel__:" prefix and show actions.
        setQuery("__sel__:" + selection.trim());
        setTimeout(() => focusInput(true), 50);
      } else {
        focusVisibleInput();
      }
    })
      .then((fn) => {
        unlistenShown = fn;
      })
      .catch(() => {});

    getCurrentWebviewWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused) focusVisibleInput();
      })
      .then((fn) => {
        unlistenFocus = fn;
      })
      .catch(() => {});

    return () => {
      unlistenFocus?.();
      unlistenShown?.();
    };
  }, [focusInput]);

  // Browser-level fallback for focus restore (useful in dev/web context)
  useEffect(() => {
    const shouldFocusLauncherInput = () => !showPluginManager && !showSkillManager;

    const restoreFocus = () => {
      if (!shouldFocusLauncherInput()) return;
      setTimeout(() => focusInput(), 0);
    };

    const onVisibilityChange = () => {
      if (document.visibilityState === "visible") {
        restoreFocus();
      }
    };

    window.addEventListener("focus", restoreFocus);
    document.addEventListener("visibilitychange", onVisibilityChange);

    return () => {
      window.removeEventListener("focus", restoreFocus);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, [focusInput, showPluginManager]);

  const doAiQuery = useCallback(
    async (q: string) => {
      if (!q.trim() || loading) return;

      const userTurn: ConversationTurn = { role: "user", content: q };
      const pendingAiTurn: ConversationTurn = {
        role: "assistant",
        content: "",
        tools_used: [],
        isStreaming: true,
      };
      setConversationHistory((prev) => [...prev, userTurn, pendingAiTurn]);
      setLoading(true);
      setResults([]);

      // Register listeners FIRST and await registration so we never miss
      // the ai-done / ai-error event that the backend may emit promptly.
      const unlisteners: (() => void)[] = [];

      const cleanup = () => {
        unlisteners.forEach((fn) => fn());
        unlisteners.length = 0;
      };

      const finish = (content: string, tools_used?: string[]) => {
        cleanup();
        setConversationHistory((prev) => {
          const next = [...prev];
          const last = next[next.length - 1];
          if (last?.role === "assistant" && last.isStreaming) {
            next[next.length - 1] = {
              role: "assistant",
              content,
              tools_used,
              isStreaming: false,
            };
          }
          return next;
        });
        setLoading(false);
        // Drain next queued prompt after loading state settles
        setTimeout(() => {
          const next = pendingQueueRef.current.shift();
          if (next) doAiQuery(next);
        }, 50);
        setTimeout(() => focusInput(), 50);
      };

      try {
        const [unToolCall, unDone, unError] = await Promise.all([
          listen<{ tool: string; iteration: number }>(
            "omnilauncher://ai-tool-call",
            (event) => {
              const toolName = event.payload.tool;
              setConversationHistory((prev) => {
                const next = [...prev];
                const last = next[next.length - 1];
                if (last?.role === "assistant" && last.isStreaming) {
                  next[next.length - 1] = {
                    ...last,
                    content: `🔧 Calling **${toolName}**…`,
                    tools_used: [...(last.tools_used ?? []), toolName],
                  };
                }
                return next;
              });
            },
          ),
          listen<AiResponse>("omnilauncher://ai-done", (event) => {
            finish(event.payload.content, event.payload.tools_used);
            refreshSessions();
          }),
          listen<string>("omnilauncher://ai-error", (event) => {
            finish(`Error: ${event.payload}`);
          }),
        ]);
        unlisteners.push(unToolCall, unDone, unError);
      } catch (e) {
        finish(`Error: ${e}`);
        return;
      }

      try {
        await invoke("ai_query", { query: q });
      } catch (e) {
        finish(`Error: ${e}`);
      }
    },
    [focusInput, loading, refreshSessions],
  );

  const handleQueryChange = useCallback(
    (value: string) => {
      setHistoryIdx(-1);
      if (isHelpQuery(value)) {
        if (debounceRef.current) clearTimeout(debounceRef.current);
        setQuery(value);
        setResults(HELP_RESULTS);
        return;
      }

      if (isHelpHintQuery(value)) {
        if (debounceRef.current) clearTimeout(debounceRef.current);
        setQuery(value);
        setResults([]);
        return;
      }

      if (isConversationResetCommand(value)) {
        if (debounceRef.current) clearTimeout(debounceRef.current);
        setQuery(value);
        setResults([]);
        return;
      }

      if (value.trim() === "?") {
        setAiModeEnabled((prev) => !prev);
        setQuery("");
        setResults([]);
        return;
      }

      setQuery(value);
      if (debounceRef.current) clearTimeout(debounceRef.current);
      if (isSlashPrefix(value)) {
        // Show slash suggestions instantly in both launcher and AI mode
        setResults(slashSuggestions(value));
      } else if (!aiModeEnabled) {
        // Don't clear results immediately — let the debounced search replace
        // them. Clearing first causes the window to shrink then re-expand on
        // every keystroke (flash/flicker UX issue).
        debounceRef.current = setTimeout(() => {
          doSearch(value);
        }, 100);
      } else {
        // In AI mode, clear slash suggestions when user types past the prefix
        setResults([]);
      }
    },
    [aiModeEnabled, doSearch],
  );

  const handleNewConversation = useCallback(async () => {
    try {
      await invoke("clear_conversation");
    } catch (e) {
      console.error("clear_conversation error:", e);
    }
    setConversationHistory([]);
    setResults([]);
    setQuery("");
    setShowSessionPicker(false);
    refreshSessions();
  }, [refreshSessions]);

  const handleSwitchSession = useCallback(
    async (sessionId: number) => {
      try {
        const msgs = await invoke<Array<{ role: string; content: string }>>(
          "switch_ai_session",
          { sessionId },
        );
        const turns: ConversationTurn[] = (msgs || [])
          .filter((m) => m.role === "user" || m.role === "assistant")
          .map((m) => ({ role: m.role as "user" | "assistant", content: m.content }));
        setConversationHistory(turns);
        setCurrentSessionId(sessionId);
      } catch (e) {
        console.error("switch_ai_session error:", e);
      }
      setShowSessionPicker(false);
      setResults([]);
      setQuery("");
      refreshSessions();
    },
    [refreshSessions],
  );

  const handleDeleteSession = useCallback(
    async (sessionId: number) => {
      try {
        const newCur = await invoke<number>("delete_ai_session", { sessionId });
        if (currentSessionId === sessionId) {
          setConversationHistory([]);
          setCurrentSessionId(newCur || null);
        }
      } catch (e) {
        console.error("delete_ai_session error:", e);
      }
      refreshSessions();
    },
    [currentSessionId, refreshSessions],
  );

  const handleSubmit = useCallback(
    async (value: string, forceAi: boolean) => {
      if (isConversationResetCommand(value)) {
        handleNewConversation();
        return;
      }

      if (isHelpQuery(value)) {
        setResults(HELP_RESULTS);
        return;
      }

      if (value.trim() === "?") {
        setAiModeEnabled((prev) => !prev);
        setQuery("");
        setResults([]);
        return;
      }

      if (isSlashPrefix(value)) {
        setResults(slashSuggestions(value));
        return;
      }

      const slashCommand = value.trim().toLowerCase();

      if (slashCommand === "/plugins" || slashCommand === "/pm") {
        setShowPluginManager(true);
        setResults([]);
        setQuery("");
        return;
      }

      if (slashCommand === "/skills") {
        setShowSkillManager(true);
        setResults([]);
        setQuery("");
        return;
      }

      if (forceAi || isAiPrefix(value)) {
        if (debounceRef.current) clearTimeout(debounceRef.current);
        setAiModeEnabled(true);
        if (value.trim() && inputHistoryRef.current[0] !== value.trim()) {
          inputHistoryRef.current = [value.trim(), ...inputHistoryRef.current].slice(0, 50);
          setInputHistory([...inputHistoryRef.current]);
        }
        setHistoryIdx(-1);
        if (loading) {
          pendingQueueRef.current.push(value);
        } else {
          doAiQuery(value);
        }
        setQuery("");
      } else if (aiModeEnabled && value.trim()) {
        if (debounceRef.current) clearTimeout(debounceRef.current);
        if (value.trim() && inputHistoryRef.current[0] !== value.trim()) {
          inputHistoryRef.current = [value.trim(), ...inputHistoryRef.current].slice(0, 50);
          setInputHistory([...inputHistoryRef.current]);
        }
        setHistoryIdx(-1);
        if (loading) {
          pendingQueueRef.current.push(value);
        } else {
          doAiQuery(value);
        }
        setQuery("");
      }
    },
    [aiModeEnabled, doAiQuery, handleNewConversation],
  );

  const handleExecute = useCallback(
    async (result: QueryResult) => {
      if (result.action_type === "open_plugin_manager") {
        setShowPluginManager(true);
        setResults([]);
        setQuery("");
        return;
      }

      if (result.action_type === "help_command") {
        setQuery(result.action_data);
        setResults([]);
        setTimeout(() => focusInput(), 50);
        return;
      }

      if (result.action_type === "slash_complete") {
        setQuery(result.action_data);
        setResults([]);
        setTimeout(() => focusInput(), 50);
        return;
      }

      if (result.action_type === "copy") {
        try {
          await navigator.clipboard.writeText(result.action_data);
        } catch {
          console.log("Copy:", result.action_data);
        }
        return;
      }

      if (result.action_type === "vision_analyze") {
        const prompt = result.action_data;
        const userLabel = prompt.trim() || "Describe what you see";
        const userTurn: ConversationTurn = {
          role: "user",
          content: `👁 Vision: ${userLabel}`,
        };
        const pendingAiTurn: ConversationTurn = {
          role: "assistant",
          content: "",
          tools_used: [],
          isStreaming: true,
        };
        setAiModeEnabled(true);
        setConversationHistory((prev) => [...prev, userTurn, pendingAiTurn]);
        setLoading(true);
        setResults([]);
        setQuery("");
        try {
          const response = await invoke<string>("vision_analyze", { prompt });
          setConversationHistory((prev) => {
            const next = [...prev];
            next[next.length - 1] = {
              role: "assistant",
              content: response,
              tools_used: ["vision"],
              isStreaming: false,
            };
            return next;
          });
        } catch (e) {
          setConversationHistory((prev) => {
            const next = [...prev];
            next[next.length - 1] = {
              role: "assistant",
              content: `Vision analysis failed: ${e}`,
              isStreaming: false,
            };
            return next;
          });
        } finally {
          setLoading(false);
          setTimeout(() => focusInput(), 150);
        }
        return;
      }

      try {
        await invoke("execute_result", { result });
      } catch (e) {
        console.error("Execute error:", e);
      }
    },
    [focusInput],
  );
  useEffect(() => {
    if (isAiMode && chatScrollRef.current) {
      chatScrollRef.current.scrollTop = chatScrollRef.current.scrollHeight;
    }
  }, [conversationHistory, isAiMode]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const active = document.activeElement as HTMLElement | null;
      const isEditableTarget =
        !!active &&
        (active.isContentEditable ||
          active.tagName === "INPUT" ||
          active.tagName === "TEXTAREA" ||
          active.tagName === "SELECT");

      if (e.key === "Escape") {
        if (showCheatSheet) { setShowCheatSheet(false); return; }
        if (
          query === "" &&
          !showPluginManager &&
          !showSkillManager
        ) {
          // Already clean — hide the window
          getCurrentWebviewWindow().hide().catch(() => {});
        } else {
          setQuery("");
          setResults([]);
          setShowPluginManager(false);
          setShowSkillManager(false);
        }
      }

      if (
        e.key === "?" &&
        !e.ctrlKey &&
        !e.metaKey &&
        !e.altKey &&
        !e.repeat &&
        !isEditableTarget
      ) {
        e.preventDefault();
        setAiModeEnabled((prev) => !prev);
        setQuery("");
        setResults([]);
        setTimeout(() => focusInput(), 50);
      }

      if (e.key === "," && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setShowSettings(true);
      }

      if (e.key === "F1") {
        e.preventDefault();
        setShowCheatSheet((prev) => !prev);
      }

      if (e.key === "s" && (e.metaKey || e.ctrlKey) && isAiMode && conversationHistory.length > 0) {
        e.preventDefault();
        const md = conversationHistory.map(turn => {
          const role = turn.role === "user" ? "**You**" : "**AI**";
          const tools = turn.tools_used?.length
            ? `\n> Tools: ${turn.tools_used.join(", ")}\n`
            : "";
          return `${role}\n${tools}${turn.content}`;
        }).join("\n\n---\n\n");
        navigator.clipboard.writeText(md).catch(() => {});
        setExportToast(true);
        setTimeout(() => setExportToast(false), 2000);
      }

      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setAiModeEnabled((prev) => !prev);
        setQuery("");
        setResults([]);
        setTimeout(() => focusInput(), 50);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [focusInput, query, showPluginManager, showSkillManager, showCheatSheet, isAiMode, conversationHistory]);

  // ── Layout geometry ────────────────────────────────────────────────────────
  useEffect(() => {
    let saveTimer: ReturnType<typeof setTimeout>;
    const unlisten = getCurrentWebviewWindow().onMoved(({ payload }) => {
      clearTimeout(saveTimer);
      saveTimer = setTimeout(() => {
        invoke("save_window_position", { x: payload.x, y: payload.y }).catch(() => {});
      }, 500);
    });
    return () => { clearTimeout(saveTimer); unlisten.then(fn => fn()); };
  }, []);

  const launcherHasContent =
    results.length > 0 || showPluginManager || showSkillManager;
  const isCompactMode = !isAiMode && !launcherHasContent;
  const isPanelMode = showPluginManager || showSkillManager;
  const screenHeight = typeof window !== "undefined" ? window.screen.height : 1080;
  const compactHeight = Math.max(320, Math.round(screenHeight * 0.3));
  const aiHeight = Math.max(560, Math.round(screenHeight * 0.5));
  const panelHeight = isPanelMode
    ? Math.round(screenHeight * 0.4)
    : isAiMode
      ? aiHeight
      : launcherHasContent
        ? 520
        : isHintBarExpanded
          ? 320
          : compactHeight;
  const effectiveHeight = showSettings ? 560 : panelHeight;
  const windowHeight = `${effectiveHeight}px`;
  const maxHeight = `${effectiveHeight}px`;
  const shellFont =
    "'Aptos Display', 'Segoe UI Variable Display', 'Segoe UI', system-ui, sans-serif";

  useEffect(() => {
    invoke("set_window_geometry", {
      height: showSettings ? 560 : panelHeight,
      aiMode: isAiMode && !showSettings,
      panelMode: isPanelMode && !showSettings,
    }).catch(() => {});
  }, [panelHeight, isAiMode, isPanelMode, showSettings]);
  return (
    <>
      {/* Global keyframe animations injected once */}
      <style>{`
        @keyframes omni-fade-in {
          from { opacity: 0; transform: translateY(8px); }
          to   { opacity: 1; transform: translateY(0); }
        }
        @keyframes omni-dot-pulse {
          0%, 80%, 100% { opacity: 0.3; transform: scale(0.8); }
          40%            { opacity: 1;   transform: scale(1); }
        }
        @keyframes omni-blink {
          0%, 100% { opacity: 1; }
          50%       { opacity: 0; }
        }
        @keyframes omni-spin {
          to { transform: rotate(360deg); }
        }
        .omni-bubble-enter {
          animation: omni-fade-in 200ms ease both;
        }
        .omni-cursor::after {
          content: '|';
          animation: omni-blink 1s step-start infinite;
          color: inherit;
          margin-left: 1px;
        }
      `}</style>

      <div
        style={{
          width: "100%",
          height: windowHeight,
          maxHeight,
          background:
            resolvedTheme === "dark"
              ? backgroundUrl
                ? `
                  linear-gradient(180deg, rgba(6, 12, 24, 0.74) 0%, rgba(8, 14, 28, 0.86) 100%),
                  radial-gradient(circle at 18% -6%, ${colors.accent}1F 0, transparent 40%),
                  url("${backgroundUrl}") center top / cover no-repeat
                `
                : `linear-gradient(160deg, #0b1220 0%, #0e1930 52%, #0a1426 100%)`
              : colors.bg,
          color: colors.text,
          fontFamily: shellFont,
          borderRadius: "0",
          overflow: "hidden",
          boxShadow: "none",
          display: "flex",
          flexDirection: "column",
          justifyContent: isCompactMode ? "center" : "flex-start",
          padding: isCompactMode ? "0" : 0,
          boxSizing: "border-box",
          transition:
            "height 220ms cubic-bezier(0.4,0,0.2,1), max-height 220ms cubic-bezier(0.4,0,0.2,1)",
          outline: isAiMode ? `1.5px solid ${colors.accent}33` : "none",
        }}
      >
        {showSettings && (
          <SettingsWindow onClose={() => setShowSettings(false)} />
        )}
        {!showSettings && (<>
        {/* ── AI MODE: top bar ─────────────────────────────────────────── */}
        {isAiMode && (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              padding: "10px 16px 0",
              flexShrink: 0,
            }}
          >
            <span
              style={{
                fontSize: "13px",
                color: colors.accent,
                fontWeight: 600,
                letterSpacing: "0.03em",
              }}
            >
              OMNILAUNCHER AI MODE
            </span>
            <div style={{ display: "flex", alignItems: "center", gap: "8px", position: "relative" }}>
              <button
                onClick={() => setShowSessionPicker((v) => !v)}
                title="Switch sessions"
                style={{
                  background: showSessionPicker ? colors.surface2 : colors.surface,
                  border: "none",
                  borderRadius: "7px",
                  padding: "4px 11px",
                  color: colors.text,
                  cursor: "pointer",
                  fontSize: "12px",
                  display: "flex",
                  alignItems: "center",
                  gap: "6px",
                  maxWidth: "240px",
                }}
                onMouseEnter={(e) =>
                  (e.currentTarget.style.background = colors.surface2)
                }
                onMouseLeave={(e) =>
                  (e.currentTarget.style.background = showSessionPicker
                    ? colors.surface2
                    : colors.surface)
                }
              >
                <span style={{ fontSize: "10px" }}>💬</span>
                <span
                  style={{
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                    maxWidth: "180px",
                  }}
                >
                  {(() => {
                    const cur = sessions.find((s) => s.id === currentSessionId);
                    return cur && cur.title
                      ? cur.title
                      : currentSessionId
                        ? `Session #${currentSessionId}`
                        : "Session";
                  })()}
                </span>
                <span style={{ fontSize: "9px", opacity: 0.6 }}>▾</span>
              </button>
              <button
                onClick={handleNewConversation}
                style={{
                  background: colors.surface,
                  border: "none",
                  borderRadius: "7px",
                  padding: "4px 11px",
                  color: colors.text,
                  cursor: "pointer",
                  fontSize: "12px",
                  display: "flex",
                  alignItems: "center",
                  gap: "5px",
                }}
                onMouseEnter={(e) =>
                  (e.currentTarget.style.background = colors.surface2)
                }
                onMouseLeave={(e) =>
                  (e.currentTarget.style.background = colors.surface)
                }
              >
                <span style={{ fontSize: "10px" }}>✦</span> New conversation
              </button>

              {showSessionPicker && (
                <div
                  style={{
                    position: "absolute",
                    top: "calc(100% + 6px)",
                    right: 0,
                    width: "320px",
                    maxHeight: "360px",
                    overflowY: "auto",
                    background: colors.surface,
                    border: `1px solid ${colors.surface2}`,
                    borderRadius: "10px",
                    boxShadow: "0 12px 32px rgba(0,0,0,0.35)",
                    zIndex: 50,
                    padding: "6px",
                  }}
                  onClick={(e) => e.stopPropagation()}
                >
                  {sessions.length === 0 && (
                    <div
                      style={{
                        padding: "10px 12px",
                        fontSize: "12px",
                        color: colors.sub,
                      }}
                    >
                      No sessions yet.
                    </div>
                  )}
                  {sessions.map((s) => {
                    const active = s.id === currentSessionId;
                    return (
                      <div
                        key={s.id}
                        onClick={() => handleSwitchSession(s.id)}
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: "8px",
                          padding: "7px 9px",
                          borderRadius: "7px",
                          cursor: "pointer",
                          background: active ? colors.surface2 : "transparent",
                        }}
                        onMouseEnter={(e) =>
                          (e.currentTarget.style.background = colors.surface2)
                        }
                        onMouseLeave={(e) =>
                          (e.currentTarget.style.background = active
                            ? colors.surface2
                            : "transparent")
                        }
                      >
                        <div style={{ flex: 1, minWidth: 0 }}>
                          <div
                            style={{
                              fontSize: "12.5px",
                              color: colors.text,
                              fontWeight: active ? 600 : 500,
                              overflow: "hidden",
                              textOverflow: "ellipsis",
                              whiteSpace: "nowrap",
                            }}
                          >
                            {s.title || `Session #${s.id}`}
                          </div>
                          <div
                            style={{
                              fontSize: "10.5px",
                              color: colors.sub,
                              marginTop: "2px",
                              display: "flex",
                              gap: "8px",
                            }}
                          >
                            <span>{s.message_count} msg</span>
                            <span style={{ opacity: 0.6 }}>
                              {(s.last_active_at || "").slice(0, 16)}
                            </span>
                          </div>
                        </div>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            handleDeleteSession(s.id);
                          }}
                          title="Delete session"
                          style={{
                            background: "transparent",
                            border: "none",
                            color: colors.sub,
                            cursor: "pointer",
                            fontSize: "13px",
                            padding: "2px 6px",
                            borderRadius: "5px",
                          }}
                          onMouseEnter={(e) => {
                            e.currentTarget.style.background = "#a33";
                            e.currentTarget.style.color = "#fff";
                          }}
                          onMouseLeave={(e) => {
                            e.currentTarget.style.background = "transparent";
                            e.currentTarget.style.color = colors.sub;
                          }}
                        >
                          ×
                        </button>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          </div>
        )}

        {/* ── AI MODE: scrollable chat history ─────────────────────────── */}
        {isAiMode && !showSkillManager && (
          <div
            ref={chatScrollRef}
            style={{
              flex: 1,
              overflowY: "auto",
              padding: "12px 16px",
              display: "flex",
              flexDirection: "column",
              gap: "10px",
              scrollbarWidth: "thin",
              scrollbarColor: `${colors.surface2} transparent`,
            }}
          >
            {conversationHistory.length === 0 && (
              <div
                style={{
                  flex: 1,
                  display: "flex",
                  flexDirection: "column",
                  alignItems: "center",
                  justifyContent: "center",
                  color: colors.sub,
                  gap: "8px",
                  paddingBottom: "24px",
                  minHeight: "360px",
                }}
              >
                <span style={{ fontSize: "32px", opacity: 0.35 }}>✦</span>
                <span
                  style={{
                    fontSize: "13px",
                    textAlign: "center",
                    maxWidth: "280px",
                    lineHeight: 1.6,
                  }}
                >
                  Ask me anything — I can search the web, run calculations, and
                  more.
                </span>
              </div>
            )}

            {conversationHistory.map((turn, i) => (
              <ChatBubble key={i} turn={turn} colors={colors} />
            ))}
          </div>
        )}

        {/* ── PLUGIN MANAGER panel ─────────────────────────────────────── */}
        {showPluginManager && !isAiMode && (
          <PluginManager
            colors={colors}
            onClose={() => setShowPluginManager(false)}
          />
        )}

        {/* ── SKILL MANAGER panel ──────────────────────────────────────── */}
        {showSkillManager && (
          <SkillManager
            colors={colors}
            onClose={() => setShowSkillManager(false)}
          />
        )}

        {/* ── LAUNCHER MODE: results list ───────────────────────────────── */}
        {!isAiMode && !showPluginManager && !showSkillManager && (
          <>
            {query.trim() === "" && favorites.length > 0 && (
              <FavoritesList
                favoriteIds={favorites}
                onExecute={handleExecute}
                colors={colors}
                onFavoritesChange={setFavorites}
              />
            )}
            {results.length > 0 && (
              <ResultList results={results} query={query} onExecute={handleExecute} colors={colors} />
            )}
          </>
        )}

        {/* ── AI MODE: slash command suggestions overlay ────────────────── */}
        {isAiMode && results.length > 0 && isSlashPrefix(query) && (
          <ResultList
            results={results}
            query={query}
            onExecute={handleExecute}
            colors={colors}
          />
        )}

        <div
          style={{
            flexShrink: 0,
            paddingBottom: "2px",
            transform: isCompactMode ? "translateY(-18px)" : undefined,
          }}
        >
          {/* ── Search / input bar (always at bottom) ────────────────────── */}
          <SearchBar
            value={query}
            onChange={handleQueryChange}
            onSubmit={handleSubmit}
            isAiMode={isAiMode}
            loading={loading}
            onCancel={() => {
              invoke("ai_cancel").catch(() => {
                /* no-op: backend will emit ai-error which finishes the turn */
              });
            }}
            colors={colors}
            onSettingsClick={() => setShowSettings(true)}
            resolvedTheme={resolvedTheme}
            onThemeToggle={handleThemeToggle}
            showHintBar={
              !isAiMode && (isHelpHintQuery(query) || query.trim() === "")
            }
            compact={isCompactMode}
            inputRef={inputRef}
            onHintBarExpandedChange={setIsHintBarExpanded}
            inputHistory={inputHistory}
            historyIdx={historyIdx}
            onHistoryNavigate={(idx, val) => {
              setHistoryIdx(idx);
              setQuery(val);
            }}
          />
        </div>
        </>)}
      </div>

      {exportToast && (
        <div style={{
          position: "fixed", bottom: 16, left: "50%", transform: "translateX(-50%)",
          background: "#1a2d4a", border: "1px solid rgba(94,129,244,0.4)",
          borderRadius: 8, padding: "8px 16px",
          fontSize: 12, color: "#5E81F4", fontWeight: 600,
          zIndex: 9998, animation: "omni-fade-in 150ms ease both",
          pointerEvents: "none",
        }}>
          ✓ Conversation copied to clipboard
        </div>
      )}

      {showCheatSheet && (
        <div
          onClick={() => setShowCheatSheet(false)}
          style={{
            position: "fixed", inset: 0, zIndex: 9999,
            background: "rgba(0,0,0,0.7)",
            display: "flex", alignItems: "center", justifyContent: "center",
            animation: "omni-fade-in 150ms ease both",
          }}
        >
          <div
            onClick={(e) => e.stopPropagation()}
            style={{
              background: "#0d1117",
              border: "1px solid rgba(255,255,255,0.1)",
              borderRadius: 14,
              padding: "20px 28px",
              minWidth: 320,
              boxShadow: "0 24px 64px rgba(0,0,0,0.6)",
            }}
          >
            <div style={{ fontSize: 13, fontWeight: 700, color: "#e8eaf6", marginBottom: 16, letterSpacing: "0.04em" }}>
              ⌨ Keyboard Shortcuts
            </div>
            {[
              ["Ctrl+K", "Toggle AI mode"],
              ["Ctrl+,", "Open Settings"],
              ["F1", "Show/hide this help"],
              ["Escape", "Clear / Hide window"],
              ["↑ / ↓", "Navigate results"],
              ["↑ (empty)", "Browse input history"],
              ["Enter", "Execute selected"],
              ["Ctrl+Enter", "Force AI query"],
              ["?", "Toggle AI mode (key)"],
              ["/help", "Show all commands"],
              ["/new", "New AI conversation"],
              ["/plugins", "Plugin manager"],
              ["/skills", "Skill manager"],
              ["Right-click", "Context menu on result"],
              ["★ (hover)", "Favorite a result"],
            ].map(([key, desc]) => (
              <div key={key} style={{ display: "flex", justifyContent: "space-between", alignItems: "center", padding: "5px 0", borderBottom: "1px solid rgba(255,255,255,0.04)" }}>
                <kbd style={{
                  fontFamily: "monospace", fontSize: 11,
                  background: "rgba(255,255,255,0.08)",
                  border: "1px solid rgba(255,255,255,0.15)",
                  borderRadius: 5, padding: "2px 8px",
                  color: "#5E81F4",
                }}>{key}</kbd>
                <span style={{ fontSize: 12, color: "#8892b0", marginLeft: 16 }}>{desc}</span>
              </div>
            ))}
            <div style={{ fontSize: 11, color: "#4a5568", marginTop: 12, textAlign: "center" }}>
              Press F1 or click outside to close
            </div>
          </div>
        </div>
      )}
    </>
  );
}

// ─── Chat bubble sub-component

function toolIcon(tool: string): string {
  if (tool.startsWith("🎯")) return "";
  if (tool.includes("file")) return "📁";
  if (tool.includes("web") || tool.includes("search")) return "🌐";
  if (tool.includes("calc")) return "🧮";
  if (tool.includes("shell") || tool.includes("exec")) return "🔧";
  if (tool.includes("app")) return "🚀";
  if (tool.includes("clip")) return "📋";
  return "🔧";
}

function ChatBubble({
  turn,
  colors,
}: {
  turn: ConversationTurn;
  colors: typeof DARK_COLORS;
}) {
  const isUser = turn.role === "user";

  return (
    <div
      className="omni-bubble-enter"
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: isUser ? "flex-end" : "flex-start",
        gap: "5px",
      }}
    >
      {/* Tool chips — only for assistant, shown above the bubble */}
      {!isUser && turn.tools_used && turn.tools_used.length > 0 && (
        <div
          style={{
            display: "flex",
            gap: "5px",
            flexWrap: "wrap",
            paddingLeft: "4px",
          }}
        >
          {turn.tools_used.map((tool, i) => {
            const isSkill = tool.startsWith("🎯");
            const isActiveLast =
              turn.isStreaming && i === turn.tools_used!.length - 1;
            return (
              <span
                key={i}
                className={
                  isActiveLast
                    ? "chat-msg__tool-badge chat-msg__tool-badge--active"
                    : isSkill
                      ? "chat-msg__tool-badge chat-msg__tool-badge--skill"
                      : "chat-msg__tool-badge"
                }
              >
                {isSkill ? tool : `${toolIcon(tool)} ${tool}`}
              </span>
            );
          })}
        </div>
      )}

      {/* Bubble */}
      <div
        style={{
          maxWidth: "78%",
          padding: isUser ? "9px 14px" : "10px 14px",
          borderRadius: isUser ? "16px 16px 4px 16px" : "4px 16px 16px 16px",
          background: isUser ? colors.userBubble : colors.aiBubble,
          color: isUser ? colors.userBubbleText : colors.aiText,
          fontSize: "14px",
          lineHeight: "1.65",
          wordBreak: "break-word",
          // Assistant bubble gets a subtle accent left border
          borderLeft: !isUser ? `3px solid ${colors.accent}55` : "none",
          boxShadow: isUser
            ? `0 2px 8px ${colors.userBubble}44`
            : "0 1px 4px rgba(0,0,0,0.15)",
        }}
      >
        {turn.isStreaming ? (
          <LoadingDots color={colors.sub} />
        ) : isUser ? (
          <span>{turn.content}</span>
        ) : (
          <span
            className={turn.isStreaming ? "omni-cursor" : ""}
            dangerouslySetInnerHTML={{ __html: renderMarkdown(turn.content) }}
          />
        )}
      </div>
    </div>
  );
}

// ─── Loading dots (3-dot pulse) ────────────────────────────────────────────

function LoadingDots({ color }: { color: string }) {
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "5px",
        padding: "2px 0",
        height: "20px",
      }}
    >
      {[0, 1, 2].map((i) => (
        <span
          key={i}
          style={{
            width: "7px",
            height: "7px",
            borderRadius: "50%",
            background: color,
            display: "inline-block",
            animation: `omni-dot-pulse 1.4s ease-in-out ${i * 0.2}s infinite`,
          }}
        />
      ))}
    </span>
  );
}
