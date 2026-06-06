import { useCallback, useEffect, useState } from "react";
import { invoke } from "../lib/runtime";
import {
  type ResolvedTheme,
  type ThemeMode,
  getSystemTheme,
} from "../utils/theme";
import type { AppSettings } from "../types/app";

export interface UseThemeResult {
  theme: ThemeMode;
  setTheme: (t: ThemeMode) => void;
  systemTheme: ResolvedTheme;
  resolvedTheme: ResolvedTheme;
  handleThemeToggle: () => Promise<void>;
}

export function useTheme(): UseThemeResult {
  const [theme, setTheme] = useState<ThemeMode>("system");
  const [systemTheme, setSystemTheme] =
    useState<ResolvedTheme>(getSystemTheme());

  const resolvedTheme: ResolvedTheme = theme === "system" ? systemTheme : theme;

  const handleThemeToggle = useCallback(async () => {
    const next: ThemeMode = resolvedTheme === "dark" ? "light" : "dark";
    setTheme(next);
    // Persist to backend settings so it survives restarts
    try {
      const current = await invoke<AppSettings>("get_settings");
      await invoke("save_settings_cmd", {
        settings: { ...current, theme: next },
      });
    } catch {
      // non-fatal — the in-memory theme change already happened
    }
  }, [resolvedTheme]);

  useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = (event: MediaQueryListEvent) => {
      setSystemTheme(event.matches ? "dark" : "light");
    };

    setSystemTheme(media.matches ? "dark" : "light");
    media.addEventListener("change", handleChange);
    return () => media.removeEventListener("change", handleChange);
  }, []);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", resolvedTheme);
  }, [resolvedTheme]);

  return { theme, setTheme, systemTheme, resolvedTheme, handleThemeToggle };
}
