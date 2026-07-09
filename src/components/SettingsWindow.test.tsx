/**
 * SettingsWindow regression tests — guards against the "Preferences silently
 * wipes user settings" bug.
 *
 * Old behavior: when `invoke("get_settings")` rejected (e.g. cross-machine
 * 401 auth failure between Windows frontend and WSL backend), the catch
 * branch substituted a hardcoded factory-default AppSettings object and
 * rendered the form. The user then either:
 *   (a) saw "wrong" settings (everything reset, no API key, etc.) and assumed
 *       the page was broken; OR
 *   (b) clicked Save anywhere on the form, sending those defaults back to the
 *       backend, which silently overwrote settings.json on disk.
 *
 * New behavior verified here:
 *  - On load failure, the form is NOT rendered.
 *  - The user sees an explicit error message and a Retry button.
 *  - There is NO Save button reachable from the error state (so a
 *    misinterpretation cannot accidentally POST defaults).
 *  - The actual error message text from the backend is shown.
 */
import React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import type { AppSettings } from "../types/app";

type RuntimeMock<Return> = (...args: unknown[]) => Promise<Return>;

const invokeMock = vi.fn<RuntimeMock<unknown>>();
const emitMock = vi.fn<RuntimeMock<void>>(async () => {});
const listenMock = vi.fn<RuntimeMock<() => void>>(async () => () => {});

vi.mock("../lib/runtime", () => ({
  invoke: <T,>(...args: unknown[]): Promise<T> => invokeMock(...args) as Promise<T>,
  emit: (...args: unknown[]) => emitMock(...args),
  listen: (...args: unknown[]) => listenMock(...args),
}));

const realSettings: AppSettings = {
  ai_base_url: "http://wsl-backend:5000",
  ai_model: "gpt-5.4",
  ai_api_key: "sk-real-from-wsl",
  ai_timeout_secs: 240,
  ai_max_tool_iterations: 50,
  ai_max_retry_attempts: 7,
  ai_retry_base_delay_ms: 1500,
  ai_loop_detector_enabled: true,
  theme: "dark",
  hotkey: "Ctrl+Shift+O",
  max_results: 25,
  background_url: "https://example.com/bg.png",
  backend_url: "http://wsl-backend:1422",
  a2a_enabled: false,
  a2a_bind_lan: false,
  a2a_port: 1423,
  a2a_token: null,
  a2a_public_url: "",
  a2a_hub_url: "",
  a2a_hub_admin_key: "",
  a2a_hub_upstream_name: "omnilauncher",
  a2a_hub_prefix: "@omnilauncher",
  a2a_hub_auto_register: false,
};

beforeEach(() => {
  invokeMock.mockReset();
  emitMock.mockClear();
  listenMock.mockClear();
});

async function importSettingsWindow() {
  const mod = await import("./SettingsWindow");
  return mod.default;
}

describe("SettingsWindow load-failure handling", () => {
  it("renders real settings when get_settings succeeds (sanity baseline)", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return realSettings;
      if (cmd === "list_models") return ["gpt-5.4", "gpt-4o"];
      return undefined;
    });

    const SettingsWindow = await importSettingsWindow();
    render(<SettingsWindow />);

    // The Save Settings button is only present when settings loaded.
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /save settings/i })).toBeInTheDocument(),
    );
    // And the real (non-default) provider URL is displayed.
    expect(screen.getByDisplayValue("http://wsl-backend:5000")).toBeInTheDocument();
  });

  it("does NOT render the Save button when get_settings fails", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") {
        throw new Error("HTTP 401: missing or invalid auth token");
      }
      return undefined;
    });

    const SettingsWindow = await importSettingsWindow();
    render(<SettingsWindow />);

    // Wait for the load to settle.
    await waitFor(() =>
      expect(
        screen.getByText(/could not load settings from the backend/i),
      ).toBeInTheDocument(),
    );

    // No Save button — this is the critical guarantee: the user cannot
    // accidentally trigger a save-of-defaults from the error state.
    expect(
      screen.queryByRole("button", { name: /save settings/i }),
    ).not.toBeInTheDocument();
  });

  it("displays the actual backend error message", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") {
        throw new Error("HTTP 401: missing or invalid auth token");
      }
      return undefined;
    });

    const SettingsWindow = await importSettingsWindow();
    render(<SettingsWindow />);

    await waitFor(() =>
      expect(
        screen.getByText(/missing or invalid auth token/i),
      ).toBeInTheDocument(),
    );
  });

  it("exposes a Retry button that re-runs get_settings", async () => {
    invokeMock.mockImplementationOnce(async () => {
      throw new Error("HTTP 401");
    });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return realSettings;
      if (cmd === "list_models") return ["gpt-5.4"];
      return undefined;
    });

    const SettingsWindow = await importSettingsWindow();
    render(<SettingsWindow />);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: /retry/i })).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole("button", { name: /retry/i }));

    // After successful retry, the form renders with real values.
    await waitFor(() =>
      expect(screen.getByDisplayValue("http://wsl-backend:5000")).toBeInTheDocument(),
    );
    expect(
      screen.getByRole("button", { name: /save settings/i }),
    ).toBeInTheDocument();
  });

  it("never invokes save_settings_cmd when loading fails", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") {
        throw new Error("HTTP 401");
      }
      return undefined;
    });

    const SettingsWindow = await importSettingsWindow();
    render(<SettingsWindow />);

    await waitFor(() =>
      expect(
        screen.getByText(/could not load settings from the backend/i),
      ).toBeInTheDocument(),
    );

    // Examine every invoke call: none should have been save_settings_cmd.
    const calls = invokeMock.mock.calls.map(([cmd]) => cmd as string);
    expect(calls).not.toContain("save_settings_cmd");
    expect(calls).not.toContain("set_hotkey_cmd");
  });
});
