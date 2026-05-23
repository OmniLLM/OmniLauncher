import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'

interface AppSettings {
  ai_base_url: string
  ai_model: string
  ai_api_key: string
  theme: string
  hotkey: string
  max_results: number
}

interface Props {
  colors: Record<string, string>
  theme: string
  onThemeChange: (t: 'dark' | 'light') => void
  onClose: () => void
}

export default function SettingsPanel({ colors, theme, onThemeChange, onClose }: Props) {
  const [settings, setSettings] = useState<AppSettings>({
    ai_base_url: 'http://localhost:5000',
    ai_model: 'auto',
    ai_api_key: '',
    theme: theme,
    hotkey: 'Alt+Space',
    max_results: 10
  })
  const [saved, setSaved] = useState(false)

  const handleSave = async () => {
    try {
      await invoke('save_settings_cmd', { settings })
      setSaved(true)
      setTimeout(() => setSaved(false), 2000)
      onThemeChange(settings.theme as 'dark' | 'light')
    } catch (e) {
      console.error('Save error:', e)
    }
  }

  const inputStyle = {
    background: colors.surface,
    border: `1px solid ${colors.sub}`,
    borderRadius: '6px',
    padding: '8px 12px',
    color: colors.text,
    fontSize: '13px',
    width: '100%',
    boxSizing: 'border-box' as const
  }

  return (
    <div style={{ padding: '16px', overflow: 'auto', maxHeight: '520px' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '16px' }}>
        <h3 style={{ margin: 0, fontSize: '16px', color: colors.accent }}>Settings</h3>
        <button onClick={onClose} style={{ background: 'none', border: 'none', color: colors.sub, cursor: 'pointer', fontSize: '18px' }}>✕</button>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
        <label>
          <div style={{ fontSize: '12px', color: colors.sub, marginBottom: '4px' }}>AI Provider URL</div>
          <input style={inputStyle} value={settings.ai_base_url} onChange={e => setSettings(s => ({ ...s, ai_base_url: e.target.value }))} />
        </label>

        <label>
          <div style={{ fontSize: '12px', color: colors.sub, marginBottom: '4px' }}>Model</div>
          <input style={inputStyle} value={settings.ai_model} onChange={e => setSettings(s => ({ ...s, ai_model: e.target.value }))} placeholder="auto" />
        </label>

        <label>
          <div style={{ fontSize: '12px', color: colors.sub, marginBottom: '4px' }}>API Key</div>
          <input style={inputStyle} type="password" value={settings.ai_api_key} onChange={e => setSettings(s => ({ ...s, ai_api_key: e.target.value }))} placeholder="(optional)" />
        </label>

        <label>
          <div style={{ fontSize: '12px', color: colors.sub, marginBottom: '4px' }}>Theme</div>
          <select style={inputStyle} value={settings.theme} onChange={e => setSettings(s => ({ ...s, theme: e.target.value }))}>
            <option value="dark">Dark (Catppuccin Mocha)</option>
            <option value="light">Light (Catppuccin Latte)</option>
          </select>
        </label>

        <div>
          <div style={{ fontSize: '12px', color: colors.sub, marginBottom: '4px' }}>Hotkey</div>
          <div style={{ ...inputStyle, opacity: 0.6 }}>{settings.hotkey}</div>
        </div>

        <button
          onClick={handleSave}
          style={{
            background: colors.accent,
            border: 'none',
            borderRadius: '8px',
            padding: '10px',
            color: '#fff',
            cursor: 'pointer',
            fontSize: '14px',
            fontWeight: 600,
            marginTop: '8px'
          }}
        >
          {saved ? '✓ Saved!' : 'Save Settings'}
        </button>
      </div>
    </div>
  )
}
