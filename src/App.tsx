import {
  useState,
  useEffect,
  useCallback,
  useRef,
  useMemo,
  memo,
  lazy,
  Suspense,
} from "react";
import { renderMarkdown } from "./utils/markdown";
import {
  loadLauncherConfig,
  isAiPrefix,
  isSlashPrefix,
  isPluginManagerQuery,
  isConversationResetCommand,
  isHelpQuery,
  isHelpHintQuery,
  slashSuggestions,
  helpResults,
} from "./launcherConfig";
import { invoke, listen, getCurrentWebviewWindow } from "./lib/runtime";
import SearchBar from "./components/SearchBar";
import ResultList from "./components/ResultList";

// Code-split heavy on-demand panels so they don't bloat the initial launcher bundle.
const SettingsWindow = lazy(() => import("./components/SettingsWindow"));
const PluginManager = lazy(() => import("./components/PluginManager"));
const SkillManager = lazy(() => import("./components/SkillManager"));

import type {
  QueryResult,
  AiResponse,
  ConversationTurn,
  AiSessionInfo,
  AppSettings,
} from "./types/app";

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
 * Routing rules now come from the backend via `launcherConfig` — see that
 * module for `isAiPrefix`, `isSlashPrefix`, `slashSuggestions`, etc.
 */

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

// Theme colors are defined in styles.css via [data-theme="dark"] / [data-theme="light"]
// CSS variables. Components read them with var(--bg), var(--accent), etc.

/// Debounce window between keystroke and backend `query_all`. 150ms balances
/// "feels instant" against "don't fire a query mid-burst" — every shaved ms
/// here shows up directly as launcher latency.
const SEARCH_DEBOUNCE_MS = 150;

// ─── App ──────────────────────────────────────────────────────────────────────

export default function App() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<QueryResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  // Monotonic request id — stale plugin responses (slower than a newer
  // keystroke's request) get dropped instead of clobbering fresh results.
  const searchSeqRef = useRef(0);
  const [loading, setLoading] = useState(false);
  const [aiModeEnabled, setAiModeEnabled] = useState(false);
  const [showPluginManager, setShowPluginManager] = useState(false);
  const [showSkillManager, setShowSkillManager] = useState(false);
  const [theme, setTheme] = useState<ThemeMode>("system");
  const [systemTheme, setSystemTheme] =
    useState<ResolvedTheme>(getSystemTheme());
  const [backgroundUrl, setBackgroundUrl] = useState<string>("");
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
  const cancelRequestedRef = useRef(false);
  const aiCleanupRef = useRef<(() => void) | null>(null);
  // Tracks whether the user has manually resized the window via the corner
  // grip. While true we stop auto-fitting the window to its content height so
  // the manual size sticks (remembered across hide/show via Ctrl+Shift+O).
  const userResizedRef = useRef(false);
  const [userResized, setUserResized] = useState(false);
  const [queueDepth, setQueueDepth] = useState(0);
  const [queuedPrompts, setQueuedPrompts] = useState<string[]>([]);

  // Favorites are persisted in the backend (SQLite). App owns the source of
  // truth and passes it down to ResultList; the component only renders + calls
  // the toggle callback.
  const [favoriteItems, setFavoriteItems] = useState<QueryResult[]>([]);
  const favorites = useMemo(
    () => new Set(favoriteItems.map((f) => f.id)),
    [favoriteItems],
  );

  const refreshFavorites = useCallback(async () => {
    try {
      const items = await invoke<QueryResult[]>("list_favorites");
      setFavoriteItems(items || []);
    } catch (e) {
      console.error("list_favorites error:", e);
    }
  }, []);

  const handleToggleFavorite = useCallback(
    async (item: QueryResult) => {
      const isFav = favorites.has(item.id);
      // Optimistic update so the star flips instantly.
      setFavoriteItems((prev) =>
        isFav ? prev.filter((f) => f.id !== item.id) : [...prev, item],
      );
      try {
        if (isFav) {
          await invoke("remove_favorite", { id: item.id });
        } else {
          await invoke("add_favorite", { result: item });
        }
      } catch (e) {
        console.error("toggle favorite error:", e);
        refreshFavorites(); // reconcile on failure
      }
    },
    [favorites, refreshFavorites],
  );
  const [showCheatSheet, setShowCheatSheet] = useState(false);
  const [exportToast, setExportToast] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

  const resolvedTheme: ResolvedTheme = theme === "system" ? systemTheme : theme;

  const handleThemeToggle = useCallback(async () => {
    const next: ThemeMode = resolvedTheme === "dark" ? "light" : "dark";
    setTheme(next);
    // Persist to backend settings so it survives restarts
    try {
      const current = await invoke<AppSettings>("get_settings");
      await invoke("save_settings_cmd", {
        settings: { ...current, theme: next },
      });
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
    // Cache the launcher rule-set (AI prefixes, slash catalog) so per-keystroke
    // predicates evaluate synchronously against backend-owned data.
    loadLauncherConfig();
    invoke<AppSettings>("get_settings")
      .then((s) => {
        setSettings(s);
        setTheme(parseThemeMode(s.theme));
        if (s.background_url) setBackgroundUrl(s.background_url);
      })
      .catch(() => {});
  }, []);

  // Hydrate favorites from the backend, running a one-time migration of any
  // favorites that were previously stored in localStorage.
  useEffect(() => {
    (async () => {
      try {
        if (!localStorage.getItem("omni-favorites-migrated")) {
          const ids: string[] = JSON.parse(
            localStorage.getItem("omni-favorites") || "[]",
          );
          const items: QueryResult[] = JSON.parse(
            localStorage.getItem("omni-favorite-items") || "[]",
          );
          for (const id of ids) {
            const item = items.find((r) => r.id === id);
            if (item) {
              try {
                await invoke("add_favorite", { result: item });
              } catch (e) {
                console.error("favorite migration error:", e);
              }
            }
          }
          localStorage.setItem("omni-favorites-migrated", "1");
          localStorage.removeItem("omni-favorites");
          localStorage.removeItem("omni-favorite-items");
        }
      } catch (e) {
        console.error("favorite migration failed:", e);
      }
      refreshFavorites();
    })();
  }, [refreshFavorites]);

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
    const unlisten = listen<AppSettings>(
      "omnilauncher://settings-saved",
      (e) => {
        setTheme(parseThemeMode(e.payload.theme));
        setBackgroundUrl(e.payload.background_url ?? "");
        setSettings(e.payload);
        setShowSettings(false);
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
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
    setSearchError(null);
    if (isHelpQuery(q)) {
      setResults(helpResults());
      setSearching(false);
      return;
    }

    if (isConversationResetCommand(q)) {
      setResults([]);
      setSearching(false);
      return;
    }

    if (!q.trim() || isAiPrefix(q) || isHelpHintQuery(q)) {
      setResults([]);
      setSearching(false);
      return;
    }

    // Plugin Manager shortcut
    if (isPluginManagerQuery(q)) {
      setResults([pluginManagerResult()]);
      setSearching(false);
      return;
    }

    // Slash prefix without a space → show autocomplete suggestions, no backend call
    if (isSlashPrefix(q)) {
      setResults(slashSuggestions(q));
      setSearching(false);
      return;
    }

    // Tag this request — only the latest one is allowed to update results.
    const mySeq = ++searchSeqRef.current;
    setSearching(true);
    try {
      const res = await invoke<QueryResult[]>("search", { query: q });
      if (mySeq !== searchSeqRef.current) return; // stale response — drop
      setResults(res);
      setSearchError(null);
    } catch (e) {
      if (mySeq !== searchSeqRef.current) return;
      setResults([]);
      setSearchError(
        e instanceof Error ? e.message : typeof e === "string" ? e : String(e),
      );
    } finally {
      if (mySeq === searchSeqRef.current) setSearching(false);
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
    const shouldFocusLauncherInput = () =>
      !showPluginManager && !showSkillManager;

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
      cancelRequestedRef.current = false;

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
        aiCleanupRef.current = null;
      };
      aiCleanupRef.current = cleanup;

      const finish = (content: string, tools_used?: string[]) => {
        const wasCancelled =
          cancelRequestedRef.current || content === "Error: Cancelled by user";
        cleanup();
        setConversationHistory((prev) => {
          const next = [...prev];
          const last = next[next.length - 1];
          if (last?.role === "assistant" && last.isStreaming) {
            next[next.length - 1] = {
              role: "assistant",
              content: wasCancelled ? "Cancelled." : content,
              tools_used: wasCancelled ? [] : tools_used,
              isStreaming: false,
            };
          }
          return next;
        });
        setLoading(false);
        if (!wasCancelled) {
          // Drain next queued prompt after loading state settles
          setTimeout(() => {
            const next = pendingQueueRef.current.shift();
            if (next) {
              setQueuedPrompts((prev) => prev.slice(1));
              setQueueDepth(pendingQueueRef.current.length);
              doAiQuery(next);
            }
          }, 50);
        }
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
        if (cancelRequestedRef.current) {
          cleanup();
          return;
        }
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

  const enqueueAiQuery = useCallback((value: string) => {
    pendingQueueRef.current.push(value);
    setQueuedPrompts((prev) => [...prev, value]);
    setQueueDepth(pendingQueueRef.current.length);
  }, []);

  const handleCancelAiRequest = useCallback(() => {
    cancelRequestedRef.current = true;
    pendingQueueRef.current = [];
    setQueuedPrompts([]);
    setQueueDepth(0);
    aiCleanupRef.current?.();

    setConversationHistory((prev) => {
      const next = [...prev];
      const last = next[next.length - 1];
      if (last?.role === "assistant" && last.isStreaming) {
        next[next.length - 1] = {
          role: "assistant",
          content: "Cancelled.",
          tools_used: [],
          isStreaming: false,
        };
      }
      return next;
    });
    setLoading(false);
    setTimeout(() => focusInput(), 50);

    invoke("ai_cancel").catch(() => {
      // The UI is already settled; backend cancellation is best-effort here.
    });
  }, [focusInput]);

  const handleQueryChange = useCallback(
    (value: string) => {
      // Strip the internal "__sel__:" sentinel if it ever surfaces in the
      // visible input (e.g. user backspaces into auto-populated selection).
      if (value.startsWith("__sel__:")) {
        value = value.slice("__sel__:".length);
      }
      setHistoryIdx(-1);
      if (isHelpQuery(value)) {
        if (debounceRef.current) clearTimeout(debounceRef.current);
        setQuery(value);
        setResults(helpResults());
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
        searchSeqRef.current++; // invalidate any in-flight backend search
        setSearching(false);
        setResults(slashSuggestions(value));
      } else if (!aiModeEnabled) {
        // Don't clear results immediately — let the debounced search replace
        // them. Clearing first causes the window to shrink then re-expand on
        // every keystroke (flash/flicker UX issue).
        debounceRef.current = setTimeout(() => {
          doSearch(value);
        }, SEARCH_DEBOUNCE_MS);
      } else {
        // In AI mode, clear slash suggestions when user types past the prefix
        searchSeqRef.current++;
        setSearching(false);
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
    pendingQueueRef.current = [];
    setQueuedPrompts([]);
    setQueueDepth(0);
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
          .map((m) => ({
            role: m.role as "user" | "assistant",
            content: m.content,
          }));
        setConversationHistory(turns);
        pendingQueueRef.current = [];
        setQueuedPrompts([]);
        setQueueDepth(0);
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
          pendingQueueRef.current = [];
          setQueuedPrompts([]);
          setQueueDepth(0);
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
        setResults(helpResults());
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
          inputHistoryRef.current = [
            value.trim(),
            ...inputHistoryRef.current,
          ].slice(0, 50);
          setInputHistory([...inputHistoryRef.current]);
        }
        setHistoryIdx(-1);
        if (loading) {
          enqueueAiQuery(value);
        } else {
          doAiQuery(value);
        }
        setQuery("");
      } else if (aiModeEnabled && value.trim()) {
        if (debounceRef.current) clearTimeout(debounceRef.current);
        if (value.trim() && inputHistoryRef.current[0] !== value.trim()) {
          inputHistoryRef.current = [
            value.trim(),
            ...inputHistoryRef.current,
          ].slice(0, 50);
          setInputHistory([...inputHistoryRef.current]);
        }
        setHistoryIdx(-1);
        if (loading) {
          enqueueAiQuery(value);
        } else {
          doAiQuery(value);
        }
        setQuery("");
      }
    },
    [aiModeEnabled, doAiQuery, enqueueAiQuery, handleNewConversation, loading],
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
          // Capture the screenshot locally (only the shell has a screen), then
          // send it to the backend for the AI vision call.
          const imageBase64 = await invoke<string>(
            "capture_vision_screenshot",
          );
          const response = await invoke<string>("vision_analyze", {
            prompt,
            imageBase64,
          });
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
  }, [conversationHistory, queuedPrompts, isAiMode]);

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
        if (showCheatSheet) {
          setShowCheatSheet(false);
          return;
        }
        if (loading && isAiMode) {
          e.preventDefault();
          handleCancelAiRequest();
          return;
        }
        if (query === "" && !showPluginManager && !showSkillManager) {
          // Already clean — hide the window
          getCurrentWebviewWindow()
            .hide()
            .catch(() => {});
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

      if (
        e.key === "s" &&
        (e.metaKey || e.ctrlKey) &&
        isAiMode &&
        conversationHistory.length > 0
      ) {
        e.preventDefault();
        const md = conversationHistory
          .map((turn) => {
            const role = turn.role === "user" ? "**You**" : "**AI**";
            const tools = turn.tools_used?.length
              ? `\n> Tools: ${turn.tools_used.join(", ")}\n`
              : "";
            return `${role}\n${tools}${turn.content}`;
          })
          .join("\n\n---\n\n");
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

      // Reset window back to its initial auto-fitted size.
      if (e.key === "0" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        if (userResizedRef.current) {
          userResizedRef.current = false;
          setUserResized(false);
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [
    focusInput,
    query,
    showPluginManager,
    showSkillManager,
    showCheatSheet,
    isAiMode,
    conversationHistory,
    loading,
    handleCancelAiRequest,
  ]);

  // ── Layout geometry ────────────────────────────────────────────────────────
  useEffect(() => {
    let saveTimer: ReturnType<typeof setTimeout>;
    const unlisten = getCurrentWebviewWindow().onMoved(({ payload }) => {
      clearTimeout(saveTimer);
      saveTimer = setTimeout(() => {
        invoke("save_window_position", { x: payload.x, y: payload.y }).catch(
          () => {},
        );
      }, 500);
    });
    return () => {
      clearTimeout(saveTimer);
      unlisten.then((fn) => fn());
    };
  }, []);

  const launcherHasContent =
    results.length > 0 || showPluginManager || showSkillManager;
  const isCompactMode = !isAiMode && !launcherHasContent;
  // In launcher (non-AI) mode, once there are results to show we lift the
  // search bar to the top and render the answer in a card below it.
  const launcherResultsMode =
    !isAiMode && !showPluginManager && !showSkillManager && results.length > 0;
  const isPanelMode = showPluginManager || showSkillManager;
  const screenHeight =
    typeof window !== "undefined" ? window.screen.height : 1080;
  const compactHeight = Math.max(320, Math.round(screenHeight * 0.3));
  const aiHeight = Math.max(560, Math.round(screenHeight * 0.5));
  const panelHeight = isPanelMode
    ? Math.round(screenHeight * 0.4)
    : isAiMode
      ? aiHeight
      : launcherHasContent
        ? 520
        : compactHeight;
  const effectiveHeight = showSettings ? 560 : panelHeight;
  const windowHeight = `${effectiveHeight}px`;
  const maxHeight = `${effectiveHeight}px`;
  const shellFont =
    "'Aptos Display', 'Segoe UI Variable Display', 'Segoe UI', system-ui, sans-serif";

  useEffect(() => {
    // Skip auto-fitting while the user is manually controlling the window size.
    if (userResizedRef.current) return;
    invoke("set_window_geometry", {
      height: showSettings ? 560 : panelHeight,
      aiMode: isAiMode && !showSettings,
      panelMode: isPanelMode && !showSettings,
    }).catch(() => {});
  }, [panelHeight, isAiMode, isPanelMode, showSettings, userResized]);

  // Corner resize grip: drag the bottom-right corner to resize the window while
  // keeping it centered on screen. Width/height grow at 2× the cursor delta so
  // the grabbed corner tracks the pointer exactly under centered layout.
  const resetWindowSize = useCallback(() => {
    if (!userResizedRef.current) return;
    // Clearing the manual-resize flag re-enables the content auto-fit effect,
    // which immediately re-applies the initial centered geometry and relayout.
    userResizedRef.current = false;
    setUserResized(false);
  }, []);

  const handleResizeStart = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      e.preventDefault();
      e.stopPropagation();
      userResizedRef.current = true;
      setUserResized(true);
      const startW = window.innerWidth;
      const startH = window.innerHeight;
      const startX = e.screenX;
      const startY = e.screenY;
      const minW = 360;
      const minH = 120;
      const maxW = window.screen.width;
      const maxH = window.screen.height;

      let raf = 0;
      let pending: { w: number; h: number } | null = null;
      const flush = () => {
        raf = 0;
        if (pending) {
          invoke("set_window_size_centered", {
            width: pending.w,
            height: pending.h,
          }).catch(() => {});
          pending = null;
        }
      };
      const onMove = (ev: PointerEvent) => {
        const w = Math.max(
          minW,
          Math.min(maxW, startW + 2 * (ev.screenX - startX)),
        );
        const h = Math.max(
          minH,
          Math.min(maxH, startH + 2 * (ev.screenY - startY)),
        );
        pending = { w, h };
        if (!raf) raf = requestAnimationFrame(flush);
      };
      const onUp = () => {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
        if (raf) {
          cancelAnimationFrame(raf);
          flush();
        }
      };
      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
    },
    [],
  );

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
          height: userResized ? "100vh" : windowHeight,
          maxHeight: userResized ? "100vh" : maxHeight,
          background:
            resolvedTheme === "dark"
              ? backgroundUrl
                ? `
                  linear-gradient(180deg, rgba(6, 12, 24, 0.74) 0%, rgba(8, 14, 28, 0.86) 100%),
                  radial-gradient(circle at 18% -6%, color-mix(in srgb, var(--accent) 12%, transparent) 0, transparent 40%),
                  url("${backgroundUrl}") center top / cover no-repeat
                `
                : `linear-gradient(160deg, #0b1220 0%, #0e1930 52%, #0a1426 100%)`
              : "var(--bg)",
          color: "var(--text)",
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
          outline: isAiMode
            ? `1.5px solid color-mix(in srgb, var(--accent) 20%, transparent)`
            : "none",
        }}
      >
        {showSettings && (
          <Suspense fallback={null}>
            <SettingsWindow onClose={() => setShowSettings(false)} />
          </Suspense>
        )}
        {!showSettings && (
          <>
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
                    color: "var(--accent)",
                    fontWeight: 600,
                    letterSpacing: "0.03em",
                  }}
                >
                  OMNILAUNCHER AI MODE
                </span>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "8px",
                    position: "relative",
                  }}
                >
                  <button
                    onClick={() => setShowSessionPicker((v) => !v)}
                    title="Switch sessions"
                    style={{
                      background: showSessionPicker
                        ? "var(--surface-2)"
                        : "var(--surface)",
                      border: "none",
                      borderRadius: "7px",
                      padding: "4px 11px",
                      color: "var(--text)",
                      cursor: "pointer",
                      fontSize: "12px",
                      display: "flex",
                      alignItems: "center",
                      gap: "6px",
                      maxWidth: "240px",
                    }}
                    onMouseEnter={(e) =>
                      (e.currentTarget.style.background = "var(--surface-2)")
                    }
                    onMouseLeave={(e) =>
                      (e.currentTarget.style.background = showSessionPicker
                        ? "var(--surface-2)"
                        : "var(--surface)")
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
                        const cur = sessions.find(
                          (s) => s.id === currentSessionId,
                        );
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
                      background: "var(--surface)",
                      border: "none",
                      borderRadius: "7px",
                      padding: "4px 11px",
                      color: "var(--text)",
                      cursor: "pointer",
                      fontSize: "12px",
                      display: "flex",
                      alignItems: "center",
                      gap: "5px",
                    }}
                    onMouseEnter={(e) =>
                      (e.currentTarget.style.background = "var(--surface-2)")
                    }
                    onMouseLeave={(e) =>
                      (e.currentTarget.style.background = "var(--surface)")
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
                        background: "var(--surface)",
                        border: `1px solid var(--surface-2)`,
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
                            color: "var(--sub)",
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
                              background: active
                                ? "var(--surface-2)"
                                : "transparent",
                            }}
                            onMouseEnter={(e) =>
                              (e.currentTarget.style.background =
                                "var(--surface-2)")
                            }
                            onMouseLeave={(e) =>
                              (e.currentTarget.style.background = active
                                ? "var(--surface-2)"
                                : "transparent")
                            }
                          >
                            <div style={{ flex: 1, minWidth: 0 }}>
                              <div
                                style={{
                                  fontSize: "12.5px",
                                  color: "var(--text)",
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
                                  color: "var(--sub)",
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
                                color: "var(--sub)",
                                cursor: "pointer",
                                fontSize: "13px",
                                padding: "2px 6px",
                                borderRadius: "5px",
                              }}
                              onMouseEnter={(e) => {
                                e.currentTarget.style.background =
                                  "var(--danger)";
                                e.currentTarget.style.color = "#fff";
                              }}
                              onMouseLeave={(e) => {
                                e.currentTarget.style.background =
                                  "transparent";
                                e.currentTarget.style.color = "var(--sub)";
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
                  scrollbarColor: `var(--surface-2) transparent`,
                }}
              >
                {conversationHistory.length === 0 &&
                  queuedPrompts.length === 0 && (
                    <div
                      style={{
                        flex: 1,
                        display: "flex",
                        flexDirection: "column",
                        alignItems: "center",
                        justifyContent: "center",
                        color: "var(--sub)",
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
                        Ask me anything — I can search the web, run
                        calculations, and more.
                      </span>
                    </div>
                  )}

                {conversationHistory.map((turn, i) => (
                  <ChatBubble key={i} turn={turn} />
                ))}

                {queuedPrompts.map((prompt, i) => (
                  <QueuedPromptBubble
                    key={`queued-${i}-${prompt}`}
                    prompt={prompt}
                  />
                ))}
              </div>
            )}

            {/* ── PLUGIN MANAGER panel ─────────────────────────────────────── */}
            {showPluginManager && !isAiMode && (
              <Suspense fallback={null}>
                <PluginManager onClose={() => setShowPluginManager(false)} />
              </Suspense>
            )}

            {/* ── SKILL MANAGER panel ──────────────────────────────────────── */}
            {showSkillManager && (
              <Suspense fallback={null}>
                <SkillManager onClose={() => setShowSkillManager(false)} />
              </Suspense>
            )}

            {/* ── LAUNCHER MODE: results list ───────────────────────────────── */}
            {!isAiMode && !showPluginManager && !showSkillManager && (
              <>
                {query.trim() === "" && favoriteItems.length > 0 && (
                  <ResultList
                    results={favoriteItems}
                    query=""
                    onExecute={handleExecute}
                    groupTitle="★ Favorites"
                    favorites={favorites}
                    onToggleFavorite={handleToggleFavorite}
                  />
                )}
                {results.length > 0 && (
                  <div
                    style={{
                      margin: "0 12px 8px",
                      background:
                        "color-mix(in srgb, var(--surface) 60%, transparent)",
                      border: "1px solid var(--border)",
                      borderRadius: "12px",
                      overflow: "hidden",
                      backdropFilter: "blur(10px)",
                      WebkitBackdropFilter: "blur(10px)",
                      boxShadow: "0 8px 24px rgba(0,0,0,0.18)",
                    }}
                  >
                    <ResultList
                      results={results}
                      query={query}
                      onExecute={handleExecute}
                      favorites={favorites}
                      onToggleFavorite={handleToggleFavorite}
                    />
                  </div>
                )}
                {/* Loading skeleton — only when the user has typed something and
                we're waiting on the backend (no stale results to show). */}
                {results.length === 0 && searching && query.trim() !== "" && (
                  <div
                    className="results"
                    aria-live="polite"
                    aria-busy="true"
                    style={{ padding: "8px 0" }}
                  >
                    {[0, 1, 2].map((i) => (
                      <div
                        key={i}
                        className="result-item"
                        style={{ cursor: "default", animation: "none" }}
                      >
                        <span
                          className="skeleton"
                          style={{
                            width: 22,
                            height: 22,
                            borderRadius: 6,
                            flexShrink: 0,
                          }}
                        />
                        <div className="result-item__content">
                          <span
                            className="skeleton"
                            style={{
                              display: "block",
                              height: 12,
                              width: `${70 - i * 12}%`,
                              marginBottom: 6,
                            }}
                          />
                          <span
                            className="skeleton"
                            style={{
                              display: "block",
                              height: 10,
                              width: `${50 - i * 8}%`,
                            }}
                          />
                        </div>
                      </div>
                    ))}
                  </div>
                )}
                {/* Error state — backend search call rejected. */}
                {results.length === 0 &&
                  !searching &&
                  searchError &&
                  query.trim() !== "" && (
                    <div
                      role="alert"
                      style={{
                        padding: "16px",
                        fontSize: 13,
                        color: "var(--error)",
                        lineHeight: 1.55,
                      }}
                    >
                      <div>⚠ Search failed: {searchError}</div>
                      <div
                        style={{
                          marginTop: 6,
                          fontSize: 12,
                          color: "var(--sub)",
                        }}
                      >
                        Edit your query to try again.
                      </div>
                    </div>
                  )}
                {/* Empty state — query typed, search finished, nothing matched. */}
                {results.length === 0 &&
                  !searching &&
                  !searchError &&
                  query.trim() !== "" &&
                  !isHelpQuery(query) &&
                  !isHelpHintQuery(query) &&
                  !isAiPrefix(query) &&
                  !isConversationResetCommand(query) && (
                    <div
                      style={{
                        padding: "16px",
                        textAlign: "center",
                        fontSize: 13,
                        color: "var(--sub)",
                        lineHeight: 1.55,
                      }}
                    >
                      <div style={{ fontSize: 22, marginBottom: 4 }}>🔍</div>
                      No matches for{" "}
                      <strong style={{ color: "var(--text)" }}>{query}</strong>
                      <div style={{ marginTop: 6, fontSize: 12 }}>
                        Press{" "}
                        <kbd
                          style={{
                            fontFamily: "monospace",
                            background:
                              "color-mix(in srgb, var(--accent) 12%, transparent)",
                            border: "1px solid var(--border)",
                            borderRadius: 4,
                            padding: "1px 6px",
                            color: "var(--accent)",
                          }}
                        >
                          Ctrl+K
                        </kbd>{" "}
                        to ask AI, or{" "}
                        <kbd
                          style={{
                            fontFamily: "monospace",
                            background:
                              "color-mix(in srgb, var(--accent) 12%, transparent)",
                            border: "1px solid var(--border)",
                            borderRadius: 4,
                            padding: "1px 6px",
                            color: "var(--accent)",
                          }}
                        >
                          ?
                        </kbd>{" "}
                        for help
                      </div>
                    </div>
                  )}
              </>
            )}

            {/* ── AI MODE: slash command suggestions overlay ────────────────── */}
            {isAiMode && results.length > 0 && isSlashPrefix(query) && (
              <ResultList
                results={results}
                query={query}
                onExecute={handleExecute}
                favorites={favorites}
                onToggleFavorite={handleToggleFavorite}
              />
            )}

            <div
              style={{
                flexShrink: 0,
                paddingBottom: "2px",
                paddingTop: launcherResultsMode ? "10px" : undefined,
                paddingLeft: launcherResultsMode ? "12px" : undefined,
                paddingRight: launcherResultsMode ? "12px" : undefined,
                order: launcherResultsMode ? -1 : undefined,
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
                queueDepth={queueDepth}
                onCancel={handleCancelAiRequest}
                onSettingsClick={() => setShowSettings(true)}
                resolvedTheme={resolvedTheme}
                onThemeToggle={handleThemeToggle}
                compact={isCompactMode}
                inputRef={inputRef}
                inputHistory={inputHistory}
                historyIdx={historyIdx}
                onHistoryNavigate={(idx, val) => {
                  setHistoryIdx(idx);
                  setQuery(val);
                }}
              />
            </div>

            {/* ── Bottom-right corner resize grip (keeps window centered) ──── */}
            <div
              onPointerDown={handleResizeStart}
              onDoubleClick={resetWindowSize}
              title="Drag to resize · Double-click or Ctrl+0 to reset"
              style={{
                position: "fixed",
                right: 0,
                bottom: 0,
                width: "18px",
                height: "18px",
                cursor: "nwse-resize",
                zIndex: 9990,
                background:
                  "linear-gradient(135deg, transparent 0 45%, color-mix(in srgb, var(--text) 35%, transparent) 45% 55%, transparent 55% 70%, color-mix(in srgb, var(--text) 35%, transparent) 70% 80%, transparent 80%)",
                opacity: 0.5,
              }}
              onMouseEnter={(e) => (e.currentTarget.style.opacity = "0.9")}
              onMouseLeave={(e) => (e.currentTarget.style.opacity = "0.5")}
            />
          </>
        )}
      </div>

      {exportToast && (
        <div
          style={{
            position: "fixed",
            bottom: 16,
            left: "50%",
            transform: "translateX(-50%)",
            background: "var(--bg-elevated)",
            border:
              "1px solid color-mix(in srgb, var(--accent) 40%, transparent)",
            borderRadius: 8,
            padding: "8px 16px",
            fontSize: 12,
            color: "var(--accent)",
            fontWeight: 600,
            zIndex: 9998,
            animation: "omni-fade-in 150ms ease both",
            pointerEvents: "none",
          }}
        >
          ✓ Conversation copied to clipboard
        </div>
      )}

      {showCheatSheet && (
        <div
          onClick={() => setShowCheatSheet(false)}
          style={{
            position: "fixed",
            inset: 0,
            zIndex: 9999,
            background: "rgba(0,0,0,0.7)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            animation: "omni-fade-in 150ms ease both",
          }}
        >
          <div
            onClick={(e) => e.stopPropagation()}
            style={{
              background: "var(--bg-elevated)",
              border: "1px solid var(--border)",
              borderRadius: 14,
              padding: "20px 28px",
              minWidth: 320,
              boxShadow: "0 24px 64px rgba(0,0,0,0.6)",
            }}
          >
            <div
              style={{
                fontSize: 13,
                fontWeight: 700,
                color: "var(--text)",
                marginBottom: 16,
                letterSpacing: "0.04em",
              }}
            >
              ⌨ Keyboard Shortcuts
            </div>
            {[
              ["Ctrl+K", "Toggle AI mode"],
              ["Ctrl+,", "Open Settings"],
              ["Ctrl+0", "Reset window size"],
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
              <div
                key={key}
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  padding: "5px 0",
                  borderBottom: "1px solid rgba(255,255,255,0.04)",
                }}
              >
                <kbd
                  style={{
                    fontFamily: "monospace",
                    fontSize: 11,
                    background: "rgba(255,255,255,0.08)",
                    border: "1px solid rgba(255,255,255,0.15)",
                    borderRadius: 5,
                    padding: "2px 8px",
                    color: "var(--accent)",
                  }}
                >
                  {key}
                </kbd>
                <span
                  style={{
                    fontSize: 12,
                    color: "var(--text-secondary)",
                    marginLeft: 16,
                  }}
                >
                  {desc}
                </span>
              </div>
            ))}
            <div
              style={{
                fontSize: 11,
                color: "var(--sub)",
                marginTop: 12,
                textAlign: "center",
              }}
            >
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

function QueuedPromptBubble({ prompt }: { prompt: string }) {
  return (
    <div
      className="omni-bubble-enter"
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "flex-end",
        gap: "5px",
        opacity: 0.72,
      }}
    >
      <span
        style={{
          fontSize: "11px",
          color: "var(--sub)",
          paddingRight: "6px",
          letterSpacing: 0,
        }}
      >
        Queued
      </span>
      <div
        style={{
          maxWidth: "78%",
          padding: "9px 14px",
          borderRadius: "16px 16px 4px 16px",
          background: `color-mix(in srgb, var(--user-bubble) 50%, transparent)`,
          color: "var(--user-bubble-text)",
          fontSize: "14px",
          lineHeight: "1.65",
          wordBreak: "break-word",
          border: `1px dashed color-mix(in srgb, var(--accent) 40%, transparent)`,
        }}
      >
        {prompt}
      </div>
    </div>
  );
}

const ChatBubble = memo(function ChatBubble({
  turn,
}: {
  turn: ConversationTurn;
}) {
  const isUser = turn.role === "user";
  // Memoize the expensive markdown render so streaming a sibling bubble doesn't re-render this one.
  const renderedHtml = useMemo(
    () => (isUser ? null : renderMarkdown(turn.content)),
    [isUser, turn.content],
  );

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
          background: isUser ? "var(--user-bubble)" : "var(--ai-bubble)",
          color: isUser ? "var(--user-bubble-text)" : "var(--ai-text)",
          fontSize: "14px",
          lineHeight: "1.65",
          wordBreak: "break-word",
          // Assistant bubble gets a subtle accent left border
          borderLeft: !isUser
            ? `3px solid color-mix(in srgb, var(--accent) 33%, transparent)`
            : "none",
          boxShadow: isUser
            ? `0 2px 8px color-mix(in srgb, var(--user-bubble) 27%, transparent)`
            : "0 1px 4px rgba(0,0,0,0.15)",
        }}
      >
        {turn.isStreaming ? (
          <LoadingDots color="var(--sub)" />
        ) : isUser ? (
          <span>{turn.content}</span>
        ) : (
          <span
            className={turn.isStreaming ? "omni-cursor" : ""}
            dangerouslySetInnerHTML={{ __html: renderedHtml || "" }}
          />
        )}
      </div>
    </div>
  );
});

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
