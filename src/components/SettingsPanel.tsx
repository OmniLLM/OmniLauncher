import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

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

type ThemeMode = "dark" | "light" | "system";

function parseThemeMode(theme: string): ThemeMode {
  if (theme === "dark" || theme === "light" || theme === "system") {
    return theme;
  }
  return "system";
}

interface Props {
  theme: string;
  onThemeChange: (t: ThemeMode) => void;
  onBackgroundChange: (url: string) => void;
  onClose: () => void;
  initialSettings: AppSettings | null;
}

export default function SettingsPanel({
  theme,
  onThemeChange,
  onBackgroundChange,
  onClose,
  initialSettings,
}: Props) {
  const [settings, setSettings] = useState<AppSettings | null>(initialSettings);
  const [saved, setSaved] = useState(false);
  const [loading, setLoading] = useState(!initialSettings);

  // Derive whether the current background_url matches a preset or is custom
  const currentBgUrl = settings?.background_url ?? "";
  const isCustomBg = currentBgUrl !== "" && !BG_PRESETS.some((p) => p.value === currentBgUrl && p.value !== "__custom__");
  const bgSelectValue = isCustomBg ? "__custom__" : currentBgUrl;

  // Model list state
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
      .catch((e) => {
        console.error("Failed to load settings:", e);
        setSettings(
          initialSettings ?? {
            ai_base_url: "http://localhost:5000",
            ai_model: "auto",
            ai_api_key: "",
            theme: theme,
            hotkey: "Alt+Space",
            max_results: 10,
          },
        );
        setModelFilter(initialSettings?.ai_model || "auto");
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

  // Fetch models when endpoint or api key changes
  useEffect(() => {
    if (settings?.ai_base_url) {
      fetchModels();
    }
  }, [settings?.ai_base_url, settings?.ai_api_key]);

  // Close dropdown on outside click
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
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
      onThemeChange(parseThemeMode(settings.theme));
      onBackgroundChange(settings.background_url ?? "");
    } catch (e) {
      console.error("Save error:", e);
    }
  };

  if (loading || !settings) {
    return (
      <div className="loading">
        <div className="loading__text">Loading settings...</div>
      </div>
    );
  }

  return (
    <div className="settings">
      <div className="settings__header">
        <h3 className="settings__title">Settings</h3>
        <button className="settings__close" onClick={onClose}>
          ✕
        </button>
      </div>

      <div className="settings__form">
        <label>
          <div className="settings__label">AI Provider URL</div>
          <input
            className="settings__input"
            value={settings.ai_base_url}
            onChange={(e) =>
              setSettings((s) => s && { ...s, ai_base_url: e.target.value })
            }
          />
        </label>

        <label>
          <div className="settings__label">API Key</div>
          <input
            className="settings__input"
            type="password"
            value={settings.ai_api_key}
            onChange={(e) =>
              setSettings((s) => s && { ...s, ai_api_key: e.target.value })
            }
            placeholder="(optional)"
          />
        </label>

        <div className="settings__model-field" ref={dropdownRef}>
          <div className="settings__label">
            Model
            {modelsLoading && (
              <span className="settings__model-loading"> (loading...)</span>
            )}
            {modelsError && <span className="settings__model-error"> ⚠</span>}
          </div>
          <input
            ref={modelInputRef}
            className="settings__input"
            value={modelFilter}
            onChange={(e) => {
              setModelFilter(e.target.value);
              setSettings((s) => s && { ...s, ai_model: e.target.value });
              setShowModelDropdown(true);
            }}
            onFocus={() => setShowModelDropdown(true)}
            placeholder="Type to filter models..."
          />
          {showModelDropdown && filteredModels.length > 0 && (
            <div className="settings__model-dropdown">
              {filteredModels.map((m) => (
                <div
                  key={m}
                  className={`settings__model-option${m === settings.ai_model ? " settings__model-option--selected" : ""}`}
                  onClick={() => handleModelSelect(m)}
                >
                  {m}
                </div>
              ))}
            </div>
          )}
          {showModelDropdown &&
            !modelsLoading &&
            filteredModels.length === 0 &&
            models.length > 0 && (
              <div className="settings__model-dropdown">
                <div className="settings__model-option settings__model-option--empty">
                  No matches
                </div>
              </div>
            )}
        </div>

        <label>
          <div className="settings__label">Theme</div>
          <select
            className="settings__select"
            value={settings.theme}
            onChange={(e) =>
              setSettings((s) => s && { ...s, theme: e.target.value })
            }
          >
            <option value="system">System (Follow OS)</option>
            <option value="dark">Dark (Battle Blue)</option>
            <option value="light">Light (Catppuccin Latte)</option>
          </select>
        </label>

        <div>
          <div className="settings__label">Background Image</div>
          <select
            className="settings__select"
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
              className="settings__input"
              style={{ marginTop: 6 }}
              value={currentBgUrl}
              onChange={(e) =>
                setSettings((s) => s && { ...s, background_url: e.target.value })
              }
              placeholder="https://example.com/image.jpg"
            />
          )}
        </div>

        <div>
          <div className="settings__label">Hotkey</div>
          <div className="settings__input settings__input--readonly">
            {settings.hotkey}
          </div>
        </div>

        <button
          className={`settings__save-btn${saved ? " settings__save-btn--saved" : ""}`}
          onClick={handleSave}
        >
          {saved ? "✓ Saved" : "Save Settings"}
        </button>
      </div>
    </div>
  );
}
