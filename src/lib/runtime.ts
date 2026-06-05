import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import {
  listen as tauriListen,
  emit as tauriEmit,
} from "@tauri-apps/api/event";
import { getCurrentWebviewWindow as tauriGetCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

const isTauriRuntime = () =>
  typeof window !== "undefined" && !!(window as any).__TAURI_INTERNALS__;

/// Window/OS-shell commands run in the local Tauri process — only it owns a
/// window. They bypass HTTP routing entirely even when a backend URL is set.
const WINDOW_LOCAL_COMMANDS = new Set<string>([
  "set_window_geometry",
  "set_window_size_centered",
  "save_window_position",
  "capture_vision_screenshot",
]);

export function isWindowLocalCommand(cmd: string): boolean {
  return WINDOW_LOCAL_COMMANDS.has(cmd);
}

/// Events emitted by the local Tauri process (window/hotkey/selection origin).
/// In the desktop shell these must use `tauriListen`; everything else
/// (ai-done, ai-error, ai-tool-call, plugin-runtime-progress, settings-saved)
/// originates on the remote backend and arrives over the SSE event stream.
const WINDOW_LOCAL_EVENTS = new Set<string>([
  "omnilauncher://shown",
  "omnilauncher://selection",
]);

/// Resolve the backend base URL lazily (read at call time, not module load) so
/// the desktop shell can inject `window.__OMNILAUNCHER_BACKEND_URL__` after the
/// frontend module has already evaluated, without a race.
function backendUrl(): string {
  if (typeof window !== "undefined") {
    const injected = (window as any).__OMNILAUNCHER_BACKEND_URL__;
    if (injected) return String(injected).trim();
  }
  return import.meta.env.VITE_OMNILAUNCHER_BACKEND_URL?.trim() || "";
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
  const response = await fetch(buildUrl(path), {
    headers: {
      "Content-Type": "application/json",
      ...(init?.headers ?? {}),
    },
    ...init,
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `HTTP ${response.status}`);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return response.json() as Promise<T>;
}

function dispatchLocalEvent<T>(name: string, payload: T) {
  eventTarget.dispatchEvent(new CustomEvent(name, { detail: payload }));
}

function ensureHttpEventStream(name: string) {
  if (!httpMode() || eventControllers.has(name)) return;
  const controller = new AbortController();
  eventControllers.set(name, controller);

  fetch(buildUrl(`/api/events/${encodeURIComponent(name)}`), {
    signal: controller.signal,
    headers: { Accept: "text/event-stream" },
  })
    .then(async (response) => {
      if (!response.ok || !response.body) {
        throw new Error(`Failed to subscribe: ${response.status}`);
      }
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
            dispatchLocalEvent(name, JSON.parse(raw));
          } catch {
            dispatchLocalEvent(name, raw as unknown);
          }
        }
      }
    })
    .catch((error) => {
      if (!controller.signal.aborted) {
        console.warn(`HTTP event stream ended for ${name}:`, error);
      }
    })
    .finally(() => {
      eventControllers.delete(name);
    });
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
  // Window/OS-shell commands always run in the local Tauri process — only it
  // owns a window. They bypass HTTP routing entirely.
  if (isWindowLocalCommand(cmd) && isTauriRuntime()) {
    return tauriInvoke<T>(cmd, args);
  }

  if (httpMode()) {
    switch (cmd) {
      case "search":
        return httpJson<T>("/api/search", {
          method: "POST",
          body: JSON.stringify({ query: args?.query ?? "" }),
        });
      case "get_settings":
        return httpJson<T>("/api/settings");
      case "save_settings_cmd":
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
      case "set_window_geometry":
      case "set_window_size_centered":
      case "save_window_position":
        return Promise.resolve(true as T);
      case "vision_analyze":
        throw new Error(
          "vision_analyze is only available in the Tauri desktop app",
        );
      default:
        throw new Error(
          `Command \"${cmd}\" is not available in browser mode yet.`,
        );
    }
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
      theme: "system",
      hotkey: "Alt+Space",
      max_results: 10,
      background_url: "",
    } as T;
  }
  return {} as T;
}

export async function listen<T>(
  eventName: string,
  handler: EventHandler<T>,
): Promise<Unlisten> {
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
