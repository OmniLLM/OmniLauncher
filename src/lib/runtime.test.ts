import { describe, it, expect, afterEach } from "vitest";
import { isWindowLocalCommand, invoke } from "./runtime";

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
});

describe("http routing for new endpoints", () => {
  afterEach(() => {
    delete (globalThis as any).window;
    delete (globalThis as any).fetch;
  });

  function mockBackend(): string[] {
    const calls: string[] = [];
    (globalThis as any).window = {
      __OMNILAUNCHER_BACKEND_URL__: "http://test.local",
    };
    (globalThis as any).fetch = async (url: string) => {
      calls.push(String(url));
      return new Response(JSON.stringify([]), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    };
    return calls;
  }

  it("maps list_skills to GET /api/skills", async () => {
    const calls = mockBackend();
    await invoke("list_skills");
    expect(calls.some((u) => u.endsWith("/api/skills"))).toBe(true);
  });

  it("maps list_plugin_collections to GET /api/plugins/collections", async () => {
    const calls = mockBackend();
    await invoke("list_plugin_collections");
    expect(calls.some((u) => u.endsWith("/api/plugins/collections"))).toBe(true);
  });

  it("maps slash_preview to POST /api/slash/preview", async () => {
    const calls = mockBackend();
    await invoke("slash_preview", { query: "/calc 1+1" });
    expect(calls.some((u) => u.endsWith("/api/slash/preview"))).toBe(true);
  });
});

