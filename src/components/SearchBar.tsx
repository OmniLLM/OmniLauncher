import { useRef, useEffect, RefObject } from "react";
import { isAiPrefix } from "../App";

interface Props {
  value: string;
  onChange: (v: string) => void;
  onSubmit: (v: string, forceAi: boolean) => void;
  isAiMode: boolean;
  loading: boolean;
  colors: Record<string, string>;
  onSettingsClick: () => void;
  /** Show the one-line hint bar at the bottom of an empty launcher input */
  showHintBar?: boolean;
  /** Render the empty launcher as a centered card */
  compact?: boolean;
  /** External ref forwarded from App so App can imperatively focus */
  inputRef?: RefObject<HTMLInputElement>;
}

// Core plugin prefixes (always shown)
const HINT_CORE = [
  { key: "=", label: "calc" },
  { key: ">", label: "shell" },
  { key: "* / f", label: "files" },
  { key: "b / bm", label: "bookmarks" },
  { key: "?", label: "AI" },
  { key: "/", label: "commands" },
];

// Web search prefixes (shown in a second row)
const HINT_SEARCH = [
  { key: "g", label: "Google" },
  { key: "yt / youtube", label: "YouTube" },
  { key: "gh / github", label: "GitHub" },
  { key: "wiki", label: "Wikipedia" },
  { key: "maps", label: "Maps" },
  { key: "so", label: "StackOverflow" },
  { key: "ddg", label: "DuckDuckGo" },
  { key: "bing", label: "Bing" },
  { key: "image", label: "Images" },
  { key: "lucky", label: "Feeling Lucky" },
  { key: "translate", label: "Translate" },
  { key: "ytmusic", label: "YT Music" },
  { key: "netflix", label: "Netflix" },
  { key: "gist", label: "Gist" },
  { key: "wolframalpha", label: "Wolfram" },
];

export default function SearchBar({
  value,
  onChange,
  onSubmit,
  isAiMode,
  loading,
  colors,
  onSettingsClick,
  showHintBar = false,
  compact = false,
  inputRef: externalRef,
}: Props) {
  const internalRef = useRef<HTMLInputElement>(null);
  const inputRef = externalRef ?? internalRef;

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Re-focus whenever AI mode changes
  useEffect(() => {
    inputRef.current?.focus();
  }, [isAiMode]);

  const isAI = isAiPrefix(value);
  const placeholder = isAiMode
    ? "Ask AI anything…"
    : "Type to launch, search, calculate…";

  return (
    <>
      <style>{`
        .omni-input-wrap {
          transition: box-shadow 180ms ease;
        }
        .omni-input-wrap:focus-within {
          box-shadow: 0 0 0 2px ${colors.accent}30, inset 0 0 0 1px ${colors.accent}55;
        }
        @keyframes omni-tagline-fadein {
          from { opacity: 0; transform: translateY(4px); }
          to   { opacity: 1; transform: translateY(0); }
        }
        @keyframes omni-hint-fadein {
          from { opacity: 0; transform: translateY(3px); }
          to   { opacity: 1; transform: translateY(0); }
        }
      `}</style>

      <div
        style={{
          flexShrink: 0,
          borderTop: isAiMode ? `1px solid ${colors.surface}` : "none",
        }}
      >
        {!isAiMode && (
          <div
            style={{
              padding: compact ? "0 2px 14px" : "12px 16px 0",
              animation: "omni-tagline-fadein 240ms ease both",
              textAlign: compact ? "center" : "left",
            }}
          >
            <div
              style={{
                color: colors.text,
                fontSize: compact ? "18px" : "13px",
                fontWeight: 800,
                letterSpacing: compact ? "0.01em" : "0.02em",
                lineHeight: 1.2,
              }}
            >
              OMNILAUNCHER
            </div>
          </div>
        )}
        {/* ── Main input row ─────────────────────────────────────────── */}
        <div
          className="omni-input-wrap"
          style={{
            display: "flex",
            alignItems: "center",
            padding: compact ? "0 16px" : "0 14px",
            height: compact ? "62px" : "56px",
            width: compact ? "min(96%, 840px)" : "100%",
            margin: compact ? "0 auto" : undefined,
            gap: "10px",
            border:
              compact && !isAiMode
                ? `1px solid ${colors.surface2}`
                : "none",
            borderBottom:
              !compact && !isAiMode && value
                ? `1px solid ${colors.surface}`
                : "none",
            background: compact
              ? colors.bg
              : colors.bg,
            borderRadius: compact ? "18px" : isAiMode ? "0" : "14px",
            boxShadow: "none",
          }}
        >
          {/* Leading icon / spinner */}
          <span
            style={{
              fontSize: compact ? "18px" : "17px",
              opacity: loading ? 1 : compact ? 0.82 : 0.5,
              color: compact ? colors.text : undefined,
              flexShrink: 0,
              lineHeight: 1,
              display: "flex",
              alignItems: "center",
            }}
          >
            {loading ? (
              <LoadingSpinner color={colors.accent} />
            ) : isAI ? (
              "✦"
            ) : (
              "⌕"
            )}
          </span>

          {/* AI badge (shown inside left of input when "?" prefix is typed) */}
          {isAI && !isAiMode && (
            <span
              style={{
                fontSize: "10px",
                background: `${colors.accent}25`,
                color: colors.accent,
                padding: "2px 6px",
                borderRadius: "5px",
                fontWeight: 700,
                letterSpacing: "0.05em",
                flexShrink: 0,
                border: `1px solid ${colors.accent}44`,
              }}
            >
              AI
            </span>
          )}

          {/* Input */}
          <input
            autoFocus
            ref={inputRef}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                onSubmit(value, e.ctrlKey || e.metaKey);
              }
            }}
            placeholder={placeholder}
            style={{
              flex: 1,
              background: "transparent",
              border: "none",
              outline: "none",
              fontSize: compact ? "16.5px" : "16px",
              color: colors.text,
              caretColor: colors.accent,
              fontFamily: "inherit",
              fontWeight: compact ? 500 : 400,
              letterSpacing: compact ? "0.005em" : undefined,
            }}
          />

          {/* AI mode badge (right side when fully in AI mode) */}
          {isAiMode && (
            <span
              style={{
                fontSize: "11px",
                background: `${colors.accent}22`,
                color: colors.accent,
                padding: "3px 8px",
                borderRadius: "6px",
                fontWeight: 600,
                letterSpacing: "0.04em",
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
              background: "none",
              border: "none",
              cursor: "pointer",
              fontSize: "15px",
              opacity: 0.4,
              color: colors.text,
              padding: "4px",
              flexShrink: 0,
              lineHeight: 1,
              transition: "opacity 150ms",
            }}
            onMouseEnter={(e) => (e.currentTarget.style.opacity = "0.75")}
            onMouseLeave={(e) => (e.currentTarget.style.opacity = "0.4")}
            title="Settings (Ctrl+,)"
          >
            ⚙
          </button>
        </div>

        {/* ── Hint bar (launcher mode, empty query only) ─────────────── */}
        {showHintBar && (
          <div
            style={{
              padding: compact ? "10px 2px 0" : "4px 16px 8px",
              width: compact ? "min(96%, 840px)" : "100%",
              margin: compact ? "0 auto" : undefined,
              animation: "omni-hint-fadein 240ms ease both",
            }}
          >
            {/* Core prefixes row */}
            <div
              style={{
                display: "flex",
                justifyContent: compact ? "center" : "flex-start",
                gap: compact ? "6px" : "4px",
                flexWrap: "wrap",
                marginBottom: compact ? "0" : "4px",
              }}
            >
              {HINT_CORE.map(({ key, label }) => (
                <HintChip
                  key={key}
                  prefix={key}
                  label={label}
                  colors={colors}
                  compact={compact}
                />
              ))}
            </div>
            <div
              style={{
                display: "flex",
                justifyContent: compact ? "center" : "flex-start",
                gap: compact ? "6px" : "4px",
                flexWrap: "wrap",
              }}
            >
              {HINT_SEARCH.map(({ key, label }) => (
                <HintChip
                  key={key}
                  prefix={key}
                  label={label}
                  colors={colors}
                  compact={compact}
                />
              ))}
            </div>
          </div>
        )}
      </div>
    </>
  );
}

// ─── Hint chip ────────────────────────────────────────────────────────────────

function HintChip({
  prefix,
  label,
  colors,
  compact = false,
}: {
  prefix: string;
  label: string;
  colors: Record<string, string>;
  compact?: boolean;
}) {
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: compact ? "5px" : "3px",
        fontSize: compact ? "10.5px" : "11px",
        color: compact ? `${colors.text}D0` : colors.sub,
        lineHeight: 1.35,
        userSelect: "none",
        marginRight: compact ? "0" : "4px",
      }}
    >
      <kbd
        style={{
          fontFamily: "'Cascadia Code', 'JetBrains Mono', 'Fira Code', 'Consolas', monospace",
          fontSize: compact ? "9.5px" : "10px",
          background: colors.surface,
          color: colors.accent,
          padding: compact ? "2px 6px" : "1px 5px",
          borderRadius: compact ? "6px" : "4px",
          border: `1px solid ${colors.surface2}`,
          lineHeight: 1.6,
          whiteSpace: "nowrap",
        }}
      >
        {prefix}
      </kbd>
      <span style={{ opacity: compact ? 0.86 : 0.7 }}>{label}</span>
    </span>
  );
}

// ─── Tiny inline spinner ──────────────────────────────────────────────────────

function LoadingSpinner({ color }: { color: string }) {
  return (
    <span
      style={{
        display: "inline-block",
        width: "15px",
        height: "15px",
        border: `2px solid transparent`,
        borderTopColor: color,
        borderRadius: "50%",
        animation: "omni-spin 0.7s linear infinite",
        verticalAlign: "middle",
        flexShrink: 0,
      }}
    />
  );
}
