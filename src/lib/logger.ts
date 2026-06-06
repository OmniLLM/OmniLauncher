// Level-gated frontend logger. Mirrors backend log levels and forwards to the
// Tauri `frontend_log` command when running inside the desktop shell so the
// two logs interleave in ~/.omnilauncher/omnilauncher.log.
//
// Active level (highest priority first):
//   1. URL query param ?log=trace|debug|info|warn|error
//   2. localStorage.OMNI_LOG_LEVEL
//   3. import.meta.env.DEV  → "debug"
//   4. fallback              → "info"
//
// Below-threshold calls are no-ops: no console output, no Tauri invoke.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";

export type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

const ORDER: Record<LogLevel, number> = {
  trace: 0,
  debug: 1,
  info: 2,
  warn: 3,
  error: 4,
};

const isTauriRuntime = () =>
  typeof window !== "undefined" && !!(window as any).__TAURI_INTERNALS__;

function readUrlLevel(): LogLevel | null {
  if (typeof window === "undefined") return null;
  try {
    const params = new URLSearchParams(window.location.search);
    const raw = params.get("log");
    if (raw && raw in ORDER) return raw as LogLevel;
  } catch {
    /* ignore */
  }
  return null;
}

function readStorageLevel(): LogLevel | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.localStorage?.getItem("OMNI_LOG_LEVEL");
    if (raw && raw in ORDER) return raw as LogLevel;
  } catch {
    /* ignore */
  }
  return null;
}

function defaultLevel(): LogLevel {
  // Vite injects import.meta.env.DEV at build time.
  try {
    if ((import.meta as any).env?.DEV) return "debug";
  } catch {
    /* ignore */
  }
  return "info";
}

let active: LogLevel = readUrlLevel() ?? readStorageLevel() ?? defaultLevel();

export function getLevel(): LogLevel {
  return active;
}

export function setLevel(level: LogLevel): void {
  active = level;
  try {
    window.localStorage?.setItem("OMNI_LOG_LEVEL", level);
  } catch {
    /* ignore */
  }
}

function enabled(level: LogLevel): boolean {
  return ORDER[level] >= ORDER[active];
}

function emit(level: LogLevel, message: string): void {
  if (!enabled(level)) return;
  const line = `[omni ${level}] ${message}`;
  const fn =
    level === "error"
      ? console.error
      : level === "warn"
        ? console.warn
        : console.log;
  fn(line);

  if (!isTauriRuntime()) return;
  tauriInvoke("frontend_log", { level, message: line }).catch(() => {
    // Logging must never break app behavior.
  });
}

export function summarizeArgs(args?: Record<string, unknown>): string {
  if (!args) return "none";
  try {
    return JSON.stringify(args, (key, value) => {
      const lower = key.toLowerCase();
      if (
        lower.includes("key") ||
        lower.includes("token") ||
        lower.includes("secret")
      ) {
        return value ? "[redacted]" : value;
      }
      if (typeof value === "string" && value.length > 160) {
        return `${value.slice(0, 160)}...(${value.length} chars)`;
      }
      return value;
    });
  } catch {
    return "[unserializable]";
  }
}

export const logger = {
  trace: (msg: string) => emit("trace", msg),
  debug: (msg: string) => emit("debug", msg),
  info: (msg: string) => emit("info", msg),
  warn: (msg: string) => emit("warn", msg),
  error: (msg: string) => emit("error", msg),
  getLevel,
  setLevel,
};
