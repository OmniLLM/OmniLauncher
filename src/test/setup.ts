/**
 * Vitest setup — runs once per worker before any test file.
 *
 * Responsibilities:
 *  1. Register @testing-library/jest-dom's custom matchers (toBeInTheDocument,
 *     toHaveAttribute, etc.) on Vitest's `expect`.
 *  2. Mock Tauri's IPC surface (@tauri-apps/api). jsdom doesn't run inside a
 *     Tauri window, so invoke/listen would otherwise throw or hang. Tests
 *     that need specific backend responses override `invoke` per-test via
 *     `vi.mocked(invoke).mockResolvedValueOnce(...)`.
 *  3. Provide DOM polyfills jsdom is missing (matchMedia, scrollIntoView)
 *     that several launcher components touch on mount.
 *  4. Reset module state between tests so localStorage / mock counters don't
 *     leak across cases.
 */
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";

// ---------------------------------------------------------------------------
// Tauri API mocks
// ---------------------------------------------------------------------------
//
// We mock the three subpaths the app imports from. Each export is a vi.fn()
// so individual tests can assert on call args or override return values.

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => undefined),
  convertFileSrc: vi.fn((p: string) => p),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
  once: vi.fn(async () => () => {}),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: vi.fn(() => ({
    show: vi.fn(async () => {}),
    hide: vi.fn(async () => {}),
    setFocus: vi.fn(async () => {}),
    listen: vi.fn(async () => () => {}),
    onFocusChanged: vi.fn(async () => () => {}),
  })),
}));

// ---------------------------------------------------------------------------
// jsdom polyfills
// ---------------------------------------------------------------------------

if (typeof window !== "undefined") {
  // Some Node/Vitest launches expose a partial localStorage object. Install the
  // Storage methods the app uses so tests see normal browser behavior.
  const localStorageData = new Map<string, string>();
  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: {
      getItem: vi.fn((key: string) => localStorageData.get(key) ?? null),
      setItem: vi.fn((key: string, value: string) => {
        localStorageData.set(key, String(value));
      }),
      removeItem: vi.fn((key: string) => {
        localStorageData.delete(key);
      }),
      clear: vi.fn(() => {
        localStorageData.clear();
      }),
    },
  });
}

// Several components call window.matchMedia for theme detection. jsdom
// doesn't implement it; fake a permanent "doesn't match" response.
if (typeof window !== "undefined" && !window.matchMedia) {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(() => false),
    }),
  });
}

// ResultList calls scrollIntoView on the active row to keep it in view.
// jsdom's HTMLElement doesn't have it; stub a no-op.
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = function () {};
}

// ---------------------------------------------------------------------------
// Per-test isolation
// ---------------------------------------------------------------------------

beforeEach(() => {
  // Don't let one test's localStorage state seep into the next.
  if (typeof window !== "undefined") {
    window.localStorage.clear();
  }
});

afterEach(() => {
  // Unmount any rendered React trees so testing-library queries don't see
  // stale DOM from a previous test.
  cleanup();
  // Reset call history on every mocked function (mock implementations
  // declared via vi.mock above are preserved).
  vi.clearAllMocks();
});
