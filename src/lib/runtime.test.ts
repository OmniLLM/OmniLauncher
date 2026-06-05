import { describe, it, expect } from "vitest";
import { isWindowLocalCommand } from "./runtime";

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
