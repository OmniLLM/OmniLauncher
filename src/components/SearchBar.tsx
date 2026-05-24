import { useRef, useEffect } from 'react'

interface Props {
  value: string
  onChange: (v: string) => void
  onSubmit: (v: string, forceAi: boolean) => void
  isAiMode: boolean
  loading: boolean
  colors: Record<string, string>
  onSettingsClick: () => void
}

export default function SearchBar({ value, onChange, onSubmit, isAiMode, loading, colors, onSettingsClick }: Props) {
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  const icon = loading ? '⏳' : isAiMode ? '🤖' : '🔍'

  const placeholder = isAiMode
    ? 'Ask AI anything… (Enter to send)'
    : 'Type to launch, search, calculate…  |  ? for AI'

  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      padding: '12px 16px',
      gap: '10px',
      borderBottom: value ? `1px solid ${colors.surface}` : 'none',
      // Highlight border when AI mode is active
      outline: isAiMode ? `1.5px solid ${colors.accent}` : 'none',
      borderRadius: isAiMode ? '12px 12px 0 0' : undefined,
    }}>
      <span style={{ fontSize: '18px', opacity: 0.6 }}>
        {icon}
      </span>
      <input
        ref={inputRef}
        value={value}
        onChange={e => onChange(e.target.value)}
        onKeyDown={e => {
          if (e.key === 'Enter') {
            e.preventDefault()
            onSubmit(value, e.ctrlKey || e.metaKey)
          }
        }}
        placeholder={placeholder}
        style={{
          flex: 1,
          background: 'transparent',
          border: 'none',
          outline: 'none',
          fontSize: '18px',
          color: colors.text,
          caretColor: colors.accent
        }}
      />
      <button
        onClick={onSettingsClick}
        style={{
          background: 'none',
          border: 'none',
          cursor: 'pointer',
          fontSize: '16px',
          opacity: 0.5,
          color: colors.text,
          padding: '4px'
        }}
        title="Settings (Ctrl+,)"
      >
        ⚙️
      </button>
    </div>
  )
}
