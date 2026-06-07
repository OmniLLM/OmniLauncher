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
] as const;

type TabId = (typeof TABS)[number]["id"];

interface Props {
  onClose?: () => void;
}

export default function SettingsWindow({ onClose }: Props = {}) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [saved, setSaved] = useState(false);
  const [loading, setLoading] = useState(true);
  const [activeTab, setActiveTab] = useState<TabId>("ai");

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
    invoke<AppSettings>("get_settings")
      .then((s) => {
        setSettings(s);
        setModelFilter(s.ai_model);
        setLoading(false);
      })
      .catch(() => {
        setSettings({
          ai_base_url: "http://localhost:5000",
          ai_model: "auto",
          ai_api_key: "",
          ai_timeout_secs: 120,
          theme: "system",
          hotkey: "Alt+Space",
          max_results: 10,
          background_url: "",
          backend_url: "",
          backend_token: "",
        });
        setModelFilter("auto");
        setLoading(false);
      });
  }, []);

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
    try {
      await invoke("save_settings_cmd", { settings });
      await emit("omnilauncher://settings-saved", settings);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      console.error("Save error:", e);
    }
  };

  if (loading || !settings) {
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
                    <div
                      className="omni-input"
                      style={{
                        color: "var(--sub)",
                        cursor: "default",
                        userSelect: "none",
                      }}
                    >
                      {settings.hotkey}
                    </div>
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
                  <div style={rowStyle(true)}>
                    <span style={rowLabelStyle}>Backend Token</span>
                    <input
                      className="omni-input"
                      type="password"
                      placeholder="auto (same-machine) / paste cross-machine token"
                      autoComplete="off"
                      spellCheck={false}
                      value={settings.backend_token}
                      onChange={(e) =>
                        setSettings(
                          (s) => s && { ...s, backend_token: e.target.value },
                        )
                      }
                    />
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
            {saved && (
              <span className="omni-status omni-status--success">✓ Saved</span>
            )}
            <button
              type="button"
              className="omni-btn omni-btn--primary"
              onClick={handleSave}
              disabled={saved}
              aria-disabled={saved}
            >
              {saved ? "✓ Saved" : "Save Settings"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
