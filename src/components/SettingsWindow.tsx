import { useState, useEffect, useRef, useCallback } from "react";
import { invoke, emit } from "../lib/runtime";
import type { AppSettings } from "../types/app";

const BG_PRESETS = [
  { label: "None (solid color)", value: "" },
  {
    label: "Overwatch — White Rabbit",
    value:
      "https://blz-contentstack-images.akamaized.net/v3/assets/bltf408a0557f4e4998/blt27903959c912debc/69fba009d002ee6d7deb5875/shop_carousel_ow_26_s2_mythicskin_desktop.webp?imwidth=1568&imdensity=1",
  },
  {
    label: "World of Warcraft",
    value:
      "https://blz-contentstack-images.akamaized.net/v3/assets/bltf408a0557f4e4998/bltf37ef22c44e74da0/69839a28b521c44554739254/WoW_Shop_HearthsteelHousingVCSKUs_BnetShop_ProductAssetGallery_1920x1080.png?imwidth=1088&imdensity=1",
  },
  {
    label: "Diablo IV",
    value:
      "https://blz-contentstack-images.akamaized.net/v3/assets/bltf408a0557f4e4998/blt524d75eb1bde1557/6920dd20a4d899a8d8ea5985/DIA_DIV_Helix_Bnet_Product_Page_Banners_Bnet_UE_Desktop-1600x500_GG01.png?imwidth=1568&imdensity=1",
  },
  {
    label: "Hearthstone",
    value:
      "https://blz-contentstack-images.akamaized.net/v3/assets/bltf408a0557f4e4998/bltd34bcafef5da9778/69cc0c9401bc870008d78112/HS_35p2_BGPremiumPass_BattleNet_Shop_Browser_DesktopBanner_1600x500_DB02.png?imwidth=1568&imdensity=1",
  },
  { label: "Custom URL…", value: "__custom__" },
];

const TABS = [
  { id: "ai", label: "AI", icon: "🤖" },
  { id: "appearance", label: "Appearance", icon: "🎨" },
  { id: "general", label: "General", icon: "⚙️" },
  { id: "a2a", label: "A2A", icon: "🔗" },
] as const;

type TabId = (typeof TABS)[number]["id"];
type SaveStatus = "idle" | "success" | "error";

const MODIFIER_KEYS = new Set([
  "Alt",
  "AltGraph",
  "Control",
  "Meta",
  "Shift",
]);

function formatHotkeyEvent(
  event: Pick<
    KeyboardEvent,
    "altKey" | "ctrlKey" | "key" | "metaKey" | "shiftKey"
  >,
): string | null {
  if (!event.key || MODIFIER_KEYS.has(event.key)) {
    return null;
  }

  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.shiftKey) parts.push("Shift");
  if (event.altKey) parts.push("Alt");
  if (event.metaKey) parts.push("Cmd");

  const key =
    event.key === " "
      ? "Space"
      : event.key.length === 1
        ? event.key.toUpperCase()
        : event.key;
  parts.push(key);
  return parts.join("+");
}

interface Props {
  onClose?: () => void;
}

export default function SettingsWindow({ onClose }: Props = {}) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");
  const [hotkeyStatus, setHotkeyStatus] = useState<SaveStatus>("idle");
  const [hotkeyError, setHotkeyError] = useState("");
  const [capturingHotkey, setCapturingHotkey] = useState(false);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState("");
  const [activeTab, setActiveTab] = useState<TabId>("ai");
  const [loadAttempt, setLoadAttempt] = useState(0);

  const currentBgUrl = settings?.background_url ?? "";
  const isCustomBg =
    currentBgUrl !== "" &&
    !BG_PRESETS.some(
      (p) => p.value === currentBgUrl && p.value !== "__custom__",
    );
  const bgSelectValue = isCustomBg ? "__custom__" : currentBgUrl;

  const [models, setModels] = useState<string[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [modelsError, setModelsError] = useState("");
  const [modelFilter, setModelFilter] = useState("");
  const [showModelDropdown, setShowModelDropdown] = useState(false);
  const modelInputRef = useRef<HTMLInputElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setLoading(true);
    setLoadError("");
    setSettings(null);
    invoke<AppSettings>("get_settings")
      .then((s) => {
        setSettings(s);
        setModelFilter(s.ai_model);
        setLoading(false);
      })
      .catch((err) => {
        // CRITICAL: Do NOT silently substitute hardcoded defaults here.
        //
        // Historically this catch swapped in `AppSettings.default()`-shaped
        // hardcoded values, which had two terrible effects:
        //
        //   1. Cross-machine connections (WSL backend ↔ Windows frontend) where
        //      the auth token is missing/stale → get_settings returns 401 →
        //      user sees factory defaults in Preferences instead of the
        //      backend's real settings, and assumes "settings page is broken".
        //
        //   2. If the user then clicks Save, the frontend POSTs those defaults
        //      back to the backend, which writes them to settings.json —
        //      silently wiping the user's actual saved settings.
        //
        // Surface the failure honestly. Keep `settings` null so the form
        // doesn't render (preventing a destructive save), and show the real
        // error with a retry affordance.
        const message =
          err instanceof Error ? err.message : String(err ?? "Unknown error");
        console.error("Failed to load settings from backend:", err);
        setLoadError(message || "Failed to load settings from backend");
        setLoading(false);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loadAttempt]);

  const fetchModels = useCallback(async () => {
    if (!settings) return;
    setModelsLoading(true);
    setModelsError("");
    try {
      const result = await invoke<string[]>("list_models", {
        baseUrl: settings.ai_base_url,
        apiKey: settings.ai_api_key,
      });
      setModels(result.sort());
    } catch (e) {
      setModelsError(String(e));
      setModels([]);
    } finally {
      setModelsLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings?.ai_base_url, settings?.ai_api_key]);

  useEffect(() => {
    if (settings?.ai_base_url) {
      fetchModels();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings?.ai_base_url, settings?.ai_api_key]);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(e.target as Node)
      ) {
        setShowModelDropdown(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  const filteredModels = models.filter((m) =>
    m.toLowerCase().includes(modelFilter.toLowerCase()),
  );

  const handleModelSelect = (model: string) => {
    setModelFilter(model);
    setSettings((s) => s && { ...s, ai_model: model });
    setShowModelDropdown(false);
  };

  const handleSave = async () => {
    if (!settings) return;
    setSaveStatus("idle");
    try {
      await invoke("save_settings_cmd", { settings });
      setSaveStatus("success");
      setTimeout(() => setSaveStatus("idle"), 2000);
      emit("omnilauncher://settings-saved", settings).catch(() => {});
    } catch (e) {
      console.error("Save error:", e);
      setSaveStatus("error");
    }
  };

  const handleHotkeyCapture = async (event: React.KeyboardEvent) => {
    if (!settings || !capturingHotkey) return;
    event.preventDefault();
    event.stopPropagation();

    if (event.key === "Escape") {
      setCapturingHotkey(false);
      setHotkeyError("");
      setHotkeyStatus("idle");
      return;
    }

    const hotkey = formatHotkeyEvent(event.nativeEvent);
    if (!hotkey) return;

    const previousHotkey = settings.hotkey;
    const nextSettings = { ...settings, hotkey };
    setSettings(nextSettings);
    setCapturingHotkey(false);
    setHotkeyStatus("idle");
    setHotkeyError("");

    try {
      const savedSettings = await invoke<AppSettings>("set_hotkey_cmd", {
        settings: nextSettings,
      });
      setSettings(savedSettings);
      setHotkeyStatus("success");
      emit("omnilauncher://settings-saved", savedSettings).catch(() => {});
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      console.error("Hotkey save error:", e);
      setSettings((s) => (s ? { ...s, hotkey: previousHotkey } : s));
      setHotkeyError(message || "Failed to register hotkey");
      setHotkeyStatus("error");
    }
  };

  if (loading) {
    return (
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: "100%",
          background: "transparent",
          color: "var(--sub)",
          fontFamily: "inherit",
          fontSize: 13,
        }}
      >
        Loading settings…
      </div>
    );
  }

  if (loadError || !settings) {
    // Show the failure honestly instead of silently rendering hardcoded
    // defaults — see the long-form comment on the get_settings catch above
    // for why this matters (cross-machine deploys, accidental save-of-defaults).
    return (
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          height: "100%",
          background: "transparent",
          color: "var(--text)",
          fontFamily: "inherit",
          overflow: "hidden",
        }}
      >
        <div data-tauri-drag-region className="omni-titlebar">
          <span className="omni-titlebar__title">
            <span aria-hidden="true">⚙</span>
            <span>Preferences</span>
          </span>
          <button
            className="omni-titlebar__close"
            onClick={() => onClose?.()}
            title="Close"
            aria-label="Close"
          >
            ✕
          </button>
        </div>
        <div
          style={{
            flex: 1,
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            gap: 16,
            padding: "32px 28px",
            textAlign: "center",
          }}
        >
          <div style={{ fontSize: 32 }} aria-hidden="true">
            ⚠
          </div>
          <div style={{ fontSize: 15, color: "var(--text)", fontWeight: 600 }}>
            Could not load settings from the backend.
          </div>
          <div
            style={{
              fontSize: 13,
              color: "var(--sub)",
              maxWidth: 480,
              wordBreak: "break-word",
              fontFamily:
                "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
              background: "var(--bg-elevated)",
              border: "1px solid var(--border)",
              borderRadius: 6,
              padding: "10px 12px",
            }}
          >
            {loadError || "No settings returned by the backend."}
          </div>
          <div style={{ fontSize: 12, color: "var(--sub)", maxWidth: 480 }}>
            The Save button is disabled until settings load successfully so a
            partial form can't overwrite your saved configuration.
          </div>
          <button
            type="button"
            className="omni-btn omni-btn--primary"
            onClick={() => setLoadAttempt((n) => n + 1)}
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  const rowStyle = (last = false): React.CSSProperties => ({
    display: "grid",
    gridTemplateColumns: "120px 1fr",
    alignItems: "center",
    gap: 12,
    padding: "10px 16px",
    borderBottom: last ? "none" : "1px solid var(--border)",
  });

  const rowLabelStyle: React.CSSProperties = {
    fontSize: 13,
    color: "var(--sub)",
  };

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        background: "transparent",
        color: "var(--text)",
        fontFamily: "inherit",
        overflow: "hidden",
      }}
    >
      <div data-tauri-drag-region className="omni-titlebar">
        <span className="omni-titlebar__title">
          <span aria-hidden="true">⚙</span>
          <span>Preferences</span>
        </span>
        <button
          className="omni-titlebar__close"
          onClick={() => onClose?.()}
          title="Close"
          aria-label="Close"
        >
          ✕
        </button>
      </div>

      {/* Body: sidebar + content */}
      <div style={{ display: "flex", flex: 1, overflow: "hidden" }}>
        {/* Left sidebar */}
        <div
          style={{
            width: 140,
            flexShrink: 0,
            background: "var(--bg-elevated)",
            borderRight: "1px solid var(--border)",
            padding: "12px 8px",
            display: "flex",
            flexDirection: "column",
            gap: 2,
          }}
        >
          {TABS.map((tab) => {
            const isActive = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                type="button"
                className={`settings-tab${isActive ? " settings-tab--active" : ""}`}
                onClick={() => setActiveTab(tab.id)}
                aria-pressed={isActive}
              >
                <span aria-hidden="true">{tab.icon}</span>
                <span>{tab.label}</span>
              </button>
            );
          })}
        </div>

        {/* Right content pane */}
        <div
          style={{
            flex: 1,
            display: "flex",
            flexDirection: "column",
            overflow: "hidden",
          }}
        >
          <div style={{ flex: 1, overflowY: "auto", padding: "24px 28px" }}>
            {activeTab === "ai" && (
              <div>
                <div className="settings-section-header">AI Provider</div>
                <div className="settings-card">
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Provider URL</span>
                    <input
                      className="omni-input"
                      value={settings.ai_base_url}
                      onChange={(e) =>
                        setSettings(
                          (s) => s && { ...s, ai_base_url: e.target.value },
                        )
                      }
                    />
                  </div>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>API Key</span>
                    <input
                      className="omni-input"
                      type="password"
                      value={settings.ai_api_key}
                      onChange={(e) =>
                        setSettings(
                          (s) => s && { ...s, ai_api_key: e.target.value },
                        )
                      }
                      placeholder="(optional)"
                    />
                  </div>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Timeout</span>
                    <input
                      className="omni-input"
                      type="number"
                      min={1}
                      max={3600}
                      value={settings.ai_timeout_secs}
                      onChange={(e) =>
                        setSettings(
                          (s) =>
                            s && {
                              ...s,
                              ai_timeout_secs: parseInt(e.target.value) || 120,
                            },
                        )
                      }
                      title="AI request timeout in seconds"
                    />
                  </div>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Tool Iterations</span>
                    <input
                      className="omni-input"
                      type="number"
                      min={1}
                      max={100}
                      value={settings.ai_max_tool_iterations}
                      onChange={(e) =>
                        setSettings(
                          (s) =>
                            s && {
                              ...s,
                              ai_max_tool_iterations:
                                parseInt(e.target.value) || 10,
                            },
                        )
                      }
                      title="Maximum AI tool-call iterations before stopping"
                    />
                  </div>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Retry Attempts</span>
                    <input
                      className="omni-input"
                      type="number"
                      min={1}
                      max={10}
                      value={settings.ai_max_retry_attempts}
                      onChange={(e) =>
                        setSettings(
                          (s) =>
                            s && {
                              ...s,
                              ai_max_retry_attempts:
                                parseInt(e.target.value) || 3,
                            },
                        )
                      }
                      title="How many times the AI client tries a transient-error request before giving up (1 = no retries)"
                    />
                  </div>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Retry Base Delay (ms)</span>
                    <input
                      className="omni-input"
                      type="number"
                      min={0}
                      max={60000}
                      step={100}
                      value={settings.ai_retry_base_delay_ms}
                      onChange={(e) =>
                        setSettings(
                          (s) =>
                            s && {
                              ...s,
                              ai_retry_base_delay_ms:
                                parseInt(e.target.value) || 2000,
                            },
                        )
                      }
                      title="Base backoff delay before the first retry; doubles on each subsequent retry plus jitter"
                    />
                  </div>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Loop Detector</span>
                    <label
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 8,
                        cursor: "pointer",
                      }}
                      title="When on (default), the AI tool loop halts after three identical (request, result) iterations in a row. Disable only when debugging long multi-step skills — Tool Iterations is still the upper bound."
                    >
                      <input
                        type="checkbox"
                        checked={settings.ai_loop_detector_enabled}
                        onChange={(e) =>
                          setSettings(
                            (s) =>
                              s && {
                                ...s,
                                ai_loop_detector_enabled: e.target.checked,
                              },
                          )
                        }
                      />
                      <span>Enable AI loop detector</span>
                    </label>
                  </div>
                  <div
                    ref={dropdownRef}
                    style={{ ...rowStyle(true), position: "relative" }}
                  >
                    <span style={rowLabelStyle}>
                      Model
                      {modelsLoading && (
                        <span style={{ color: "var(--accent)" }}>
                          {" "}
                          (loading…)
                        </span>
                      )}
                      {modelsError && (
                        <span
                          style={{ color: "var(--error)" }}
                          title={modelsError}
                        >
                          {" "}
                          ⚠
                        </span>
                      )}
                    </span>
                    <div style={{ position: "relative", width: "100%" }}>
                      <input
                        ref={modelInputRef}
                        className="omni-input"
                        value={modelFilter}
                        onChange={(e) => {
                          setModelFilter(e.target.value);
                          setSettings(
                            (s) => s && { ...s, ai_model: e.target.value },
                          );
                          setShowModelDropdown(true);
                        }}
                        onFocus={() => setShowModelDropdown(true)}
                        placeholder="Type to filter models…"
                      />
                      {showModelDropdown && filteredModels.length > 0 && (
                        <div className="settings-popover">
                          {filteredModels.map((m) => {
                            const isSel = m === settings.ai_model;
                            return (
                              <div
                                key={m}
                                onClick={() => handleModelSelect(m)}
                                className={`settings-popover__item${isSel ? " settings-popover__item--active" : ""}`}
                              >
                                {m}
                              </div>
                            );
                          })}
                        </div>
                      )}
                      {showModelDropdown &&
                        !modelsLoading &&
                        filteredModels.length === 0 &&
                        models.length > 0 && (
                          <div className="settings-popover">
                            <div className="settings-popover__empty">
                              No matches
                            </div>
                          </div>
                        )}
                    </div>
                  </div>
                </div>
              </div>
            )}

            {activeTab === "appearance" && (
              <div>
                <div className="settings-section-header">Appearance</div>
                <div className="settings-card">
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Theme</span>
                    <select
                      className="omni-select"
                      style={{ cursor: "pointer" }}
                      value={settings.theme}
                      onChange={(e) =>
                        setSettings((s) => s && { ...s, theme: e.target.value })
                      }
                    >
                      <option value="system">System (Follow OS)</option>
                      <option value="dark">Dark (Battle Blue)</option>
                      <option value="light">Light (Catppuccin Latte)</option>
                    </select>
                  </div>
                  <div
                    style={rowStyle(
                      !isCustomBg && bgSelectValue !== "__custom__",
                    )}
                  >
                    <span style={rowLabelStyle}>Background</span>
                    <select
                      className="omni-select"
                      style={{ cursor: "pointer" }}
                      value={bgSelectValue}
                      onChange={(e) => {
                        const val = e.target.value;
                        if (val !== "__custom__") {
                          setSettings(
                            (s) => s && { ...s, background_url: val },
                          );
                        }
                      }}
                    >
                      {BG_PRESETS.map((p) => (
                        <option key={p.label} value={p.value}>
                          {p.label}
                        </option>
                      ))}
                    </select>
                  </div>
                  {(bgSelectValue === "__custom__" || isCustomBg) && (
                    <div style={rowStyle(true)}>
                      <span style={rowLabelStyle}>Image URL</span>
                      <input
                        className="omni-input"
                        value={currentBgUrl}
                        onChange={(e) =>
                          setSettings(
                            (s) =>
                              s && { ...s, background_url: e.target.value },
                          )
                        }
                        placeholder="https://example.com/image.jpg"
                      />
                    </div>
                  )}
                </div>
              </div>
            )}

            {activeTab === "general" && (
              <div>
                <div className="settings-section-header">General</div>
                <div className="settings-card">
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Hotkey</span>
                    <button
                      type="button"
                      className="omni-input"
                      onClick={() => {
                        setCapturingHotkey(true);
                        setHotkeyError("");
                        setHotkeyStatus("idle");
                      }}
                      onBlur={() => setCapturingHotkey(false)}
                      onKeyDown={handleHotkeyCapture}
                      style={{
                        color: capturingHotkey ? "var(--accent)" : "var(--text)",
                        cursor: "pointer",
                        textAlign: "left",
                        userSelect: "none",
                      }}
                      aria-label="Edit hotkey"
                    >
                      {capturingHotkey ? "Press new hotkey…" : settings.hotkey}
                    </button>
                    {hotkeyStatus === "success" && (
                      <span
                        className="omni-status omni-status--success"
                        style={{ gridColumn: "2" }}
                      >
                        ✓ Hotkey saved
                      </span>
                    )}
                    {hotkeyStatus === "error" && (
                      <span
                        className="omni-status omni-status--error"
                        style={{ gridColumn: "2" }}
                        title={hotkeyError}
                      >
                        ✗ {hotkeyError || "Hotkey failed"}
                      </span>
                    )}
                  </div>
                  <div style={rowStyle(true)}>
                    <span style={rowLabelStyle}>Max Results</span>
                    <input
                      className="omni-input"
                      type="number"
                      min={1}
                      max={50}
                      value={settings.max_results}
                      onChange={(e) =>
                        setSettings(
                          (s) =>
                            s && {
                              ...s,
                              max_results: parseInt(e.target.value) || 10,
                            },
                        )
                      }
                    />
                  </div>
                  <div style={rowStyle(true)}>
                    <span style={rowLabelStyle}>Backend URL</span>
                    <input
                      className="omni-input"
                      type="text"
                      placeholder="http://127.0.0.1:1422 (default)"
                      value={settings.backend_url}
                      onChange={(e) =>
                        setSettings(
                          (s) => s && { ...s, backend_url: e.target.value },
                        )
                      }
                    />
                  </div>
                </div>
              </div>
            )}

            {activeTab === "a2a" && (
              <div>
                <div className="settings-section-header">A2A Server</div>
                <div className="settings-card">
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Enable A2A Server</span>
                    <label style={{ display: "flex", alignItems: "center", gap: 6 }}>
                      <input
                        type="checkbox"
                        checked={settings.a2a_enabled}
                        onChange={(e) =>
                          setSettings(
                            (s) =>
                              s && { ...s, a2a_enabled: e.target.checked },
                          )
                        }
                      />
                      <span>Start an A2A-compatible HTTP server for agent-to-agent communication</span>
                    </label>
                  </div>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>LAN Access</span>
                    <label style={{ display: "flex", alignItems: "center", gap: 6 }}>
                      <input
                        type="checkbox"
                        checked={settings.a2a_bind_lan}
                        onChange={(e) =>
                          setSettings(
                            (s) =>
                              s && { ...s, a2a_bind_lan: e.target.checked },
                          )
                        }
                      />
                      <span style={{ color: settings.a2a_bind_lan ? "var(--error)" : undefined }}>
                        {settings.a2a_bind_lan
                          ? "Listening on all interfaces (0.0.0.0) — accessible from LAN"
                          : "Local only (127.0.0.1)"}
                      </span>
                    </label>
                  </div>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Port</span>
                    <input
                      className="omni-input"
                      type="number"
                      min={1024}
                      max={65535}
                      value={settings.a2a_port}
                      onChange={(e) =>
                        setSettings(
                          (s) =>
                            s && {
                              ...s,
                              a2a_port: parseInt(e.target.value) || 1423,
                            },
                        )
                      }
                    />
                  </div>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Token</span>
                    <div style={{ display: "flex", gap: 6, alignItems: "center", width: "100%" }}>
                      <input
                        className="omni-input"
                        type="text"
                        readOnly
                        value={settings.a2a_token ?? "(not generated — enable and save to generate)"}
                        style={{ flex: 1, fontFamily: "monospace", fontSize: 12 }}
                      />
                      <button
                        type="button"
                        className="omni-button"
                        style={{ whiteSpace: "nowrap" }}
                        disabled={!settings.a2a_token}
                        onClick={() => {
                          if (settings.a2a_token) {
                            navigator.clipboard.writeText(settings.a2a_token);
                          }
                        }}
                      >
                        Copy
                      </button>
                    </div>
                  </div>
                  <div style={rowStyle(false)}>
                    <span style={rowLabelStyle}>Server URL</span>
                    <input
                      className="omni-input"
                      type="text"
                      readOnly
                      value={`http://${settings.a2a_bind_lan ? "0.0.0.0" : "127.0.0.1"}:${settings.a2a_port}`}
                      style={{ fontFamily: "monospace", fontSize: 12 }}
                    />
                  </div>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Public A2A URL</span>
                    <input
                      className="omni-input"
                      type="text"
                      placeholder="http://127.0.0.1:1423 (default for local hub)"
                      value={settings.a2a_public_url}
                      onChange={(e) =>
                        setSettings((s) => s && { ...s, a2a_public_url: e.target.value })
                      }
                    />
                  </div>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Hub URL</span>
                    <input
                      className="omni-input"
                      type="text"
                      placeholder="http://127.0.0.1:8222"
                      value={settings.a2a_hub_url}
                      onChange={(e) =>
                        setSettings((s) => s && { ...s, a2a_hub_url: e.target.value })
                      }
                    />
                  </div>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Hub Admin Key</span>
                    <input
                      className="omni-input"
                      type="password"
                      placeholder="Prefer OMNILAUNCHER_A2A_HUB_ADMIN_KEY"
                      value={settings.a2a_hub_admin_key}
                      onChange={(e) =>
                        setSettings((s) => s && { ...s, a2a_hub_admin_key: e.target.value })
                      }
                    />
                  </div>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Hub Upstream Name</span>
                    <input
                      className="omni-input"
                      type="text"
                      value={settings.a2a_hub_upstream_name}
                      onChange={(e) =>
                        setSettings((s) => s && { ...s, a2a_hub_upstream_name: e.target.value })
                      }
                    />
                  </div>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Hub Prefix</span>
                    <input
                      className="omni-input"
                      type="text"
                      value={settings.a2a_hub_prefix}
                      onChange={(e) =>
                        setSettings((s) => s && { ...s, a2a_hub_prefix: e.target.value })
                      }
                    />
                  </div>
                  <div style={rowStyle(true)}>
                    <span style={rowLabelStyle}>Auto-register in Hub</span>
                    <label style={{ display: "flex", alignItems: "center", gap: 6 }}>
                      <input
                        type="checkbox"
                        checked={settings.a2a_hub_auto_register}
                        onChange={(e) =>
                          setSettings((s) =>
                            s && { ...s, a2a_hub_auto_register: e.target.checked },
                          )
                        }
                      />
                      <span>Upsert this backend as an omni-agent-hub upstream on server startup</span>
                    </label>
                  </div>
                </div>
              </div>
            )}
          </div>

          {/* Save bar */}
          <div
            style={{
              padding: "12px 28px",
              flexShrink: 0,
              borderTop: "1px solid var(--border)",
              display: "flex",
              justifyContent: "flex-end",
              alignItems: "center",
              gap: 10,
            }}
          >
            {saveStatus === "success" && (
              <span className="omni-status omni-status--success">✓ Saved</span>
            )}
            {saveStatus === "error" && (
              <span className="omni-status omni-status--error">
                ✗ Save failed
              </span>
            )}
            <button
              type="button"
              className="omni-btn omni-btn--primary"
              onClick={handleSave}
              disabled={saveStatus === "success"}
              aria-disabled={saveStatus === "success"}
            >
              {saveStatus === "success" ? "✓ Saved" : "Save Settings"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
