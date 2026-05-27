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

const inputStyle: React.CSSProperties = {
  width: "100%",
  background: "rgba(255,255,255,0.06)",
  border: "1px solid rgba(255,255,255,0.12)",
  borderRadius: 6,
  color: "#e8eaf6",
  padding: "7px 10px",
  fontSize: 13,
  outline: "none",
  boxSizing: "border-box",
};

const labelStyle: React.CSSProperties = {
  fontSize: 11,
  color: "#8892b0",
  marginBottom: 4,
  display: "block",
};

export default function SettingsWindow() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [saved, setSaved] = useState(false);
  const [loading, setLoading] = useState(true);

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

  if (loading || !settings) {
    return (
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: "100vh",
          background: "#0b1220",
          color: "#8892b0",
          fontFamily: "'Aptos Display', 'Segoe UI Variable Display', 'Segoe UI', system-ui, sans-serif",
        }}
      >
        Loading settings…
      </div>
    );
  }

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100vh",
        background: "#0b1220",
        color: "#e8eaf6",
        fontFamily: "'Aptos Display', 'Segoe UI Variable Display', 'Segoe UI', system-ui, sans-serif",
      }}
    >
      {/* Title bar */}
      <div
        style={{
          padding: "16px 20px 0",
          flexShrink: 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <span style={{ fontSize: 15, fontWeight: 700, letterSpacing: "0.04em", color: "#e8eaf6" }}>
          ⚙ Settings
        </span>
        <button
          onClick={() => getCurrentWindow().close()}
          style={{ background: "none", border: "none", color: "#8892b0", fontSize: 16, cursor: "pointer" }}
        >
          ✕
        </button>
      </div>

      {/* Scrollable form */}
      <div
        style={{
          flex: 1,
          overflowY: "auto",
          padding: "20px",
          display: "flex",
          flexDirection: "column",
          gap: 24,
        }}
      >
        {/* AI Section */}
        <Section title="AI">
          <label>
            <span style={labelStyle}>Provider URL</span>
            <input
              style={inputStyle}
              value={settings.ai_base_url}
              onChange={(e) => setSettings((s) => s && { ...s, ai_base_url: e.target.value })}
            />
          </label>

          <label>
            <span style={labelStyle}>API Key</span>
            <input
              style={inputStyle}
              type="password"
              value={settings.ai_api_key}
              onChange={(e) => setSettings((s) => s && { ...s, ai_api_key: e.target.value })}
              placeholder="(optional)"
            />
          </label>

          <div ref={dropdownRef} style={{ position: "relative" }}>
            <span style={labelStyle}>
              Model
              {modelsLoading && <span style={{ color: "#5E81F4" }}> (loading…)</span>}
              {modelsError && <span style={{ color: "#f87171" }}> ⚠</span>}
            </span>
            <input
              ref={modelInputRef}
              style={inputStyle}
              value={modelFilter}
              onChange={(e) => {
                setModelFilter(e.target.value);
                setSettings((s) => s && { ...s, ai_model: e.target.value });
                setShowModelDropdown(true);
              }}
              onFocus={() => setShowModelDropdown(true)}
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
                  background: "#16233B",
                  border: "1px solid rgba(255,255,255,0.12)",
                  borderRadius: 6,
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
                      color: m === settings.ai_model ? "#5E81F4" : "#e8eaf6",
                      background: m === settings.ai_model ? "rgba(94,129,244,0.1)" : "transparent",
                    }}
                    onMouseEnter={(e) => ((e.target as HTMLDivElement).style.background = "rgba(255,255,255,0.06)")}
                    onMouseLeave={(e) =>
                      ((e.target as HTMLDivElement).style.background =
                        m === settings.ai_model ? "rgba(94,129,244,0.1)" : "transparent")
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
                  background: "#16233B",
                  border: "1px solid rgba(255,255,255,0.12)",
                  borderRadius: 6,
                  padding: "7px 10px",
                  fontSize: 13,
                  color: "#8892b0",
                }}
              >
                No matches
              </div>
            )}
          </div>
        </Section>

        {/* Appearance Section */}
        <Section title="Appearance">
          <label>
            <span style={labelStyle}>Theme</span>
            <select
              style={{ ...inputStyle, cursor: "pointer" }}
              value={settings.theme}
              onChange={(e) => setSettings((s) => s && { ...s, theme: e.target.value })}
            >
              <option value="system">System (Follow OS)</option>
              <option value="dark">Dark (Battle Blue)</option>
              <option value="light">Light (Catppuccin Latte)</option>
            </select>
          </label>

          <div>
            <span style={labelStyle}>Background Image</span>
            <select
              style={{ ...inputStyle, cursor: "pointer" }}
              value={bgSelectValue}
              onChange={(e) => {
                const val = e.target.value;
                if (val !== "__custom__") {
                  setSettings((s) => s && { ...s, background_url: val });
                }
              }}
            >
              {BG_PRESETS.map((p) => (
                <option key={p.label} value={p.value}>
                  {p.label}
                </option>
              ))}
            </select>
            {(bgSelectValue === "__custom__" || isCustomBg) && (
              <input
                style={{ ...inputStyle, marginTop: 6 }}
                value={currentBgUrl}
                onChange={(e) => setSettings((s) => s && { ...s, background_url: e.target.value })}
                placeholder="https://example.com/image.jpg"
              />
            )}
          </div>
        </Section>

        {/* General Section */}
        <Section title="General">
          <div>
            <span style={labelStyle}>Hotkey</span>
            <div
              style={{
                ...inputStyle,
                color: "#8892b0",
                cursor: "default",
                userSelect: "none",
              }}
            >
              {settings.hotkey}
            </div>
          </div>

          <label>
            <span style={labelStyle}>Max Results</span>
            <input
              style={inputStyle}
              type="number"
              min={1}
              max={50}
              value={settings.max_results}
              onChange={(e) =>
                setSettings((s) => s && { ...s, max_results: parseInt(e.target.value) || 10 })
              }
            />
          </label>
        </Section>

        {/* Save button */}
        <button
          onClick={handleSave}
          style={{
            padding: "10px 20px",
            background: saved ? "rgba(94,129,244,0.2)" : "#5E81F4",
            color: saved ? "#5E81F4" : "#fff",
            border: saved ? "1px solid #5E81F4" : "none",
            borderRadius: 8,
            fontSize: 14,
            fontWeight: 600,
            cursor: "pointer",
            transition: "all 0.2s",
            alignSelf: "flex-start",
          }}
        >
          {saved ? "✓ Saved" : "Save Settings"}
        </button>
      </div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <div
        style={{
          fontSize: 11,
          fontWeight: 700,
          letterSpacing: "0.1em",
          color: "#5E81F4",
          textTransform: "uppercase",
          marginBottom: 12,
        }}
      >
        {title}
      </div>
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 14,
          background: "rgba(255,255,255,0.03)",
          borderRadius: 10,
          padding: "14px 16px",
          border: "1px solid rgba(255,255,255,0.07)",
        }}
      >
        {children}
      </div>
    </div>
  );
}
