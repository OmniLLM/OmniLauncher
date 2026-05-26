import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface PluginInfo {
  name: string;
  description: string;
  version: string;
  keyword?: string;
  icon?: string;
  entry: string;
  dir_name: string;
}

interface AppSettings {
  plugin_dirs: string[];
  [key: string]: unknown;
}

interface PluginManagerProps {
  colors: {
    bg: string;
    surface: string;
    surface2: string;
    text: string;
    accent: string;
    accentDim: string;
    sub: string;
  };
  onClose: () => void;
}

const DEFAULT_DIR = "~/.omnilauncher/plugins (default)";

export default function PluginManager({ colors, onClose }: PluginManagerProps) {
  const [plugins, setPlugins] = useState<PluginInfo[]>([]);
  const [source, setSource] = useState("");
  const [targetDir, setTargetDir] = useState<string>(""); // "" = default
  const [extraDirs, setExtraDirs] = useState<string[]>([]);
  const [status, setStatus] = useState<{
    type: "idle" | "loading" | "success" | "error";
    message: string;
  }>({ type: "idle", message: "" });

  const refresh = useCallback(() => {
    invoke<PluginInfo[]>("list_plugins")
      .then((list) => setPlugins(list))
      .catch(() => setPlugins([]));
  }, []);

  useEffect(() => {
    refresh();
    // Load extra plugin_dirs from settings
    invoke<AppSettings>("get_settings")
      .then((s) => setExtraDirs(s.plugin_dirs ?? []))
      .catch(() => {});
  }, [refresh]);

  const handleInstall = async () => {
    const trimmed = source.trim();
    if (!trimmed) return;
    setStatus({ type: "loading", message: "Installing…" });
    try {
      const message = await invoke<string>("install_plugin", {
        source: trimmed,
        targetDir: targetDir || null,
      });
      setStatus({ type: "success", message: `✓ ${message}` });
      setSource("");
      refresh();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    }
  };

  const handleRemove = async (dirName: string) => {
    setStatus({ type: "loading", message: `Removing "${dirName}"…` });
    try {
      await invoke("remove_plugin", { name: dirName });
      setStatus({ type: "success", message: `✓ Removed "${dirName}"` });
      refresh();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    }
  };

  const statusColor =
    status.type === "success"
      ? "#a6e3a1"
      : status.type === "error"
        ? "#f38ba8"
        : colors.sub;

  const selectStyle: React.CSSProperties = {
    background: colors.surface,
    border: `1px solid ${colors.surface2}`,
    borderRadius: "8px",
    padding: "7px 10px",
    color: extraDirs.length === 0 ? colors.sub : colors.text,
    fontSize: "12px",
    outline: "none",
    cursor: "pointer",
    flexShrink: 0,
    maxWidth: "180px",
  };

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        padding: "14px 16px 10px",
        gap: "12px",
        overflowY: "auto",
        scrollbarWidth: "thin",
        scrollbarColor: `${colors.surface2} transparent`,
      }}
    >
      {/* Header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <span
          style={{
            fontSize: "13px",
            fontWeight: 700,
            color: colors.accent,
            letterSpacing: "0.04em",
            display: "flex",
            alignItems: "center",
            gap: "6px",
          }}
        >
          🔌 Plugin Manager
        </span>
        <button
          onClick={onClose}
          style={{
            background: "none",
            border: "none",
            cursor: "pointer",
            color: colors.sub,
            fontSize: "16px",
            lineHeight: 1,
            padding: "2px 4px",
          }}
          title="Close"
        >
          ×
        </button>
      </div>

      {/* Install row */}
      <div style={{ display: "flex", gap: "8px", flexWrap: "wrap" }}>
        <input
          type="text"
          value={source}
          onChange={(e) => setSource(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleInstall()}
          placeholder="Git URL or local path…"
          style={{
            flex: 1,
            minWidth: "180px",
            background: colors.surface,
            border: `1px solid ${colors.surface2}`,
            borderRadius: "8px",
            padding: "7px 12px",
            color: colors.text,
            fontSize: "13px",
            outline: "none",
          }}
        />
        {/* Install-to selector — only shown when there are extra dirs */}
        {extraDirs.length > 0 && (
          <select
            value={targetDir}
            onChange={(e) => setTargetDir(e.target.value)}
            style={selectStyle}
            title="Install into…"
          >
            <option value="">{DEFAULT_DIR}</option>
            {extraDirs.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </select>
        )}
        <button
          onClick={handleInstall}
          disabled={status.type === "loading" || !source.trim()}
          style={{
            background: colors.accent,
            border: "none",
            borderRadius: "8px",
            padding: "7px 14px",
            color: "#111214",
            fontSize: "13px",
            fontWeight: 600,
            cursor: source.trim() ? "pointer" : "default",
            opacity: source.trim() ? 1 : 0.5,
            transition: "opacity 150ms",
          }}
        >
          Install
        </button>
      </div>

      {/* Status message */}
      {status.type !== "idle" && (
        <div
          style={{
            fontSize: "12px",
            color: statusColor,
            minHeight: "18px",
          }}
        >
          {status.message}
        </div>
      )}

      {/* Plugin list */}
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: "6px",
        }}
      >
        {plugins.length === 0 ? (
          <div
            style={{
              fontSize: "13px",
              color: colors.sub,
              textAlign: "center",
              padding: "18px 0",
            }}
          >
            No external plugins installed yet.
            <br />
            <span style={{ fontSize: "12px", opacity: 0.7 }}>
              Paste a Git URL or local path above to install one.
            </span>
          </div>
        ) : (
          plugins.map((p) => (
            <div
              key={p.dir_name}
              style={{
                display: "flex",
                alignItems: "center",
                gap: "10px",
                background: colors.surface,
                borderRadius: "9px",
                padding: "9px 12px",
              }}
            >
              {/* Icon */}
              <span style={{ fontSize: "20px", flexShrink: 0 }}>
                {p.icon ?? "🔌"}
              </span>

              {/* Info */}
              <div style={{ flex: 1, minWidth: 0 }}>
                <div
                  style={{
                    fontSize: "13px",
                    fontWeight: 600,
                    color: colors.text,
                    display: "flex",
                    alignItems: "center",
                    gap: "6px",
                  }}
                >
                  {p.name}
                  <span
                    style={{
                      fontSize: "11px",
                      color: colors.sub,
                      fontWeight: 400,
                    }}
                  >
                    v{p.version}
                  </span>
                  {p.keyword && (
                    <span
                      style={{
                        fontSize: "11px",
                        background: `${colors.accent}22`,
                        border: `1px solid ${colors.accent}44`,
                        borderRadius: "5px",
                        padding: "1px 6px",
                        color: colors.accent,
                      }}
                    >
                      {p.keyword}
                    </span>
                  )}
                </div>
                <div
                  style={{
                    fontSize: "12px",
                    color: colors.sub,
                    marginTop: "2px",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {p.description}
                </div>
              </div>

              {/* Remove button */}
              <button
                onClick={() => handleRemove(p.dir_name)}
                title="Remove plugin"
                style={{
                  background: "none",
                  border: `1px solid ${colors.surface2}`,
                  borderRadius: "6px",
                  padding: "4px 10px",
                  color: colors.sub,
                  fontSize: "12px",
                  cursor: "pointer",
                  flexShrink: 0,
                  transition: "color 150ms, border-color 150ms",
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.color = "#f38ba8";
                  e.currentTarget.style.borderColor = "#f38ba8";
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.color = colors.sub;
                  e.currentTarget.style.borderColor = colors.surface2;
                }}
              >
                Remove
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
