import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { emit } from "@tauri-apps/api/event";

interface AppSettings {
  ai_base_url: string;
  ai_model: string;
  ai_api_key: string;
  theme: string;
  hotkey: string;
  max_results: number;
  background_url: string;
}

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

const baseInputStyle: React.CSSProperties = {
  width: "100%",
  background: "var(--surface)",
  border: "1px solid var(--border)",
  borderRadius: 8,
  color: "var(--text)",
  padding: "8px 10px",
  fontSize: 13,
  outline: "none",
  boxSizing: "border-box",
  transition: "border-color 0.15s, box-shadow 0.15s",
  fontFamily: "inherit",
};

const focusedInputStyle: React.CSSProperties = {
  ...baseInputStyle,
  borderColor: "var(--accent)",
  boxShadow: "0 0 0 2px var(--accent-dim)",
};

interface Props {
  onClose?: () => void;
}

export default function SettingsWindow({ onClose }: Props = {}) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [saved, setSaved] = useState(false);
  const [loading, setLoading] = useState(true);
  const [activeTab, setActiveTab] = useState<TabId>("ai");
  const [saveHover, setSaveHover] = useState(false);
  const [focusedField, setFocusedField] = useState<string | null>(null);

  const currentBgUrl = settings?.background_url ?? "";
  const isCustomBg =
    currentBgUrl !== "" &&
    !BG_PRESETS.some((p) => p.value === currentBgUrl && p.value !== "__custom__");
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
          theme: "system",
          hotkey: "Alt+Space",
          max_results: 10,
          background_url: "",
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
  }, [settings?.ai_base_url, settings?.ai_api_key]);

  useEffect(() => {
    if (settings?.ai_base_url) {
      fetchModels();
    }
  }, [settings?.ai_base_url, settings?.ai_api_key]);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setShowModelDropdown(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  const filteredModels = models.filter((m) =>
    m.toLowerCase().includes(modelFilter.toLowerCase())
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

  const inputStyle = (field: string): React.CSSProperties =>
    focusedField === field ? focusedInputStyle : baseInputStyle;

  const fProps = (field: string) => ({
    onFocus: () => setFocusedField(field),
    onBlur: () => setFocusedField(null),
  });

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
        }}
      >
        Loading settings…
      </div>
    );
  }

  const sectionHeaderStyle: React.CSSProperties = {
    fontSize: 11,
    fontWeight: 700,
    letterSpacing: "0.08em",
    color: "var(--accent)",
    textTransform: "uppercase",
    marginBottom: 10,
  };

  const cardStyle: React.CSSProperties = {
    background: "var(--bg-elevated)",
    border: "1px solid var(--border)",
    borderRadius: 8,
    padding: "4px 0",
    marginBottom: 20,
  };

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
      {/* Title bar — full width */}
      <div
        data-tauri-drag-region
        style={{
          height: 44,
          display: "flex",
          alignItems: "center",
          padding: "0 20px",
          borderBottom: "1px solid var(--border)",
          flexShrink: 0,
          justifyContent: "space-between",
        }}
      >
        <span style={{ fontSize: 14, fontWeight: 600, color: "var(--text)" }}>
          ⚙&nbsp;&nbsp;Preferences
        </span>
        <button
          onClick={() => onClose?.()}
          style={{
            background: "none",
            border: "none",
            color: "var(--sub)",
            fontSize: 16,
            cursor: "pointer",
            lineHeight: 1,
          }}
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
                onClick={() => setActiveTab(tab.id)}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  padding: "10px 14px",
                  borderRadius: 6,
                  fontSize: 13,
                  cursor: "pointer",
                  border: "none",
                  background: isActive ? "var(--accent-dim)" : "transparent",
                  color: isActive ? "var(--text)" : "var(--sub)",
                  borderLeft: isActive ? "2px solid var(--accent)" : "2px solid transparent",
                  borderTop: "none",
                  borderRight: "none",
                  borderBottom: "none",
                  transition: "all 0.15s",
                  textAlign: "left",
                  width: "100%",
                  fontFamily: "inherit",
                }}
                onMouseEnter={(e) => {
                  if (!isActive) (e.currentTarget as HTMLButtonElement).style.background = "var(--accent-hover)";
                }}
                onMouseLeave={(e) => {
                  if (!isActive) (e.currentTarget as HTMLButtonElement).style.background = "transparent";
                }}
              >
                <span>{tab.icon}</span>
                <span>{tab.label}</span>
              </button>
            );
          })}
        </div>

        {/* Right content pane */}
        <div style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
          {/* Scrollable area */}
          <div style={{ flex: 1, overflowY: "auto", padding: "24px 28px" }}>

            {activeTab === "ai" && (
              <div>
                <div style={sectionHeaderStyle}>AI Provider</div>
                <div style={cardStyle}>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Provider URL</span>
                    <input
                      style={inputStyle("ai_base_url")}
                      value={settings.ai_base_url}
                      onChange={(e) => setSettings((s) => s && { ...s, ai_base_url: e.target.value })}
                      {...fProps("ai_base_url")}
                    />
                  </div>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>API Key</span>
                    <input
                      style={inputStyle("ai_api_key")}
                      type="password"
                      value={settings.ai_api_key}
                      onChange={(e) => setSettings((s) => s && { ...s, ai_api_key: e.target.value })}
                      placeholder="(optional)"
                      {...fProps("ai_api_key")}
                    />
                  </div>
                  <div ref={dropdownRef} style={{ ...rowStyle(true), position: "relative" }}>
                    <span style={rowLabelStyle}>
                      Model
                      {modelsLoading && <span style={{ color: "var(--accent)" }}> (loading…)</span>}
                      {modelsError && <span style={{ color: "var(--error)" }}> ⚠</span>}
                    </span>
                    <div style={{ position: "relative", width: "100%" }}>
                      <input
                        ref={modelInputRef}
                        style={inputStyle("ai_model")}
                        value={modelFilter}
                        onChange={(e) => {
                          setModelFilter(e.target.value);
                          setSettings((s) => s && { ...s, ai_model: e.target.value });
                          setShowModelDropdown(true);
                        }}
                        onFocus={() => { setFocusedField("ai_model"); setShowModelDropdown(true); }}
                        onBlur={() => setFocusedField(null)}
                        placeholder="Type to filter models…"
                      />
                      {showModelDropdown && filteredModels.length > 0 && (
                        <div
                          style={{
                            position: "absolute",
                            zIndex: 100,
                            left: 0,
                            right: 0,
                            top: "calc(100% + 2px)",
                            background: "var(--surface)",
                            border: "1px solid var(--border)",
                            borderRadius: 8,
                            maxHeight: 180,
                            overflowY: "auto",
                          }}
                        >
                          {filteredModels.map((m) => (
                            <div
                              key={m}
                              onClick={() => handleModelSelect(m)}
                              style={{
                                padding: "7px 10px",
                                fontSize: 13,
                                cursor: "pointer",
                                color: m === settings.ai_model ? "var(--accent)" : "var(--text)",
                                background: m === settings.ai_model ? "var(--accent-dim)" : "transparent",
                              }}
                              onMouseEnter={(e) => ((e.target as HTMLDivElement).style.background = "var(--accent-hover)")}
                              onMouseLeave={(e) =>
                                ((e.target as HTMLDivElement).style.background =
                                  m === settings.ai_model ? "var(--accent-dim)" : "transparent")
                              }
                            >
                              {m}
                            </div>
                          ))}
                        </div>
                      )}
                      {showModelDropdown && !modelsLoading && filteredModels.length === 0 && models.length > 0 && (
                        <div
                          style={{
                            position: "absolute",
                            zIndex: 100,
                            left: 0,
                            right: 0,
                            top: "calc(100% + 2px)",
                            background: "var(--surface)",
                            border: "1px solid var(--border)",
                            borderRadius: 8,
                            padding: "8px 10px",
                            fontSize: 13,
                            color: "var(--sub)",
                          }}
                        >
                          No matches
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              </div>
            )}

            {activeTab === "appearance" && (
              <div>
                <div style={sectionHeaderStyle}>Appearance</div>
                <div style={cardStyle}>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Theme</span>
                    <select
                      style={{ ...inputStyle("theme"), cursor: "pointer" }}
                      value={settings.theme}
                      onChange={(e) => setSettings((s) => s && { ...s, theme: e.target.value })}
                      {...fProps("theme")}
                    >
                      <option value="system">System (Follow OS)</option>
                      <option value="dark">Dark (Battle Blue)</option>
                      <option value="light">Light (Catppuccin Latte)</option>
                    </select>
                  </div>
                  <div style={rowStyle(!isCustomBg && bgSelectValue !== "__custom__")}>
                    <span style={rowLabelStyle}>Background</span>
                    <select
                      style={{ ...inputStyle("bg_select"), cursor: "pointer" }}
                      value={bgSelectValue}
                      onChange={(e) => {
                        const val = e.target.value;
                        if (val !== "__custom__") {
                          setSettings((s) => s && { ...s, background_url: val });
                        }
                      }}
                      {...fProps("bg_select")}
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
                        style={inputStyle("bg_url")}
                        value={currentBgUrl}
                        onChange={(e) => setSettings((s) => s && { ...s, background_url: e.target.value })}
                        placeholder="https://example.com/image.jpg"
                        {...fProps("bg_url")}
                      />
                    </div>
                  )}
                </div>
              </div>
            )}

            {activeTab === "general" && (
              <div>
                <div style={sectionHeaderStyle}>General</div>
                <div style={cardStyle}>
                  <div style={rowStyle()}>
                    <span style={rowLabelStyle}>Hotkey</span>
                    <div
                      style={{
                        ...baseInputStyle,
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
                      style={inputStyle("max_results")}
                      type="number"
                      min={1}
                      max={50}
                      value={settings.max_results}
                      onChange={(e) =>
                        setSettings((s) => s && { ...s, max_results: parseInt(e.target.value) || 10 })
                      }
                      {...fProps("max_results")}
                    />
                  </div>
                </div>
              </div>
            )}
          </div>

          {/* Save button — fixed at bottom of right pane */}
          <div style={{ padding: "12px 28px", flexShrink: 0, borderTop: "1px solid var(--border)", display: "flex", justifyContent: "flex-end", gap: 8 }}>
            <button
              onClick={handleSave}
              onMouseEnter={() => setSaveHover(true)}
              onMouseLeave={() => setSaveHover(false)}
              style={{
                height: 32,
                padding: "0 18px",
                background: saved
                  ? "var(--accent-dim)"
                  : "var(--accent)",
                color: saved ? "var(--accent)" : "var(--user-bubble-text)",
                border: saved ? "1px solid var(--accent)" : "1px solid transparent",
                borderRadius: 8,
                fontSize: 13,
                fontWeight: 600,
                cursor: "pointer",
                transition: "all 0.15s",
                fontFamily: "inherit",
                opacity: saveHover && !saved ? 0.9 : 1,
              }}
            >
              {saved ? "✓ Saved" : "Save Settings"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
