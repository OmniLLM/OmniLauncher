import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import {
  listen as tauriListen,
  emit as tauriEmit,
} from "@tauri-apps/api/event";
import { getCurrentWebviewWindow as tauriGetCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { logger, summarizeArgs, type LogLevel } from "./logger";

const isTauriRuntime = () =>
  typeof window !== "undefined" && !!(window as any).__TAURI_INTERNALS__;

type FrontendLogLevel = LogLevel;

// Local thin wrapper so existing call sites keep their `[runtime]` prefix
// while routing through the centralized logger (level-gated, forwards to
// the Tauri `frontend_log` command when available).
function frontendLog(level: FrontendLogLevel, message: string) {
  logger[level](`[runtime] ${message}`);
}

let lastLoggedBackendUrl = "__unset__";
let promptedBackendToken: { url: string; token: string } | null = null;

function normalizeToken(token: unknown): string {
  return typeof token === "string" ? token.trim() : "";
}

async function readFrontendBackendToken(): Promise<string> {
  if (!isTauriRuntime()) return "";
  return tauriInvoke<string>("get_frontend_backend_token")
    .then(normalizeToken)
    .catch(() => "");
}

async function saveFrontendBackendToken(token: string): Promise<void> {
  if (!isTauriRuntime()) return;
  await tauriInvoke("save_frontend_backend_token", { token });
}

async function promptForBackendToken(): Promise<string> {
  if (typeof window === "undefined" || !isTauriRuntime()) return "";
  const entered = normalizeToken(
    window.prompt?.("Enter the backend token to connect to OmniLauncher:", ""),
  );
  if (!entered) return "";

  promptedBackendToken = { url: backendUrl(), token: entered };
  await saveFrontendBackendToken(entered).catch((error) => {
    frontendLog(
      "warn",
      `failed to save frontend backend token: ${error instanceof Error ? error.message : String(error)}`,
    );
  });
  return entered;
}

export async function ensureBackendToken(): Promise<string> {
  if (typeof window === "undefined" || !httpMode() || !isTauriRuntime()) return "";

  const injected = normalizeToken((window as any).__OMNILAUNCHER_TOKEN__);
  if (injected) return injected;

  const stored = await readFrontendBackendToken();
  if (stored) {
    return stored;
  }

  if (promptedBackendToken?.url === backendUrl()) {
    return promptedBackendToken.token;
  }

  return promptForBackendToken();
}

async function promptForReplacementBackendToken(): Promise<string> {
  if (typeof window === "undefined" || !httpMode() || !isTauriRuntime()) return "";
  (window as any).__OMNILAUNCHER_TOKEN__ = "";
  promptedBackendToken = null;
  return promptForBackendToken();
}

async function activeBackendToken(): Promise<string> {
  if (typeof window !== "undefined") {
    const injected = normalizeToken((window as any).__OMNILAUNCHER_TOKEN__);
    if (injected) return injected;
  }
  if (promptedBackendToken?.url === backendUrl()) {
    return promptedBackendToken.token;
  }
  return resolveServerToken();
}

/// Per-launch auth token for the API server. Source priority:
///   1. `window.__OMNILAUNCHER_TOKEN__` injected by the Tauri shell.
///   2. Frontend-local `~/.config/omnilauncher/backend-token` via Tauri.
///   3. Prompted token cached in-page and saved to the frontend-local file.
///   4. Empty string — browser / mock / no-auth mode.
///
/// Read lazily on each request so settings changes can rotate the token
/// without restarting the app.
async function resolveServerToken(): Promise<string> {
  if (typeof window !== "undefined") {
    const injected = normalizeToken((window as any).__OMNILAUNCHER_TOKEN__);
    if (injected) {
      return injected;
    }
    const ensured = await ensureBackendToken();
    if (ensured) {
      return ensured;
    }
    if (isTauriRuntime()) {
      return tauriInvoke<string>("get_server_token").then(normalizeToken).catch(() => "");
    }
  }
  return "";
}

/// Window/OS-shell commands run in the local Tauri process — only it owns a
/// window. They bypass HTTP routing entirely even when a backend URL is set.
const WINDOW_LOCAL_COMMANDS = new Set<string>([
  "set_window_geometry",
  "set_window_size_centered",
  "save_window_position",
  "capture_vision_screenshot",
]);

const TAURI_NATIVE_COMMANDS = new Set<string>([]);

function isNativeCommand(cmd: string): boolean {
  return WINDOW_LOCAL_COMMANDS.has(cmd) || TAURI_NATIVE_COMMANDS.has(cmd);
}

export function isWindowLocalCommand(cmd: string): boolean {
  return WINDOW_LOCAL_COMMANDS.has(cmd);
}

/// Events emitted by the local Tauri process (window/hotkey/selection/settings origin).
/// In the desktop shell these must use `tauriListen`; everything else
/// (ai-done, ai-error, ai-tool-call, plugin-runtime-progress)
/// originates on the remote backend and arrives over the SSE event stream.
const WINDOW_LOCAL_EVENTS = new Set<string>([
  "omnilauncher://shown",
  "omnilauncher://selection",
  "omnilauncher://settings-saved",
]);

/// Resolve the backend base URL lazily (read at call time, not module load) so
/// the desktop shell can inject `window.__OMNILAUNCHER_BACKEND_URL__` after the
/// frontend module has already evaluated, without a race.
function backendUrl(): string {
  let resolved = "";
  let source = "vite-env";
  if (typeof window !== "undefined") {
    const injected = (window as any).__OMNILAUNCHER_BACKEND_URL__;
    if (injected) {
      resolved = String(injected).trim();
      source = "tauri-injected-window";
    }
  }
  if (!resolved) {
    resolved = import.meta.env.VITE_OMNILAUNCHER_BACKEND_URL?.trim() || "";
  }

  if (resolved !== lastLoggedBackendUrl) {
    lastLoggedBackendUrl = resolved;
    frontendLog(
      "info",
      `backend URL resolved from ${source}: ${resolved || "[empty - using local/mock runtime]"}`,
    );
  }
  return resolved;
}

/// HTTP mode is active whenever a backend URL is known — including inside the
/// Tauri shell, which now delegates business logic to the remote backend.
function httpMode(): boolean {
  return !!backendUrl();
}

export function getBackendMode(): "tauri" | "http" | "mock" {
  if (httpMode()) return "http";
  if (isTauriRuntime()) return "tauri";
  return "mock";
}

/// Public accessor for the resolved backend URL. Returns an empty string when
/// the launcher is running in pure-Tauri mode (no HTTP backend configured).
export function getBackendUrl(): string {
  return backendUrl();
}

type EventHandler<T> = (event: { payload: T }) => void;
type Unlisten = () => void;

const eventTarget = new EventTarget();
const eventControllers = new Map<string, AbortController>();
let selectionPollTimer: number | null = null;
let lastSelectionToken = "";

function buildUrl(path: string): string {
  return `${backendUrl()}${path}`;
}

async function httpJson<T>(path: string, init?: RequestInit): Promise<T> {
  return httpJsonWithRetry<T>(path, init, true);
}

async function httpJsonWithRetry<T>(
  path: string,
  init: RequestInit | undefined,
  retryAuth: boolean,
): Promise<T> {
  const url = buildUrl(path);
  const method = init?.method ?? "GET";
  const start = performance.now();
  const bodySummary = typeof init?.body === "string" ? `${init.body.length} bytes` : "none";
  frontendLog("debug", `HTTP ${method} ${url} start body=${bodySummary}`);

  const serverToken = await activeBackendToken();

  let response: Response;
  try {
    response = await fetch(url, {
      ...init,
      headers: {
        "Content-Type": "application/json",
        ...(serverToken ? { "X-OmniLauncher-Token": serverToken } : {}),
        ...(init?.headers ?? {}),
      },
    });
  } catch (error) {
    frontendLog(
      "error",
      `HTTP ${method} ${url} network failure after ${Math.round(performance.now() - start)}ms: ${error instanceof Error ? error.message : String(error)}`,
    );
    throw error;
  }

  const elapsed = Math.round(performance.now() - start);
  frontendLog("debug", `HTTP ${method} ${url} response status=${response.status} elapsed=${elapsed}ms`);

  if (response.status === 401 && retryAuth) {
    const replacementToken = await promptForReplacementBackendToken();
    if (replacementToken) {
      return httpJsonWithRetry<T>(path, init, false);
    }
  }

  if (!response.ok) {
    const text = await response.text();
    frontendLog("error", `HTTP ${method} ${url} failed status=${response.status} body=${text.slice(0, 500)}`);
    throw new Error(text || `HTTP ${response.status}`);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  try {
    return (await response.json()) as T;
  } catch (error) {
    frontendLog(
      "error",
      `HTTP ${method} ${url} JSON parse failure: ${error instanceof Error ? error.message : String(error)}`,
    );
    throw error;
  }
}

function dispatchLocalEvent<T>(name: string, payload: T) {
  eventTarget.dispatchEvent(new CustomEvent(name, { detail: payload }));
}

function ensureHttpEventStream(name: string) {
  if (!httpMode() || eventControllers.has(name)) return;
  const controller = new AbortController();
  eventControllers.set(name, controller);
  const url = buildUrl(`/api/events/${encodeURIComponent(name)}`);
  frontendLog("debug", `SSE subscribe ${name} via ${url}`);

  resolveServerToken().then((serverToken) => {
    fetch(url, {
      signal: controller.signal,
      headers: {
        Accept: "text/event-stream",
        ...(serverToken ? { "X-OmniLauncher-Token": serverToken } : {}),
      },
    })
    .then(async (response) => {
      if (!response.ok || !response.body) {
        throw new Error(`Failed to subscribe: ${response.status}`);
      }
      frontendLog("debug", `SSE connected ${name} status=${response.status}`);
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const chunks = buffer.split("\n\n");
        buffer = chunks.pop() ?? "";
        for (const chunk of chunks) {
          const line = chunk
            .split("\n")
            .find((part) => part.startsWith("data: "));
          if (!line) continue;
          const raw = line.slice(6);
          try {
            const parsed = JSON.parse(raw);
            frontendLog("trace", `SSE event ${name} payload=${summarizeArgs({ payload: parsed })}`);
            dispatchLocalEvent(name, parsed);
          } catch {
            frontendLog("trace", `SSE event ${name} raw=${raw.slice(0, 200)}`);
            dispatchLocalEvent(name, raw as unknown);
          }
        }
      }
    })
    .catch((error) => {
      if (!controller.signal.aborted) {
        frontendLog(
          "warn",
          `SSE stream ended for ${name}: ${error instanceof Error ? error.message : String(error)}`,
        );
        console.warn(`HTTP event stream ended for ${name}:`, error);
      }
    })
    .finally(() => {
      frontendLog("debug", `SSE closed ${name}`);
      eventControllers.delete(name);
    });
  }); // end serverTokenPromise.then
}

function ensureSelectionPolling() {
  if (!httpMode() || selectionPollTimer !== null) return;
  selectionPollTimer = window.setInterval(async () => {
    try {
      const payload = await httpJson<{
        token: string;
        selection: string;
      } | null>("/api/selection/latest");
      if (!payload || !payload.token || payload.token === lastSelectionToken)
        return;
      lastSelectionToken = payload.token;
      dispatchLocalEvent("omnilauncher://selection", payload.selection);
    } catch {
      // ignore polling errors in browser mode
    }
  }, 750);
}

export async function invoke<T = unknown>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const mode = getBackendMode();
  frontendLog("debug", `invoke ${cmd} mode=${mode} args=${summarizeArgs(args)}`);

  // Window/OS-shell commands and authoritative native commands always run in
  // the local Tauri process when available. The separated HTTP backend cannot
  // own OS window state or live global shortcut registration.
  if (isNativeCommand(cmd) && isTauriRuntime()) {
    frontendLog("debug", `invoke ${cmd} routed to local Tauri command`);
    return tauriInvoke<T>(cmd, args);
  }

  if (mode === "http") {
    switch (cmd) {
      case "search":
        return httpJson<T>("/api/search", {
          method: "POST",
          body: JSON.stringify({ query: args?.query ?? "" }),
        });
      case "get_settings":
        return httpJson<T>("/api/settings");
      case "save_settings_cmd":
      case "set_hotkey_cmd":
        if (isTauriRuntime() && !(await activeBackendToken())) {
          throw new Error("Backend token is required to save settings");
        }
        return httpJson<T>("/api/settings", {
          method: "POST",
          body: JSON.stringify(args?.settings ?? {}),
        });
      case "list_models":
        return httpJson<T>("/api/models", {
          method: "POST",
          body: JSON.stringify({
            base_url: args?.baseUrl,
            api_key: args?.apiKey,
          }),
        });
      case "get_launcher_config":
        return httpJson<T>("/api/launcher-config");
      case "list_favorites":
        return httpJson<T>("/api/favorites");
      case "add_favorite":
        return httpJson<T>("/api/favorites", {
          method: "POST",
          body: JSON.stringify({ result: args?.result }),
        });
      case "remove_favorite":
        return httpJson<T>(
          `/api/favorites/${encodeURIComponent(String(args?.id ?? ""))}`,
          { method: "DELETE" },
        );
      case "list_ai_sessions":
        return httpJson<T>("/api/sessions");
      case "current_ai_session":
        return httpJson<T>("/api/sessions/current");
      case "switch_ai_session":
        return httpJson<T>("/api/sessions/switch", {
          method: "POST",
          body: JSON.stringify({ session_id: args?.sessionId }),
        });
      case "delete_ai_session":
        return httpJson<T>("/api/sessions/delete", {
          method: "POST",
          body: JSON.stringify({ session_id: args?.sessionId }),
        });
      case "clear_conversation":
        return httpJson<T>("/api/sessions/clear", { method: "POST" });
      case "ai_query":
        return httpJson<T>("/api/ai/query", {
          method: "POST",
          body: JSON.stringify({ query: args?.query ?? "" }),
        });
      case "ai_cancel":
        return httpJson<T>("/api/ai/cancel", { method: "POST" });
      case "execute_result":
        return httpJson<T>("/api/execute-result", {
          method: "POST",
          body: JSON.stringify({ result: args?.result }),
        });
      case "slash_preview":
        return httpJson<T>("/api/slash/preview", {
          method: "POST",
          body: JSON.stringify({ query: args?.query ?? "" }),
        });
      case "execute_slash_command":
        return httpJson<T>("/api/slash/execute", {
          method: "POST",
          body: JSON.stringify({ query: args?.query ?? "" }),
        });
      case "list_skills":
        return httpJson<T>("/api/skills");
      case "list_skill_usage":
        return httpJson<T>("/api/skills/usage");
      case "install_skill":
        return httpJson<T>("/api/skills/install", {
          method: "POST",
          body: JSON.stringify({ source: args?.source }),
        });
      case "update_skill":
        return httpJson<T>("/api/skills/update", {
          method: "POST",
          body: JSON.stringify({ name: args?.name }),
        });
      case "delete_skill":
        return httpJson<T>("/api/skills/delete", {
          method: "POST",
          body: JSON.stringify({ name: args?.name }),
        });
      case "pin_skill":
        return httpJson<T>("/api/skills/pin", {
          method: "POST",
          body: JSON.stringify({ name: args?.name, pinned: args?.pinned }),
        });
      case "run_curator_now":
        return httpJson<T>("/api/skills/curator/run", { method: "POST" });
      case "propose_skill_consolidation":
        return httpJson<T>("/api/skills/consolidation/propose", {
          method: "POST",
        });
      case "apply_skill_consolidation":
        return httpJson<T>("/api/skills/consolidation/apply", {
          method: "POST",
          body: JSON.stringify({ proposal: args?.proposal }),
        });
      case "list_plugin_collections":
        return httpJson<T>("/api/plugins/collections");
      case "list_plugin_runtime_dependencies":
        return httpJson<T>("/api/plugins/runtime-deps");
      case "install_plugin":
        return httpJson<T>("/api/plugins/install", {
          method: "POST",
          body: JSON.stringify({
            source: args?.source,
            target_dir: args?.targetDir,
          }),
        });
      case "update_plugin":
        return httpJson<T>("/api/plugins/update", {
          method: "POST",
          body: JSON.stringify({ name: args?.name }),
        });
      case "update_plugin_collection_all":
        return httpJson<T>("/api/plugins/collections/update", {
          method: "POST",
          body: JSON.stringify({
            collection_source: args?.collectionSource,
            repo_dirs: args?.repoDirs,
            git_repo_dirs: args?.gitRepoDirs,
          }),
        });
      case "remove_plugin_collection":
        return httpJson<T>("/api/plugins/collections/remove", {
          method: "POST",
          body: JSON.stringify({ repo_dirs: args?.repoDirs }),
        });
      case "install_plugin_runtime_dependency":
        return httpJson<T>("/api/plugins/runtime-deps/install", {
          method: "POST",
          body: JSON.stringify({ id: args?.id }),
        });
      case "set_window_geometry":
      case "set_window_size_centered":
      case "save_window_position":
        return Promise.resolve(true as T);
      case "vision_analyze":
        return httpJson<T>("/api/vision/analyze", {
          method: "POST",
          body: JSON.stringify({
            prompt: args?.prompt,
            image_base64: args?.imageBase64,
          }),
        });
      default:
        throw new Error(
          `Command \"${cmd}\" is not available in browser mode yet.`,
        );
    }
  }

  if (isTauriRuntime() && (cmd === "save_settings_cmd" || cmd === "set_hotkey_cmd")) {
    frontendLog("debug", `invoke ${cmd} routed to local Tauri command without backend`);
    return tauriInvoke<T>(cmd, args);
  }

  console.warn(`[Tauri Shim] Mock invoke for: ${cmd}`);
  if (cmd === "search") {
    return [
      {
        id: "1",
        title: "Calculator",
        subtitle: "App",
        score: 1,
        action_type: "open",
        action_data: "calc",
        icon: "🧮",
      },
      {
        id: "2",
        title: "Notepad",
        subtitle: "App",
        score: 0.8,
        action_type: "open",
        action_data: "notepad",
        icon: "📝",
      },
    ] as T;
  }
  if (cmd === "get_settings") {
    return {
      ai_base_url: "",
      ai_api_key: "",
      ai_model: "gpt-4",
      ai_timeout_secs: 120,
      ai_max_tool_iterations: 10,
      theme: "system",
      hotkey: "Ctrl+Shift+O",
      max_results: 10,
      background_url: "",
      backend_url: "",
    } as T;
  }
  if (cmd === "list_favorites") {
    return [] as T;
  }
  if (cmd === "current_ai_session") {
    return 0 as T;
  }
  if (cmd === "switch_ai_session") {
    return [] as T;
  }
  if (cmd === "set_hotkey_cmd") {
    return (args?.settings ?? {}) as T;
  }
  if (cmd === "save_settings_cmd") {
    return true as T;
  }
  return {} as T;
}

export async function listen<T>(
  eventName: string,
  handler: EventHandler<T>,
): Promise<Unlisten> {
  frontendLog("debug", `listen ${eventName} mode=${getBackendMode()}`);
  // Window-origin events (shown/selection) are emitted by the local Tauri
  // process, so prefer the Tauri listener for them when running in the shell.
  if (WINDOW_LOCAL_EVENTS.has(eventName) && isTauriRuntime()) {
    return tauriListen<T>(eventName, handler);
  }

  // Everything else (AI + progress events) originates on the backend and
  // arrives over SSE whenever a backend URL is configured.
  if (httpMode()) {
    ensureHttpEventStream(eventName);
    if (eventName === "omnilauncher://selection") {
      ensureSelectionPolling();
    }
    const listener = (event: Event) => {
      handler({ payload: (event as CustomEvent<T>).detail });
    };
    eventTarget.addEventListener(eventName, listener as EventListener);
    return () =>
      eventTarget.removeEventListener(eventName, listener as EventListener);
  }

  // No backend configured: fall back to the local Tauri event bus if present.
  if (isTauriRuntime()) {
    return tauriListen<T>(eventName, handler);
  }

  return () => {};
}

export async function emit<T>(eventName: string, payload?: T): Promise<void> {
  frontendLog("debug", `emit ${eventName} mode=${getBackendMode()} payload=${summarizeArgs({ payload })}`);
  if (isTauriRuntime()) {
    return tauriEmit(eventName, payload);
  }

  dispatchLocalEvent(eventName, payload as T);
}

export function getCurrentWebviewWindow() {
  if (isTauriRuntime()) {
    return tauriGetCurrentWebviewWindow();
  }

  return {
    async onFocusChanged(handler: (event: { payload: boolean }) => void) {
      const listener = () => handler({ payload: true });
      window.addEventListener("focus", listener);
      return () => window.removeEventListener("focus", listener);
    },
    async onMoved(
      _handler: (event: { payload: { x: number; y: number } }) => void,
    ) {
      return () => {};
    },
    async hide() {
      if (!httpMode()) return;
      await httpJson("/api/window/hide", { method: "POST" });
    },
  };
}
