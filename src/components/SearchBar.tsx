import { useRef, useEffect, RefObject } from "react";
import { isAiPrefix } from "../App";

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
  /** External ref forwarded from App so App can imperatively focus */
  inputRef?: RefObject<HTMLInputElement>
}

const HINT_ITEMS = [
  { key: '=', label: 'calc' },
  { key: '>', label: 'shell' },
  { key: 'cb', label: 'clipboard' },
  { key: 'g', label: 'web' },
  { key: '?', label: 'AI' },
  { key: '/', label: 'commands' },
]

export default function SearchBar({
  value,
  onChange,
  onSubmit,
  isAiMode,
  loading,
  colors,
  onSettingsClick,
  showHintBar = false,
  inputRef: externalRef,
}: Props) {
  const internalRef = useRef<HTMLInputElement>(null)
  const inputRef = externalRef ?? internalRef

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  // Re-focus whenever AI mode changes
  useEffect(() => {
    inputRef.current?.focus()
  }, [isAiMode])

  const isAI = isAiPrefix(value)
  const placeholder = isAiMode
    ? 'Ask AI anything…'
    : 'Type to launch, search, calculate…'

  return (
    <>
      <style>{`
        .omni-input-wrap {
          transition: box-shadow 180ms ease;
        }
        .omni-input-wrap:focus-within {
          box-shadow: 0 0 0 2px ${colors.accent}30, inset 0 0 0 1px ${colors.accent}55;
        }
        @keyframes omni-hint-fadein {
          from { opacity: 0; transform: translateY(3px); }
          to   { opacity: 1; transform: translateY(0); }
        }
      `}</style>

      <div
        style={{
          flexShrink: 0,
          borderTop: isAiMode ? `1px solid ${colors.surface}` : 'none',
        }}
      >
        {/* ── Main input row ─────────────────────────────────────────── */}
        <div
          className="omni-input-wrap"
          style={{
            display: 'flex',
            alignItems: 'center',
            padding: '0 14px',
            height: '56px',
            gap: '10px',
            borderBottom: !isAiMode && value ? `1px solid ${colors.surface}` : 'none',
            background: isAiMode ? colors.bg : 'transparent',
            borderRadius: isAiMode ? '0' : '14px',
          }}
        >
          {/* Leading icon / spinner */}
          <span
            style={{
              fontSize: '17px',
              opacity: loading ? 1 : 0.5,
              transition: 'opacity 150ms',
              flexShrink: 0,
              lineHeight: 1,
              display: 'flex',
              alignItems: 'center',
            }}
          >
            {loading ? <LoadingSpinner color={colors.accent} /> : isAI ? '✦' : '⌕'}
          </span>

          {/* AI badge (shown inside left of input when "?" prefix is typed) */}
          {isAI && !isAiMode && (
            <span
              style={{
                fontSize: '10px',
                background: `${colors.accent}25`,
                color: colors.accent,
                padding: '2px 6px',
                borderRadius: '5px',
                fontWeight: 700,
                letterSpacing: '0.05em',
                flexShrink: 0,
                border: `1px solid ${colors.accent}44`,
              }}
            >
              AI
            </span>
          )}

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
              fontSize: '16px',
              color: colors.text,
              caretColor: colors.accent,
              fontFamily: 'inherit',
            }}
          />

          {/* AI mode badge (right side when fully in AI mode) */}
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
                border: `1px solid ${colors.accent}33`,
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

        {/* ── Hint bar (launcher mode, empty query only) ─────────────── */}
        {showHintBar && (
          <div
            style={{
              display: 'flex',
              gap: '4px',
              padding: '4px 16px 8px',
              alignItems: 'center',
              flexWrap: 'wrap',
              animation: 'omni-hint-fadein 200ms ease both',
            }}
          >
            {HINT_ITEMS.map(({ key, label }) => (
              <span
                key={key}
                style={{
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: '3px',
                  fontSize: '11px',
                  color: colors.sub,
                  userSelect: 'none',
                  marginRight: '6px',
                }}
              >
                <kbd
                  style={{
                    fontFamily: "'JetBrains Mono', 'Fira Code', 'Consolas', monospace",
                    fontSize: '10px',
                    background: colors.surface,
                    color: colors.accent,
                    padding: '1px 5px',
                    borderRadius: '4px',
                    border: `1px solid ${colors.surface2}`,
                    lineHeight: 1.6,
                  }}
                >
                  {key}
                </kbd>
                <span style={{ opacity: 0.7 }}>{label}</span>
              </span>
            ))}
          </div>
        )}
      </div>
    </>
  )
}

// ─── Tiny inline spinner ──────────────────────────────────────────────────────

function LoadingSpinner({ color }: { color: string }) {
  return (
    <span
      style={{
        display: 'inline-block',
        width: '15px',
        height: '15px',
        border: `2px solid transparent`,
        borderTopColor: color,
        borderRadius: '50%',
        animation: 'omni-spin 0.7s linear infinite',
        verticalAlign: 'middle',
        flexShrink: 0,
      }}
    />
  )
}
