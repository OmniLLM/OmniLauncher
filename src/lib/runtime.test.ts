import { describe, it, expect, afterEach, vi, beforeEach } from "vitest";
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import mainRsSource from "../../src-tauri/src/main.rs?raw";
import { isWindowLocalCommand, invoke, getBackendMode, listen, emit, ensureBackendToken } from "./runtime";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Set up a mock backend URL and fetch spy. Returns the captured request info. */
function mockBackend(): { calls: { url: string; method: string; body?: string; headers: Record<string, string> }[] } {
  const state = { calls: [] as { url: string; method: string; body?: string; headers: Record<string, string> }[] };
  (globalThis as any).window = {
    __OMNILAUNCHER_BACKEND_URL__: "http://test.local",
    addEventListener: () => {},
    removeEventListener: () => {},
  };
  (globalThis as any).fetch = async (url: string, init?: RequestInit) => {
    state.calls.push({
      url: String(url),
      method: init?.method ?? "GET",
      body: typeof init?.body === "string" ? init.body : undefined,
      headers: Object.fromEntries(new Headers(init?.headers).entries()),
    });
    return new Response(JSON.stringify({ ok: true }), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  };
  return state;
}

/** Set up mock backend that returns a specific response. */
function mockBackendWithResponse(responseBody: unknown, status = 200) {
  const state = { calls: [] as { url: string; method: string; body?: string }[] };
  (globalThis as any).window = {
    __OMNILAUNCHER_BACKEND_URL__: "http://test.local",
    addEventListener: () => {},
    removeEventListener: () => {},
  };
  (globalThis as any).fetch = async (url: string, init?: RequestInit) => {
    state.calls.push({
      url: String(url),
      method: init?.method ?? "GET",
      body: typeof init?.body === "string" ? init.body : undefined,
    });
    return new Response(JSON.stringify(responseBody), {
      status,
      headers: { "Content-Type": "application/json" },
    });
  };
  return state;
}

/** Set up mock backend that rejects with a network error. */
function mockBackendNetworkError(errorMessage: string) {
  (globalThis as any).window = {
    __OMNILAUNCHER_BACKEND_URL__: "http://test.local",
    addEventListener: () => {},
    removeEventListener: () => {},
  };
  (globalThis as any).fetch = async () => {
    throw new Error(errorMessage);
  };
}

/** Set up mock backend that returns a non-2xx status. */
function mockBackendHttpError(status: number, body: string) {
  (globalThis as any).window = {
    __OMNILAUNCHER_BACKEND_URL__: "http://test.local",
    addEventListener: () => {},
    removeEventListener: () => {},
  };
  (globalThis as any).fetch = async () => {
    return new Response(body, { status, headers: { "Content-Type": "text/plain" } });
  };
}

function cleanupGlobals() {
  delete (globalThis as any).window;
  delete (globalThis as any).fetch;
}

// ===========================================================================
// 0. Frontend backend-token storage
// ===========================================================================

describe("frontend backend-token storage", () => {
  afterEach(cleanupGlobals);

  it("uses a frontend-local backend token from the Tauri shell before HTTP requests", async () => {
    const state = mockBackend();
    (globalThis as any).window.__TAURI_INTERNALS__ = {};
    vi.mocked(tauriInvoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_frontend_backend_token") return "frontend-file-token";
      return undefined;
    });

    await invoke("get_settings");

    const call = state.calls.find((c) => c.url.endsWith("/api/settings"));
    expect(call).toBeDefined();
    expect(call!.headers["x-omnilauncher-token"]).toBe("frontend-file-token");
    expect(tauriInvoke).toHaveBeenCalledWith("get_frontend_backend_token");
  });

  it("re-prompts and retries once when a saved frontend token is rejected", async () => {
    const calls: { url: string; method: string; headers: Record<string, string> }[] = [];
    (globalThis as any).window = {
      __OMNILAUNCHER_BACKEND_URL__: "http://test.local",
      __TAURI_INTERNALS__: {},
      prompt: vi.fn(() => "replacement-token"),
      addEventListener: () => {},
      removeEventListener: () => {},
    };
    vi.mocked(tauriInvoke).mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_frontend_backend_token") return "old-token";
      if (cmd === "save_frontend_backend_token") return args?.token;
      return undefined;
    });
    (globalThis as any).fetch = vi.fn(async (url: string, init?: RequestInit) => {
      calls.push({
        url: String(url),
        method: init?.method ?? "GET",
        headers: Object.fromEntries(new Headers(init?.headers).entries()),
      });
      if (calls.length === 1) {
        return new Response("missing or invalid auth token", { status: 401 });
      }
      return new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    });

    await invoke("get_settings");

    expect(calls.map((call) => call.headers["x-omnilauncher-token"])).toEqual([
      "old-token",
      "replacement-token",
    ]);
    expect((globalThis as any).window.prompt).toHaveBeenCalledTimes(1);
    expect(tauriInvoke).toHaveBeenCalledWith("save_frontend_backend_token", {
      token: "replacement-token",
    });
  });

  it("does not save local settings when the backend token prompt is cancelled", async () => {
    mockBackend();
    (globalThis as any).window.__OMNILAUNCHER_BACKEND_URL__ = "http://cancelled-token.local";
    (globalThis as any).window.__TAURI_INTERNALS__ = {};
    (globalThis as any).window.prompt = vi.fn(() => "");
    vi.mocked(tauriInvoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_frontend_backend_token") return "";
      return undefined;
    });

    await expect(
      invoke("save_settings_cmd", { settings: { hotkey: "Ctrl+Shift+P" } }),
    ).rejects.toThrow("Backend token is required");

    expect(tauriInvoke).not.toHaveBeenCalledWith("save_settings_cmd", {
      settings: { hotkey: "Ctrl+Shift+P" },
    });
  });
});

// ===========================================================================
// 1. Command classification
// ===========================================================================

describe("command classification", () => {
  it("treats window/geometry commands as local", () => {
    expect(isWindowLocalCommand("set_window_geometry")).toBe(true);
    expect(isWindowLocalCommand("set_window_size_centered")).toBe(true);
    expect(isWindowLocalCommand("save_window_position")).toBe(true);
    expect(isWindowLocalCommand("capture_vision_screenshot")).toBe(true);
  });

  it("treats business commands as not-local (go to backend)", () => {
    expect(isWindowLocalCommand("search")).toBe(false);
    expect(isWindowLocalCommand("ai_query")).toBe(false);
    expect(isWindowLocalCommand("list_skills")).toBe(false);
    expect(isWindowLocalCommand("install_plugin")).toBe(false);
    expect(isWindowLocalCommand("get_settings")).toBe(false);
  });

  it("treats unknown commands as not-local", () => {
    expect(isWindowLocalCommand("unknown_command")).toBe(false);
    expect(isWindowLocalCommand("")).toBe(false);
    expect(isWindowLocalCommand("SET_WINDOW_GEOMETRY")).toBe(false); // case-sensitive
  });
});

// ===========================================================================
// 2. Backend mode detection
// ===========================================================================

describe("backend mode detection", () => {
  afterEach(cleanupGlobals);

  it('returns "http" when backend URL is set', () => {
    (globalThis as any).window = {
      __OMNILAUNCHER_BACKEND_URL__: "http://test.local",
    };
    expect(getBackendMode()).toBe("http");
  });

  it('returns "mock" when no Tauri runtime and no backend URL', () => {
    // No window at all => mock mode
    expect(getBackendMode()).toBe("mock");
  });

  it('returns "mock" when window exists but no backend URL and no Tauri', () => {
    (globalThis as any).window = {};
    expect(getBackendMode()).toBe("mock");
  });
});

// ===========================================================================
// 3. HTTP routing for all command endpoints
// ===========================================================================

describe("http routing for new endpoints", () => {
  afterEach(cleanupGlobals);

  // --- GET endpoints ---

  it("maps list_skills to GET /api/skills", async () => {
    const state = mockBackend();
    await invoke("list_skills");
    expect(state.calls.some((c) => c.url.endsWith("/api/skills") && c.method === "GET")).toBe(true);
  });

  it("maps list_plugin_collections to GET /api/plugins/collections", async () => {
    const state = mockBackend();
    await invoke("list_plugin_collections");
    expect(state.calls.some((c) => c.url.endsWith("/api/plugins/collections"))).toBe(true);
  });

  it("maps get_settings to GET /api/settings", async () => {
    const state = mockBackend();
    await invoke("get_settings");
    expect(state.calls.some((c) => c.url.endsWith("/api/settings") && c.method === "GET")).toBe(true);
  });

  it("maps get_launcher_config to GET /api/launcher-config", async () => {
    const state = mockBackend();
    await invoke("get_launcher_config");
    expect(state.calls.some((c) => c.url.endsWith("/api/launcher-config"))).toBe(true);
  });

  it("maps list_favorites to GET /api/favorites", async () => {
    const state = mockBackend();
    await invoke("list_favorites");
    expect(state.calls.some((c) => c.url.endsWith("/api/favorites") && c.method === "GET")).toBe(true);
  });

  it("maps list_ai_sessions to GET /api/sessions", async () => {
    const state = mockBackend();
    await invoke("list_ai_sessions");
    expect(state.calls.some((c) => c.url.endsWith("/api/sessions"))).toBe(true);
  });

  it("maps current_ai_session to GET /api/sessions/current", async () => {
    const state = mockBackend();
    await invoke("current_ai_session");
    expect(state.calls.some((c) => c.url.endsWith("/api/sessions/current"))).toBe(true);
  });

  it("maps list_skill_usage to GET /api/skills/usage", async () => {
    const state = mockBackend();
    await invoke("list_skill_usage");
    expect(state.calls.some((c) => c.url.endsWith("/api/skills/usage"))).toBe(true);
  });

  it("maps list_plugin_runtime_dependencies to GET /api/plugins/runtime-deps", async () => {
    const state = mockBackend();
    await invoke("list_plugin_runtime_dependencies");
    expect(state.calls.some((c) => c.url.endsWith("/api/plugins/runtime-deps"))).toBe(true);
  });

  // --- POST endpoints ---

  it("maps slash_preview to POST /api/slash/preview", async () => {
    const state = mockBackend();
    await invoke("slash_preview", { query: "/calc 1+1" });
    expect(state.calls.some((c) => c.url.endsWith("/api/slash/preview") && c.method === "POST")).toBe(true);
  });

  it("maps search to POST /api/search with query body", async () => {
    const state = mockBackend();
    await invoke("search", { query: "notepad" });
    const call = state.calls.find((c) => c.url.endsWith("/api/search"));
    expect(call).toBeDefined();
    expect(call!.method).toBe("POST");
    expect(JSON.parse(call!.body!)).toEqual({ query: "notepad" });
  });

  it("maps ai_query to POST /api/ai/query with query body", async () => {
    const state = mockBackend();
    await invoke("ai_query", { query: "what is 2+2" });
    const call = state.calls.find((c) => c.url.endsWith("/api/ai/query"));
    expect(call).toBeDefined();
    expect(call!.method).toBe("POST");
    expect(JSON.parse(call!.body!)).toEqual({ query: "what is 2+2" });
  });

  it("maps ai_cancel to POST /api/ai/cancel", async () => {
    const state = mockBackend();
    await invoke("ai_cancel");
    expect(state.calls.some((c) => c.url.endsWith("/api/ai/cancel") && c.method === "POST")).toBe(true);
  });

  it("routes save_settings_cmd only to the backend in the desktop shell", async () => {
    const state = mockBackend();
    const settings = { ai_base_url: "http://example.com", ai_model: "gpt-4", ai_timeout_secs: 300 };
    (globalThis as any).window.__TAURI_INTERNALS__ = {};

    await invoke("save_settings_cmd", { settings });

    const call = state.calls.find((c) => c.url.endsWith("/api/settings") && c.method === "POST");
    expect(tauriInvoke).not.toHaveBeenCalledWith("save_settings_cmd", { settings });
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual(settings);
  });

  it("routes set_hotkey_cmd only to the backend in the desktop shell", async () => {
    const state = mockBackend();
    const settings = { hotkey: "Ctrl+Shift+O" };
    (globalThis as any).window.__TAURI_INTERNALS__ = {};
    (globalThis as any).window.__OMNILAUNCHER_TOKEN__ = "current-backend-token";

    await invoke("set_hotkey_cmd", { settings });

    const call = state.calls.find((c) => c.url.endsWith("/api/settings") && c.method === "POST");
    expect(tauriInvoke).not.toHaveBeenCalledWith("set_hotkey_cmd", { settings });
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual(settings);
  });

  it("maps save_settings_cmd to POST /api/settings with settings body outside Tauri", async () => {
    const state = mockBackend();
    const settings = { ai_base_url: "http://example.com", ai_model: "gpt-4", ai_timeout_secs: 300 };
    await invoke("save_settings_cmd", { settings });
    const call = state.calls.find((c) => c.url.endsWith("/api/settings") && c.method === "POST");
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual(settings);
  });

  it("routes save_settings_cmd to backend only in the desktop shell when a backend URL is configured", async () => {
    const state = mockBackend();
    const settings = { ai_base_url: "http://example.com", ai_model: "gpt-4", ai_timeout_secs: 300 };
    (globalThis as any).window.__TAURI_INTERNALS__ = {};

    await invoke("save_settings_cmd", { settings });

    const call = state.calls.find((c) => c.url.endsWith("/api/settings") && c.method === "POST");
    expect(tauriInvoke).not.toHaveBeenCalledWith("save_settings_cmd", { settings });
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual(settings);
  });

  it("routes save_settings_cmd to native Tauri when no backend URL is configured", async () => {
    const settings = { ai_base_url: "http://example.com", ai_model: "gpt-4", ai_timeout_secs: 300 };
    (globalThis as any).window = {
      __TAURI_INTERNALS__: {},
    };
    (globalThis as any).fetch = vi.fn(async () => {
      throw new Error("fetch should not be called for native settings saves");
    });

    await expect(invoke("save_settings_cmd", { settings })).resolves.toBeUndefined();

    expect(tauriInvoke).toHaveBeenCalledWith("save_settings_cmd", { settings });
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it("authenticates settings saves with the current backend token, not the edited token", async () => {
    const state = mockBackend();
    (globalThis as any).window.__OMNILAUNCHER_TOKEN__ = "current-backend-token";
    const settings = {
      ai_base_url: "http://example.com",
      ai_model: "gpt-4",
      ai_timeout_secs: 300,
    };
    await invoke("save_settings_cmd", { settings });
    const call = state.calls.find((c) => c.url.endsWith("/api/settings") && c.method === "POST");
    expect(call).toBeDefined();
    expect(call!.headers["x-omnilauncher-token"]).toBe("current-backend-token");
  });

  it("keeps the in-page backend token after saving settings", async () => {
    mockBackend();
    (globalThis as any).window.__OMNILAUNCHER_TOKEN__ = "current-backend-token";
    await invoke("save_settings_cmd", {
      settings: {
        ai_base_url: "http://example.com",
        ai_model: "gpt-4",
        ai_timeout_secs: 300,
      },
    });
    expect((globalThis as any).window.__OMNILAUNCHER_TOKEN__).toBe("current-backend-token");
  });

  it("keeps authenticating later requests with the current token after saving settings", async () => {
    const state = mockBackend();
    (globalThis as any).window.__OMNILAUNCHER_TOKEN__ = "current-backend-token";

    await invoke("save_settings_cmd", {
      settings: {
        ai_base_url: "http://example.com",
        ai_model: "gpt-4",
        ai_timeout_secs: 300,
      },
    });
    await invoke("get_settings");

    const settingsCalls = state.calls.filter((c) => c.url.endsWith("/api/settings"));
    expect(settingsCalls.map((c) => c.headers["x-omnilauncher-token"])).toEqual([
      "current-backend-token",
      "current-backend-token",
    ]);
  });

  it("maps list_models to POST /api/models with base_url and api_key", async () => {
    const state = mockBackend();
    await invoke("list_models", { baseUrl: "http://ai.local", apiKey: "sk-test" });
    const call = state.calls.find((c) => c.url.endsWith("/api/models"));
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual({ base_url: "http://ai.local", api_key: "sk-test" });
  });

  it("maps switch_ai_session to POST /api/sessions/switch", async () => {
    const state = mockBackend();
    await invoke("switch_ai_session", { sessionId: 42 });
    const call = state.calls.find((c) => c.url.endsWith("/api/sessions/switch"));
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual({ session_id: 42 });
  });

  it("maps delete_ai_session to POST /api/sessions/delete", async () => {
    const state = mockBackend();
    await invoke("delete_ai_session", { sessionId: 99 });
    const call = state.calls.find((c) => c.url.endsWith("/api/sessions/delete"));
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual({ session_id: 99 });
  });

  it("maps clear_conversation to POST /api/sessions/clear", async () => {
    const state = mockBackend();
    await invoke("clear_conversation");
    expect(state.calls.some((c) => c.url.endsWith("/api/sessions/clear") && c.method === "POST")).toBe(true);
  });

  it("maps execute_result to POST /api/execute-result", async () => {
    const state = mockBackend();
    const result = { id: "calc::1", action_type: "plugin_execute", action_data: "calc" };
    await invoke("execute_result", { result });
    const call = state.calls.find((c) => c.url.endsWith("/api/execute-result"));
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual({ result });
  });

  it("maps execute_slash_command to POST /api/slash/execute", async () => {
    const state = mockBackend();
    await invoke("execute_slash_command", { query: "/todo add milk" });
    const call = state.calls.find((c) => c.url.endsWith("/api/slash/execute"));
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual({ query: "/todo add milk" });
  });

  // --- DELETE endpoints ---

  it("maps remove_favorite to DELETE /api/favorites/:id", async () => {
    const state = mockBackend();
    await invoke("remove_favorite", { id: "fav-123" });
    const call = state.calls.find((c) => c.url.includes("/api/favorites/fav-123") && c.method === "DELETE");
    expect(call).toBeDefined();
  });

  // --- Skill management endpoints ---

  it("maps install_skill to POST /api/skills/install", async () => {
    const state = mockBackend();
    await invoke("install_skill", { source: "https://example.com/skill.git" });
    const call = state.calls.find((c) => c.url.endsWith("/api/skills/install"));
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual({ source: "https://example.com/skill.git" });
  });

  it("maps update_skill to POST /api/skills/update", async () => {
    const state = mockBackend();
    await invoke("update_skill", { name: "my-skill" });
    const call = state.calls.find((c) => c.url.endsWith("/api/skills/update"));
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual({ name: "my-skill" });
  });

  it("maps delete_skill to POST /api/skills/delete", async () => {
    const state = mockBackend();
    await invoke("delete_skill", { name: "old-skill" });
    const call = state.calls.find((c) => c.url.endsWith("/api/skills/delete"));
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual({ name: "old-skill" });
  });

  it("maps pin_skill to POST /api/skills/pin", async () => {
    const state = mockBackend();
    await invoke("pin_skill", { name: "my-skill", pinned: true });
    const call = state.calls.find((c) => c.url.endsWith("/api/skills/pin"));
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual({ name: "my-skill", pinned: true });
  });

  it("maps run_curator_now to POST /api/skills/curator/run", async () => {
    const state = mockBackend();
    await invoke("run_curator_now");
    expect(state.calls.some((c) => c.url.endsWith("/api/skills/curator/run") && c.method === "POST")).toBe(true);
  });

  it("maps propose_skill_consolidation to POST /api/skills/consolidation/propose", async () => {
    const state = mockBackend();
    await invoke("propose_skill_consolidation");
    expect(state.calls.some((c) => c.url.endsWith("/api/skills/consolidation/propose"))).toBe(true);
  });

  it("maps apply_skill_consolidation to POST /api/skills/consolidation/apply", async () => {
    const state = mockBackend();
    await invoke("apply_skill_consolidation", { proposal: { name: "merged" } });
    const call = state.calls.find((c) => c.url.endsWith("/api/skills/consolidation/apply"));
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual({ proposal: { name: "merged" } });
  });

  // --- Plugin management endpoints ---

  it("maps install_plugin to POST /api/plugins/install", async () => {
    const state = mockBackend();
    await invoke("install_plugin", { source: "https://github.com/user/plugin", targetDir: "/plugins" });
    const call = state.calls.find((c) => c.url.endsWith("/api/plugins/install"));
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual({ source: "https://github.com/user/plugin", target_dir: "/plugins" });
  });

  it("maps update_plugin to POST /api/plugins/update", async () => {
    const state = mockBackend();
    await invoke("update_plugin", { name: "my-plugin" });
    const call = state.calls.find((c) => c.url.endsWith("/api/plugins/update"));
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual({ name: "my-plugin" });
  });

  it("maps install_plugin_runtime_dependency to POST /api/plugins/runtime-deps/install", async () => {
    const state = mockBackend();
    await invoke("install_plugin_runtime_dependency", { id: "python" });
    const call = state.calls.find((c) => c.url.endsWith("/api/plugins/runtime-deps/install"));
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual({ id: "python" });
  });

  // --- Vision endpoint ---

  it("maps vision_analyze to POST /api/vision/analyze", async () => {
    const state = mockBackend();
    await invoke("vision_analyze", { prompt: "describe this", imageBase64: "abc123" });
    const call = state.calls.find((c) => c.url.endsWith("/api/vision/analyze"));
    expect(call).toBeDefined();
    expect(JSON.parse(call!.body!)).toEqual({ prompt: "describe this", image_base64: "abc123" });
  });
});

// ===========================================================================
// 4. Error handling
// ===========================================================================

describe("error handling", () => {
  afterEach(cleanupGlobals);

  it("throws on network failure", async () => {
    mockBackendNetworkError("Failed to fetch");
    await expect(invoke("search", { query: "test" })).rejects.toThrow("Failed to fetch");
  });

  it("throws on non-2xx HTTP response", async () => {
    mockBackendHttpError(500, "Internal Server Error");
    await expect(invoke("search", { query: "test" })).rejects.toThrow("Internal Server Error");
  });

  it("throws on 404 response", async () => {
    mockBackendHttpError(404, "Not Found");
    await expect(invoke("get_settings")).rejects.toThrow("Not Found");
  });

  it("throws on unknown command in HTTP mode", async () => {
    mockBackend();
    await expect(invoke("nonexistent_command")).rejects.toThrow(
      'Command "nonexistent_command" is not available in browser mode yet.'
    );
  });

  it("throws descriptive error on empty body with non-2xx status", async () => {
    mockBackendHttpError(503, "");
    await expect(invoke("search", { query: "test" })).rejects.toThrow("HTTP 503");
  });
});

// ===========================================================================
// 5. Default / missing arguments
// ===========================================================================

describe("default arguments handling", () => {
  afterEach(cleanupGlobals);

  it("search defaults to empty query when no args provided", async () => {
    const state = mockBackend();
    await invoke("search");
    const call = state.calls.find((c) => c.url.endsWith("/api/search"));
    expect(JSON.parse(call!.body!)).toEqual({ query: "" });
  });

  it("ai_query defaults to empty query when no args provided", async () => {
    const state = mockBackend();
    await invoke("ai_query");
    const call = state.calls.find((c) => c.url.endsWith("/api/ai/query"));
    expect(JSON.parse(call!.body!)).toEqual({ query: "" });
  });

  it("slash_preview defaults to empty query when no args provided", async () => {
    const state = mockBackend();
    await invoke("slash_preview");
    const call = state.calls.find((c) => c.url.endsWith("/api/slash/preview"));
    expect(JSON.parse(call!.body!)).toEqual({ query: "" });
  });

  it("remove_favorite encodes the id in the URL", async () => {
    const state = mockBackend();
    await invoke("remove_favorite", { id: "fav with spaces" });
    const call = state.calls.find((c) => c.method === "DELETE");
    expect(call!.url).toContain("fav%20with%20spaces");
  });

  it("remove_favorite handles missing id gracefully", async () => {
    const state = mockBackend();
    await invoke("remove_favorite", {});
    const call = state.calls.find((c) => c.method === "DELETE");
    expect(call).toBeDefined();
  });
});

// ===========================================================================
// 6. Mock / fallback mode (no backend, no Tauri)
// ===========================================================================

describe("mock mode fallbacks", () => {
  afterEach(cleanupGlobals);

  it("returns mock search results when no backend", async () => {
    // No window, no fetch => mock mode
    const results = await invoke<any[]>("search");
    expect(Array.isArray(results)).toBe(true);
    expect(results.length).toBeGreaterThan(0);
    expect(results[0]).toHaveProperty("title");
    expect(results[0]).toHaveProperty("id");
  });

  it("returns mock settings when no backend", async () => {
    const settings = await invoke<any>("get_settings");
    expect(settings).toHaveProperty("ai_model");
    expect(settings).toHaveProperty("theme");
    expect(settings).toHaveProperty("hotkey");
  });

  it("returns empty object for unknown commands in mock mode", async () => {
    const result = await invoke("anything_else");
    expect(result).toEqual({});
  });
});

// ===========================================================================
// 7. Window-local commands in HTTP mode
// ===========================================================================

describe("window-local commands in HTTP mode bypass HTTP", () => {
  afterEach(cleanupGlobals);

  it("resolves locally for geometry commands without Tauri runtime", async () => {
    // In HTTP mode, window-local commands resolve to true immediately
    (globalThis as any).window = {
      __OMNILAUNCHER_BACKEND_URL__: "http://test.local",
    };
    (globalThis as any).fetch = async () => {
      throw new Error("should not be called for window-local commands");
    };
    // Without Tauri, these go through the HTTP switch and return Promise.resolve(true)
    const result = await invoke("set_window_geometry");
    expect(result).toBe(true);
  });
});

describe("AI event streams in HTTP mode", () => {
  afterEach(cleanupGlobals);

  it("subscribes ai-error through backend SSE with the current token", async () => {
    let captured: { url: string; headers: Record<string, string> } | null = null;
    (globalThis as any).window = {
      __OMNILAUNCHER_BACKEND_URL__: "http://test.local",
      __OMNILAUNCHER_TOKEN__: "current-token",
      addEventListener: () => {},
      removeEventListener: () => {},
    };
    (globalThis as any).fetch = vi.fn(async (url: string, init?: RequestInit) => {
      captured = {
        url: String(url),
        headers: Object.fromEntries(new Headers(init?.headers).entries()),
      };
      return new Response(
        new ReadableStream<Uint8Array>({
          start(controller) {
            controller.close();
          },
        }),
        {
          status: 200,
          headers: { "Content-Type": "text/event-stream" },
        },
      );
    });

    const unlisten = await listen("omnilauncher://ai-error", () => {});
    unlisten();
    await vi.waitFor(() => expect(captured).not.toBeNull());

    expect(captured!.url).toBe(
      "http://test.local/api/events/omnilauncher%3A%2F%2Fai-error",
    );
    expect(captured!.headers.accept).toBe("text/event-stream");
    expect(captured!.headers["x-omnilauncher-token"]).toBe("current-token");
  });

  it("dispatches backend ai-done SSE payloads to listeners", async () => {
    let controller!: ReadableStreamDefaultController<Uint8Array>;
    (globalThis as any).window = {
      __OMNILAUNCHER_BACKEND_URL__: "http://test.local",
      addEventListener: () => {},
      removeEventListener: () => {},
    };
    (globalThis as any).fetch = vi.fn(async () => {
      const stream = new ReadableStream<Uint8Array>({
        start(c) {
          controller = c;
        },
      });
      return new Response(stream, {
        status: 200,
        headers: { "Content-Type": "text/event-stream" },
      });
    });

    const received: unknown[] = [];
    const unlisten = await listen("omnilauncher://ai-done", (event) => {
      received.push(event.payload);
    });

    await vi.waitFor(() => expect(controller).toBeDefined());
    controller.enqueue(
      new TextEncoder().encode(
        'data: {"content":"ok","tools_used":[],"results":[],"is_ai":true}\n\n',
      ),
    );

    await vi.waitFor(() =>
      expect(received).toEqual([
        { content: "ok", tools_used: [], results: [], is_ai: true },
      ]),
    );
    unlisten();
  });
});

// ===========================================================================
// 7b. Local Tauri events in HTTP mode
// ===========================================================================

describe("local Tauri events in HTTP mode", () => {
  afterEach(cleanupGlobals);

  it("listens for settings-saved on the local Tauri bus instead of opening backend SSE", async () => {
    (globalThis as any).window = {
      __TAURI_INTERNALS__: {},
      __OMNILAUNCHER_BACKEND_URL__: "http://test.local",
      addEventListener: () => {},
      removeEventListener: () => {},
    };
    const fetchSpy = vi.fn(async () => {
      throw new Error("settings-saved must not use backend SSE");
    });
    (globalThis as any).fetch = fetchSpy;

    const unlisten = await listen("omnilauncher://settings-saved", () => {});
    unlisten();

    expect(tauriListen).toHaveBeenCalledWith(
      "omnilauncher://settings-saved",
      expect.any(Function),
    );
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

// ===========================================================================
// 7c. Windows release shell configuration
// ===========================================================================

describe("Windows release shell configuration", () => {
  it("declares the Windows subsystem so the frontend does not open a console window", () => {
    expect(mainRsSource).toContain("cfg_attr(not(debug_assertions), windows_subsystem = \"windows\")");
  });
});

// ===========================================================================
// 8. Response data integrity
// ===========================================================================

describe("response data integrity", () => {
  afterEach(cleanupGlobals);

  it("returns parsed JSON response correctly", async () => {
    const mockData = { items: [{ id: 1, name: "test" }], total: 1 };
    mockBackendWithResponse(mockData);
    const result = await invoke<typeof mockData>("search", { query: "test" });
    expect(result).toEqual(mockData);
  });

  it("handles empty array response", async () => {
    mockBackendWithResponse([]);
    const result = await invoke<unknown[]>("search", { query: "nonexistent" });
    expect(result).toEqual([]);
  });

  it("handles boolean response", async () => {
    mockBackendWithResponse(true);
    const result = await invoke<boolean>("ai_cancel");
    expect(result).toBe(true);
  });
});
