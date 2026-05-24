import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface AppSettings {
  ai_base_url: string;
  ai_model: string;
  ai_api_key: string;
  theme: string;
  hotkey: string;
  max_results: number;
}

interface Props {
  theme: string;
  onThemeChange: (t: "dark" | "light") => void;
  onClose: () => void;
  initialSettings: AppSettings | null;
}

export default function SettingsPanel({
  theme,
  onThemeChange,
  onClose,
  initialSettings,
}: Props) {
  const [settings, setSettings] = useState<AppSettings | null>(initialSettings);
  const [saved, setSaved] = useState(false);
  const [loading, setLoading] = useState(!initialSettings);

  useEffect(() => {
    invoke<AppSettings>("get_settings")
      .then((s) => {
        setSettings(s);
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
        setLoading(false);
      });
  }, []);

  const handleSave = async () => {
    if (!settings) return;
    try {
      await invoke("save_settings_cmd", { settings });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
      onThemeChange(settings.theme as "dark" | "light");
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
        <button className="settings__close" onClick={onClose}>✕</button>
      </div>

      <div className="settings__form">
        <label>
          <div className="settings__label">AI Provider URL</div>
          <input
            className="settings__input"
            value={settings.ai_base_url}
            onChange={(e) => setSettings((s) => s && { ...s, ai_base_url: e.target.value })}
          />
        </label>

        <label>
          <div className="settings__label">Model</div>
          <input
            className="settings__input"
            value={settings.ai_model}
            onChange={(e) => setSettings((s) => s && { ...s, ai_model: e.target.value })}
            placeholder="auto"
          />
        </label>

        <label>
          <div className="settings__label">API Key</div>
          <input
            className="settings__input"
            type="password"
            value={settings.ai_api_key}
            onChange={(e) => setSettings((s) => s && { ...s, ai_api_key: e.target.value })}
            placeholder="(optional)"
          />
        </label>

        <label>
          <div className="settings__label">Theme</div>
          <select
            className="settings__select"
            value={settings.theme}
            onChange={(e) => setSettings((s) => s && { ...s, theme: e.target.value })}
          >
            <option value="dark">Dark (Catppuccin Mocha)</option>
            <option value="light">Light (Catppuccin Latte)</option>
          </select>
        </label>

        <div>
          <div className="settings__label">Hotkey</div>
          <div className="settings__input settings__input--readonly">{settings.hotkey}</div>
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
