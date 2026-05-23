import { useRef, useEffect } from 'react'

interface Props {
  value: string
  onChange: (v: string) => void
  onSubmit: (v: string, forceAi: boolean) => void
  isNatural: boolean
  loading: boolean
  colors: Record<string, string>
  onSettingsClick: () => void
}

export default function SearchBar({ value, onChange, onSubmit, isNatural, loading, colors, onSettingsClick }: Props) {
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      padding: '12px 16px',
      gap: '10px',
      borderBottom: value ? `1px solid ${colors.surface}` : 'none'
    }}>
      <span style={{ fontSize: '18px', opacity: 0.6 }}>
        {loading ? '⏳' : isNatural ? '🤖' : '🔍'}
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
        placeholder="Search or ask anything..."
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
