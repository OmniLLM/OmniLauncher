/**
 * App-level integration tests — "human-like" front-end smoke.
 *
 * These tests mount the full <App /> tree and drive it the way a real user
 * would: typing into the input, pressing arrow keys, hitting Enter. They
 * verify that the launcher's wiring (App → hooks → lib/runtime → invoke)
 * survives end-to-end, not just that individual components render.
 *
 * Mock surface: we mock `lib/runtime` (the app's IPC wrapper). The mocked
 * `invoke` is a router that returns realistic shapes per command name, so
 * tests can assert on observable behavior without spinning up a Tauri
 * window or backend server.
 *
 * What we DON'T test here (covered elsewhere):
 *  - Individual component logic → ResultList.test.tsx etc.
 *  - AI tool-calling round-trip → Rust integration tests
 *    (src-tauri/tests/ai_tool_call_simulation_tests.rs) — that path lives
 *    entirely in the backend; the front-end only sees opaque "ai-done" payloads.
 */
import React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, within, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type {
  AiResponse,
  AppSettings,
  ConversationTurn,
  QueryResult,
} from "./types/app";

// ---------------------------------------------------------------------------
// Mock the IPC layer. hooks/ all import { invoke, listen } from "./lib/runtime",
// so mocking that module short-circuits every backend call in the app.
// ---------------------------------------------------------------------------

type Handler<T> = (event: { payload: T }) => void;

// Holds active listeners keyed by event name so tests can fire backend
// events from the outside (e.g. simulate "omnilauncher://ai-done").
const listeners = new Map<string, Set<Handler<unknown>>>();

function emit<T>(eventName: string, payload: T) {
  const set = listeners.get(eventName);
  if (!set) return;
  set.forEach((h) => h({ payload: payload as unknown }));
}

// In-memory mock state — reset in beforeEach.
let favorites: QueryResult[] = [];
let searchResults: QueryResult[] = [];
let nextAiResponse: AiResponse = {
  content: "default mock answer",
  tools_used: [],
  results: [],
  is_ai: true,
};
const invokeCalls: Array<{ cmd: string; args?: Record<string, unknown> }> = [];
let failNextSettingsSave = false;

vi.mock("./lib/runtime", () => ({
  invoke: vi.fn(
    async <T,>(
      cmd: string,
      args?: Record<string, unknown>,
    ): Promise<T> => {
      invokeCalls.push({ cmd, args });
      switch (cmd) {
        case "get_settings":
          return {
            ai_base_url: "",
            ai_model: "",
            ai_api_key: "",
            ai_timeout_secs: 120,
            ai_max_tool_iterations: 10,
            theme: "system",
            hotkey: "Cmd+Space",
            max_results: 30,
            background_url: "",
            backend_url: "",
            a2a_enabled: false,
            a2a_bind_lan: false,
            a2a_port: 1423,
            a2a_token: null,
          } as AppSettings as T;
        case "search":
          return searchResults as T;
        case "execute_result":
          return undefined as T;
        case "list_favorites":
          return favorites as T;
        case "add_favorite": {
          const r = args?.result as QueryResult;
          if (r && !favorites.find((f) => f.id === r.id)) favorites.push(r);
          return undefined as T;
        }
        case "remove_favorite": {
          const id = args?.id as string;
          favorites = favorites.filter((f) => f.id !== id);
          return undefined as T;
        }
        case "list_ai_sessions":
          return [] as T;
        case "current_ai_session":
          return 0 as T;
        case "clear_conversation":
          return undefined as T;
        case "delete_ai_session":
          return 0 as T;
        case "ai_query": {
          // Real backend fires events asynchronously; mirror that here so
          // useAiQuery's listener subscription wins the race.
          setTimeout(() => {
            emit("omnilauncher://ai-done", nextAiResponse);
          }, 0);
          return undefined as T;
        }
        case "ai_cancel":
          return undefined as T;
        case "set_hotkey_cmd":
          if (failNextSettingsSave) {
            failNextSettingsSave = false;
            throw new Error("mock save failure");
          }
          return args?.settings as AppSettings as T;
        case "save_settings_cmd":
          if (failNextSettingsSave) {
            failNextSettingsSave = false;
            throw new Error("mock save failure");
          }
          return true as T;
        case "save_window_position":
        case "set_window_geometry":
        case "set_window_size_centered":
          return undefined as T;
        default:
          return undefined as T;
      }
    },
  ),
  emit: vi.fn(async <T,>(eventName: string, payload: T): Promise<void> => {
    emit(eventName, payload);
  }),
  listen: vi.fn(
    async <T,>(
      eventName: string,
      handler: Handler<T>,
    ): Promise<() => void> => {
      let set = listeners.get(eventName);
      if (!set) {
        set = new Set();
        listeners.set(eventName, set);
      }
      set.add(handler as Handler<unknown>);
      return () => {
        set?.delete(handler as Handler<unknown>);
      };
    },
  ),
  getCurrentWebviewWindow: () => ({
    // Mirror the shape from lib/runtime.ts's non-Tauri fallback. onMoved /
    // onFocusChanged are async-returning-unlisten so the hooks don't choke.
    onFocusChanged: async (
      _handler: (event: { payload: boolean }) => void,
    ) => {
      return () => {};
    },
    onMoved: async (
      _handler: (event: { payload: { x: number; y: number } }) => void,
    ) => {
      return () => {};
    },
    hide: vi.fn(async () => {}),
    show: vi.fn(async () => {}),
    setFocus: vi.fn(async () => {}),
  }),
  getBackendMode: () => "mock" as const,
  getBackendUrl: () => "",
  isWindowLocalCommand: () => false,
}));

// Has to import AFTER vi.mock so the module sees the mock.
import App from "./App";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const mkResult = (
  over: Partial<QueryResult> & { id: string; title: string },
): QueryResult => ({
  subtitle: "",
  icon: "📄",
  score: 1,
  action_type: "open",
  action_data: "",
  source: "files",
  ...over,
});

beforeEach(() => {
  listeners.clear();
  favorites = [];
  searchResults = [];
  invokeCalls.length = 0;
  failNextSettingsSave = false;
  nextAiResponse = {
    content: "default mock answer",
    tools_used: [],
    results: [],
    is_ai: true,
  };
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Find the launcher input by placeholder. The input has no explicit role,
 *  so we match by the placeholder string declared in SearchBar.tsx. */
async function findInput(): Promise<HTMLInputElement> {
  return (await screen.findByPlaceholderText(
    /Type to launch|Ask AI/i,
  )) as HTMLInputElement;
}

/** Wait until invoke was called with the given command. */
async function waitForInvoke(cmd: string, timeoutMs = 2000): Promise<void> {
  await waitFor(
    () => {
      const found = invokeCalls.some((c) => c.cmd === cmd);
      expect(found, `expected invoke(${cmd})`).toBe(true);
    },
    { timeout: timeoutMs },
  );
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

describe("App — bootstrap", () => {
  it("renders the search input on mount", async () => {
    render(<App />);
    const input = await findInput();
    expect(input).toBeInTheDocument();
    expect(input).toHaveValue("");
  });

  it("loads settings + favorites on first paint", async () => {
    favorites = [mkResult({ id: "fav-1", title: "Pinned thing" })];
    render(<App />);
    await waitForInvoke("get_settings");
    await waitForInvoke("list_favorites");
  });

  it("shows favorites under the ★ Favorites header when query is empty", async () => {
    favorites = [
      mkResult({ id: "fav-1", title: "Pinned thing" }),
      mkResult({ id: "fav-2", title: "Other pin" }),
    ];
    render(<App />);
    await waitForInvoke("list_favorites");
    // Wait for the favorites section to render.
    await screen.findByText("★ Favorites");
    // Scope to the listbox — the footer actionbar also echoes the selected
    // title, which would multi-match a bare getByText.
    const listbox = within(screen.getByRole("listbox"));
    expect(listbox.getByText("Pinned thing")).toBeInTheDocument();
    expect(listbox.getByText("Other pin")).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

describe("App — settings", () => {
  it("shows a saved confirmation after saving settings", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Open settings" }));
    await screen.findByText("Preferences");
    await user.click(screen.getByRole("button", { name: "Save Settings" }));

    const savedLabels = await screen.findAllByText("✓ Saved");
    expect(savedLabels.length).toBeGreaterThan(0);
    expect(screen.getByText("Preferences")).toBeInTheDocument();
  });

  it("saves an adjustable AI tool iteration limit", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Open settings" }));
    await screen.findByText("Preferences");

    const toolIterations = screen.getByTitle(
      "Maximum AI tool-call iterations before stopping",
    );
    fireEvent.change(toolIterations, { target: { value: "25" } });
    await user.click(screen.getByRole("button", { name: "Save Settings" }));

    await screen.findAllByText("✓ Saved");
    expect(invokeCalls[invokeCalls.length - 1]).toMatchObject({
      cmd: "save_settings_cmd",
      args: {
        settings: expect.objectContaining({ ai_max_tool_iterations: 25 }),
      },
    });
  });

  it("does not show or save a backend token in settings", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Open settings" }));
    await screen.findByText("Preferences");

    expect(screen.queryByText("Backend Token")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save Settings" }));

    await screen.findAllByText("✓ Saved");
    const saveCall = invokeCalls.find((call) => call.cmd === "save_settings_cmd");
    expect(saveCall).toBeDefined();
    expect(saveCall!.args?.settings).not.toHaveProperty("backend_token");
  });

  it("toggles settings with Ctrl+,", async () => {
    render(<App />);

    expect(await findInput()).toBeInTheDocument();

    fireEvent.keyDown(window, { key: ",", ctrlKey: true });
    await screen.findByText("Preferences");

    fireEvent.keyDown(window, { key: ",", ctrlKey: true });
    await waitFor(() => {
      expect(screen.queryByText("Preferences")).not.toBeInTheDocument();
    });
    expect(await findInput()).toBeInTheDocument();
  });

  it("captures a new hotkey and saves it immediately", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Open settings" }));
    await user.click(screen.getByRole("button", { name: "General" }));

    const hotkeyButton = await screen.findByRole("button", { name: "Edit hotkey" });
    expect(hotkeyButton).toHaveTextContent("Cmd+Space");

    await user.click(hotkeyButton);
    expect(hotkeyButton).toHaveTextContent("Press new hotkey…");

    fireEvent.keyDown(hotkeyButton, {
      key: "k",
      code: "KeyK",
      ctrlKey: true,
      shiftKey: true,
    });

    await screen.findByText("✓ Hotkey saved");
    expect(hotkeyButton).toHaveTextContent("Ctrl+Shift+K");
    expect(invokeCalls[invokeCalls.length - 1]).toMatchObject({
      cmd: "set_hotkey_cmd",
      args: {
        settings: expect.objectContaining({ hotkey: "Ctrl+Shift+K" }),
      },
    });
  });

  it("keeps the previous hotkey when immediate registration fails", async () => {
    failNextSettingsSave = true;
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Open settings" }));
    await user.click(screen.getByRole("button", { name: "General" }));

    const hotkeyButton = await screen.findByRole("button", { name: "Edit hotkey" });
    await user.click(hotkeyButton);
    fireEvent.keyDown(hotkeyButton, { key: " ", code: "Space", altKey: true });

    expect(await screen.findByText(/mock save failure/)).toBeInTheDocument();
    expect(hotkeyButton).toHaveTextContent("Cmd+Space");
  });
});

describe("App — search flow (human typing)", () => {
  it("types into the input character-by-character and triggers a debounced search", async () => {
    searchResults = [
      mkResult({ id: "r1", title: "= 2+2 = 4", source: "calculator" }),
    ];
    const user = userEvent.setup();
    render(<App />);
    const input = await findInput();

    // Type the calculator query the way a user would.
    await user.type(input, "= 2+2");
    expect(input).toHaveValue("= 2+2");

    // Search is debounced (~150ms); wait for it to fire.
    await waitForInvoke("search");
    // The last invoke("search", { query }) should reflect the final input.
    const searchCalls = invokeCalls.filter((c) => c.cmd === "search");
    expect(searchCalls[searchCalls.length - 1].args).toEqual({ query: "= 2+2" });

    // Result should make it into the listbox.
    await screen.findByText("= 2+2 = 4");
  });

  it("ArrowDown then Enter executes the selected result", async () => {
    searchResults = [
      mkResult({ id: "a", title: "alpha.md" }),
      mkResult({ id: "b", title: "beta.md" }),
      mkResult({ id: "c", title: "gamma.md" }),
    ];
    const user = userEvent.setup();
    render(<App />);
    const input = await findInput();

    await user.type(input, "md");
    // Highlight wraps the matched substring in <mark>, splitting the title
    // across text nodes. Wait via a relaxed matcher that ignores tag splits.
    await waitFor(() => {
      const titles = Array.from(
        document.querySelectorAll(".result-item__title"),
      ).map((el) => el.textContent);
      expect(titles).toEqual(
        expect.arrayContaining(["alpha.md", "beta.md", "gamma.md"]),
      );
    });

    // ArrowDown moves selection 0 -> 1, Enter fires execute_result on item 1.
    await user.keyboard("{ArrowDown}");
    await user.keyboard("{Enter}");

    await waitForInvoke("execute_result");
    const execCall = invokeCalls.find((c) => c.cmd === "execute_result");
    expect(execCall?.args?.result).toMatchObject({ id: "b", title: "beta.md" });
  });

  it("Ctrl+1 directly executes the first result without arrow keys", async () => {
    searchResults = [
      mkResult({ id: "a", title: "alpha.md" }),
      mkResult({ id: "b", title: "beta.md" }),
    ];
    const user = userEvent.setup();
    render(<App />);
    const input = await findInput();

    await user.type(input, "md");
    await waitFor(() => {
      const titles = Array.from(
        document.querySelectorAll(".result-item__title"),
      ).map((el) => el.textContent);
      expect(titles).toContain("alpha.md");
    });

    // Ctrl+1 is the cross-platform first-result shortcut.
    await user.keyboard("{Control>}1{/Control}");

    await waitForInvoke("execute_result");
    const execCall = invokeCalls.find((c) => c.cmd === "execute_result");
    expect(execCall?.args?.result).toMatchObject({ id: "a" });
  });

  it("Esc clears the query when results are present", async () => {
    searchResults = [mkResult({ id: "a", title: "alpha.md" })];
    const user = userEvent.setup();
    render(<App />);
    const input = await findInput();

    await user.type(input, "md");
    await waitFor(() => {
      const titles = Array.from(
        document.querySelectorAll(".result-item__title"),
      ).map((el) => el.textContent);
      expect(titles).toContain("alpha.md");
    });

    await user.keyboard("{Escape}");
    // Either the query clears or the window hides — useGlobalKeyboard hands
    // Escape several roles. We only assert the visible one: the input empties.
    await waitFor(() => expect(input).toHaveValue(""));
  });
});

// ---------------------------------------------------------------------------
// AI query flow — full round trip through events
// ---------------------------------------------------------------------------

describe("App — AI query flow", () => {
  it("? prefix toggles AI mode, typed query routes to ai_query and renders the answer", async () => {
    nextAiResponse = {
      content: "The capital of France is Paris.",
      tools_used: [],
      results: [],
      is_ai: true,
    };
    const user = userEvent.setup();
    render(<App />);
    const input = await findInput();

    // Real UX: typing "?" alone is intercepted by useSearch as an AI-mode
    // toggle — the launcher swaps the <input> for a <textarea> (different
    // node) and clears the value. We must re-query the live input after
    // the toggle, otherwise userEvent types into a detached node.
    await user.type(input, "?");
    const textarea = (await screen.findByPlaceholderText(
      /Ask AI/i,
    )) as HTMLTextAreaElement;
    expect(textarea.tagName).toBe("TEXTAREA");
    await user.type(textarea, "capital of france");
    expect(textarea).toHaveValue("capital of france");
    await user.keyboard("{Enter}");

    await waitForInvoke("ai_query");
    const aiCall = invokeCalls.find((c) => c.cmd === "ai_query");
    expect(aiCall?.args?.query).toBe("capital of france");

    // The mock fires omnilauncher://ai-done with nextAiResponse — the chat
    // body should pick it up.
    await screen.findByText(/capital of France is Paris/i);
  });

  it("renders tool-call breadcrumbs as ai-tool-call events arrive mid-flight", async () => {
    // Slow response so we can interleave tool-call events.
    nextAiResponse = {
      content: "Looked it up.",
      tools_used: ["file_read", "grep"],
      results: [],
      is_ai: true,
    };
    const user = userEvent.setup();
    render(<App />);
    const input = await findInput();

    await user.type(input, "?find the readme");
    await user.keyboard("{Enter}");

    await waitForInvoke("ai_query");

    // Simulate backend emitting tool-call progress events. They should appear
    // in the conversation history as "Used tool: …" turns.
    emit("omnilauncher://ai-tool-call", { tool: "file_read", iteration: 1 });
    emit("omnilauncher://ai-tool-call", { tool: "grep", iteration: 2 });

    // After both tools, the ai-done arrives (already queued from the mock).
    await screen.findByText(/Looked it up/i);
  });
});

// ---------------------------------------------------------------------------
// Favorites toggle from the result list
// ---------------------------------------------------------------------------

describe("App — favorites toggle", () => {
  it("clicking the star on a search result calls add_favorite", async () => {
    searchResults = [mkResult({ id: "x", title: "x.md" })];
    const user = userEvent.setup();
    render(<App />);
    const input = await findInput();

    await user.type(input, "x");
    await screen.findByText("x.md");

    // Wait until the favorites have loaded so the star buttons exist.
    await waitForInvoke("list_favorites");
    const stars = await screen.findAllByRole("button", { name: /favorite/i });
    await user.click(stars[0]);

    await waitForInvoke("add_favorite");
    const call = invokeCalls.find((c) => c.cmd === "add_favorite");
    expect(call?.args?.result).toMatchObject({ id: "x", title: "x.md" });
  });
});
