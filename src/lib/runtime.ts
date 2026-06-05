import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import {
  listen as tauriListen,
  emit as tauriEmit,
} from "@tauri-apps/api/event";
import { getCurrentWebviewWindow as tauriGetCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

const isTauriRuntime = () =>
  typeof window !== "undefined" && !!(window as any).__TAURI_INTERNALS__;
const browserBackendUrl =
  import.meta.env.VITE_OMNILAUNCHER_BACKEND_URL?.trim() || "";
const isHttpMode = !isTauriRuntime() && !!browserBackendUrl;

export function getBackendMode(): "tauri" | "http" | "mock" {
  if (isTauriRuntime()) return "tauri";
  if (isHttpMode) return "http";
  return "mock";
}

type EventHandler<T> = (event: { payload: T }) => void;
type Unlisten = () => void;

const eventTarget = new EventTarget();
const eventControllers = new Map<string, AbortController>();
let selectionPollTimer: number | null = null;
let lastSelectionToken = "";

function buildUrl(path: string): string {
  return `${browserBackendUrl}${path}`;
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
  if (!isHttpMode || eventControllers.has(name)) return;
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
  if (!isHttpMode || selectionPollTimer !== null) return;
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
  if (isTauriRuntime()) {
    return tauriInvoke<T>(cmd, args);
  }

  if (isHttpMode) {
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
  if (isTauriRuntime()) {
    return tauriListen<T>(eventName, handler);
  }

  if (isHttpMode) {
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
      if (!isHttpMode) return;
      await httpJson("/api/window/hide", { method: "POST" });
    },
  };
}
