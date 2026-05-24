import { useRef, useEffect } from "react";

interface Props {
  value: string
  onChange: (v: string) => void
  onSubmit: (v: string, forceAi: boolean) => void
  isAiMode: boolean
  loading: boolean
  colors: Record<string, string>
  onSettingsClick: () => void
  /** Show the one-line hint bar at the bottom of an empty launcher input */
  showHintBar?: boolean
}

const HINT_TEXT = '= calc   > shell   cb clipboard   g web   ? AI'

export default function SearchBar({
  value,
  onChange,
  onSubmit,
  isAiMode,
  loading,
  colors,
  onSettingsClick,
  showHintBar = false,
}: Props) {
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const iconClass = `search-bar__icon${loading ? " search-bar__icon--loading" : isNatural ? " search-bar__icon--ai" : ""}`;

  // Re-focus whenever AI mode changes (after transition)
  useEffect(() => {
    inputRef.current?.focus()
  }, [isAiMode])

  const placeholder = isAiMode
    ? 'Ask AI anything…'
    : 'Type to launch, search, calculate…'

  return (
    <div
      style={{
        flexShrink: 0,
        borderTop: isAiMode ? `1px solid ${colors.surface}` : 'none',
      }}
    >
      {/* ── Main input row ─────────────────────────────────────────────── */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          padding: '12px 14px',
          gap: '10px',
          borderBottom: !isAiMode && value ? `1px solid ${colors.surface}` : 'none',
          background: isAiMode ? colors.bg : 'transparent',
        }}
      >
        {/* Icon */}
        <span
          style={{
            fontSize: '17px',
            opacity: loading ? 1 : 0.55,
            transition: 'opacity 150ms',
            flexShrink: 0,
            lineHeight: 1,
          }}
        >
          {loading ? null : isAiMode ? '✦' : '⌕'}
          {loading && <LoadingSpinner color={colors.accent} />}
        </span>

        {/* Input */}
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
            caretColor: colors.accent,
            fontFamily: 'inherit',
          }}
        />

        {/* AI mode badge */}
        {isAiMode && (
          <span
            style={{
              fontSize: '11px',
              background: `${colors.accent}22`,
              color: colors.accent,
              padding: '3px 8px',
              borderRadius: '6px',
              fontWeight: 600,
              letterSpacing: '0.04em',
              flexShrink: 0,
            }}
          >
            AI
          </span>
        )}

        {/* Settings button */}
        <button
          onClick={onSettingsClick}
          style={{
            background: 'none',
            border: 'none',
            cursor: 'pointer',
            fontSize: '15px',
            opacity: 0.4,
            color: colors.text,
            padding: '4px',
            flexShrink: 0,
            lineHeight: 1,
            transition: 'opacity 150ms',
          }}
          onMouseEnter={e => (e.currentTarget.style.opacity = '0.75')}
          onMouseLeave={e => (e.currentTarget.style.opacity = '0.4')}
          title="Settings (Ctrl+,)"
        >
          ⚙
        </button>
      </div>

      {/* ── Hint bar (launcher mode, empty query only) ─────────────────── */}
      {showHintBar && (
        <div
          style={{
            padding: '4px 16px 8px',
            fontSize: '11px',
            color: colors.sub,
            letterSpacing: '0.02em',
            userSelect: 'none',
          }}
        >
          {HINT_TEXT}
        </div>
      )}
    </div>
  );
}

// ─── Tiny inline spinner for loading state ────────────────────────────────

function LoadingSpinner({ color }: { color: string }) {
  return (
    <>
      <style>{`
        @keyframes omni-spin {
          to { transform: rotate(360deg); }
        }
        .omni-spinner {
          display: inline-block;
          width: 15px; height: 15px;
          border: 2px solid transparent;
          border-top-color: currentColor;
          border-radius: 50%;
          animation: omni-spin 0.7s linear infinite;
          vertical-align: middle;
        }
      `}</style>
      <span className="omni-spinner" style={{ color }} />
    </>
  )
}
