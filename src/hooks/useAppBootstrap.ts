import { useEffect } from "react";
import {
  loadLauncherConfig,
} from "../launcherConfig";
import {
  invoke,
  listen,
  getBackendMode,
  getBackendUrl,
} from "../lib/runtime";
import { logger } from "../lib/logger";
import { parseThemeMode, type ThemeMode } from "../utils/theme";
import type { AppSettings } from "../types/app";

export interface UseAppBootstrapArgs {
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
  setBackgroundUrl: React.Dispatch<React.SetStateAction<string>>;
  setTheme: (t: ThemeMode) => void;
  setShowSettings: React.Dispatch<React.SetStateAction<boolean>>;
}

/**
 * One-time bootstrap effects: log the startup banner, load persisted settings,
 * and listen for settings updates from the standalone settings window.
 */
export function useAppBootstrap(args: UseAppBootstrapArgs): void {
  const { setSettings, setBackgroundUrl, setTheme } = args;

  // Startup banner — one line on initial mount so the devtools console
  // and ~/.omnilauncher/omnilauncher.log (via tauri frontend_log) both
  // show backend wiring at a glance.
  useEffect(() => {
    const mode = getBackendMode();
    const url = mode === "http" ? getBackendUrl() : "(tauri ipc)";
    const dev = !!(import.meta as any).env?.DEV;
    logger.info(
      `OmniLauncher UI mounted backend=${url} mode=${mode} dev=${dev}`,
    );
    // Run once on mount; deps intentionally empty.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Listen for settings changes from the standalone settings window
  useEffect(() => {
    const unlisten = listen<AppSettings>(
      "omnilauncher://settings-saved",
      (e) => {
        setTheme(parseThemeMode(e.payload.theme));
        setBackgroundUrl(e.payload.background_url ?? "");
        setSettings(e.payload);
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
