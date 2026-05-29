import { useRef, useEffect, useState, RefObject } from "react";
import { isAiPrefix } from "../utils/aiPrefix";

interface Props {
  value: string;
  onChange: (v: string) => void;
  onSubmit: (v: string, forceAi: boolean) => void;
  isAiMode: boolean;
  loading: boolean;
  /** Number of prompts queued behind the current one. */
  queueDepth?: number;
  /** Called when the user clicks the spinner / Stop button while loading. */
  onCancel?: () => void;
  colors: Record<string, string>;
  onSettingsClick: () => void;
  /** Show the one-line hint bar at the bottom of an empty launcher input */
  showHintBar?: boolean;
  /** Render the empty launcher as a centered card */
  compact?: boolean;
  /** External ref forwarded from App so App can imperatively focus */
  inputRef?: RefObject<HTMLInputElement | HTMLTextAreaElement>;
  onHintBarExpandedChange?: (expanded: boolean) => void;
  inputHistory?: string[];
  historyIdx?: number;
  onHistoryNavigate?: (idx: number, value: string) => void;
  /** Current resolved theme for the toggle icon */
  resolvedTheme?: "dark" | "light";
  /** Called when the user clicks the theme toggle button */
  onThemeToggle?: () => void;
}

// Non-AI launcher prefixes shown in the idle hint bar.
const HINT_LOCAL = [
  { key: "* / f / open", label: "files" },
  { key: "=", label: "calc" },
  { key: ">", label: "shell" },
  { key: "cb", label: "clipboard" },
  { key: "bm / b", label: "bookmarks" },
  { key: "color", label: "color" },
  { key: "env", label: "env" },
  { key: "git", label: "git" },
  { key: "hosts", label: "hosts" },
  { key: "net", label: "network" },
  { key: "plugins / pm", label: "plugins" },
  { key: "ps", label: "processes" },
  { key: "settings", label: "windows" },
  { key: "snip", label: "snippets" },
  { key: "sys", label: "system" },
  { key: "timer", label: "timer" },
  { key: "todo", label: "todo" },
  { key: "conv", label: "units" },
];

// Web search prefixes (shown in a second row).
const HINT_SEARCH = [
  { key: "g", label: "Google" },
  { key: "yt / youtube", label: "YouTube" },
  { key: "gh / github", label: "GitHub" },
  { key: "wiki", label: "Wikipedia" },
  { key: "maps", label: "Maps" },
  { key: "so / stackoverflow", label: "StackOverflow" },
  { key: "ddg / duckduckgo", label: "DuckDuckGo" },
  { key: "bing", label: "Bing" },
  { key: "image", label: "Images" },
  { key: "lucky", label: "Feeling Lucky" },
  { key: "translate", label: "Translate" },
  { key: "ytmusic", label: "YT Music" },
  { key: "netflix", label: "Netflix" },
  { key: "gist", label: "Gist" },
  { key: "wolframalpha", label: "Wolfram" },
  { key: "gmail", label: "Gmail" },
  { key: "drive", label: "Drive" },
  { key: "facebook", label: "Facebook" },
  { key: "twitter", label: "X/Twitter" },
];

const PRIMARY_HINT_LOCAL_KEYS = new Set([
  "* / f / open",
  "=",
  ">",
  "cb",
  "bm / b",
  "git",
  "plugins / pm",
  "ps",
]);

const PRIMARY_HINT_SEARCH_KEYS = new Set([
  "g",
  "yt / youtube",
  "gh / github",
  "wiki",
  "maps",
  "translate",
]);

const PRIMARY_HINT_LOCAL = HINT_LOCAL.filter(({ key }) =>
  PRIMARY_HINT_LOCAL_KEYS.has(key),
);

const PRIMARY_HINT_SEARCH = HINT_SEARCH.filter(({ key }) =>
  PRIMARY_HINT_SEARCH_KEYS.has(key),
);

export default function SearchBar({
  value,
  onChange,
  onSubmit,
  isAiMode,
  loading,
  queueDepth = 0,
  onCancel,
  colors,
  onSettingsClick,
  showHintBar = false,
  compact = false,
  inputRef: externalRef,
  onHintBarExpandedChange,
  inputHistory,
  historyIdx,
  onHistoryNavigate,
  resolvedTheme,
  onThemeToggle,
}: Props) {
  const internalRef = useRef<HTMLInputElement | HTMLTextAreaElement>(null);
  const inputRef = externalRef ?? internalRef;
  const [showAllHints, setShowAllHints] = useState(false);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Re-focus whenever AI mode changes
  useEffect(() => {
    inputRef.current?.focus();
  }, [isAiMode]);

  useEffect(() => {
    if (!showHintBar) {
      setShowAllHints(false);
    }
  }, [showHintBar]);

  // Auto-resize textarea (AI mode) based on content
  useEffect(() => {
    const el = inputRef.current;
    if (el && el.tagName === "TEXTAREA") {
      const ta = el as HTMLTextAreaElement;
      ta.style.height = "auto";
      ta.style.height = Math.min(ta.scrollHeight, 160) + "px";
    }
  }, [value, isAiMode]);

  useEffect(() => {
    onHintBarExpandedChange?.(showHintBar && showAllHints);
  }, [showAllHints, showHintBar, onHintBarExpandedChange]);

  const isAI = isAiPrefix(value);
  const placeholder = isAiMode
    ? "Ask AI anything…"
    : "Type to launch, search, calculate…";
  const localHints = showAllHints ? HINT_LOCAL : PRIMARY_HINT_LOCAL;
  const searchHints = showAllHints ? HINT_SEARCH : PRIMARY_HINT_SEARCH;
  const hasCollapsedHints =
    PRIMARY_HINT_LOCAL.length < HINT_LOCAL.length ||
    PRIMARY_HINT_SEARCH.length < HINT_SEARCH.length;
  const wrapHints = !compact || showAllHints;

  return (
    <>
      <style>{`
        .omni-input-wrap {
          transition: box-shadow 180ms ease, border-color 180ms ease;
        }
        .omni-input-wrap:focus-within {
          border-color: ${colors.accent}88 !important;
          box-shadow: 0 0 0 3px ${colors.accent}22, 0 8px 28px rgba(0, 0, 0, 0.28), inset 0 1px 0 rgba(255,255,255,0.04);
        }
        .omni-ai-textarea::placeholder { color: ${colors.text}66; }
        .omni-ai-textarea::-webkit-scrollbar { width: 6px; }
        .omni-ai-textarea::-webkit-scrollbar-thumb {
          background: ${colors.surface2};
          border-radius: 999px;
        }
        @keyframes omni-tagline-fadein {
          from { opacity: 0; transform: translateY(4px); }
          to   { opacity: 1; transform: translateY(0); }
        }
        @keyframes omni-hint-fadein {
          from { opacity: 0; transform: translateY(3px); }
          to   { opacity: 1; transform: translateY(0); }
        }
        .omni-hint-row::-webkit-scrollbar {
          height: 4px;
        }
        .omni-hint-row::-webkit-scrollbar-thumb {
          background: ${colors.surface2};
          border-radius: 999px;
        }
      `}</style>

      <div
        style={{
          flexShrink: 0,
          padding: isAiMode ? "14px 16px 18px" : 0,
          background: isAiMode
            ? `linear-gradient(to top, ${colors.bg} 70%, ${colors.bg}00)`
            : "transparent",
        }}
      >
        {!isAiMode && (
          <div
            style={{
              padding: compact ? "0 2px 12px" : "12px 16px 0",
              animation: "omni-tagline-fadein 240ms ease both",
              textAlign: compact ? "center" : "left",
            }}
          >
            <div
              style={{
                color: colors.text,
                fontSize: compact ? "16px" : "13px",
                fontWeight: 800,
                letterSpacing: compact ? "0.04em" : "0.02em",
                lineHeight: 1.2,
                opacity: compact ? 0.9 : 1,
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
            alignItems: isAiMode ? "flex-end" : "center",
            padding: isAiMode ? "8px 12px 8px 16px" : compact ? "0 18px" : "0 14px",
            height: isAiMode ? "auto" : compact ? "54px" : "56px",
            minHeight: isAiMode ? "52px" : undefined,
            width: compact ? "min(94%, 820px)" : "100%",
            margin: compact ? "0 auto" : undefined,
            gap: "10px",
            boxSizing: "border-box",
            border: `1px solid ${colors.surface2}`,
            background: isAiMode ? `${colors.surface}80` : colors.bg,
            backdropFilter: isAiMode ? "blur(10px)" : undefined,
            WebkitBackdropFilter: isAiMode ? "blur(10px)" : undefined,
            borderRadius: isAiMode ? "18px" : compact ? "16px" : "14px",
            boxShadow: isAiMode
              ? "0 8px 28px rgba(0, 0, 0, 0.28), inset 0 1px 0 rgba(255,255,255,0.04)"
              : compact
                ? "0 18px 44px rgba(0, 0, 0, 0.24), inset 0 1px 0 rgba(255,255,255,0.03)"
                : "none",
          }}
        >
          {/* Leading icon / spinner (clickable when loading to cancel the request) */}
          <span
            onClick={loading && onCancel ? onCancel : undefined}
            title={loading ? "Stop request" : undefined}
            style={{
              fontSize: compact ? "18px" : "17px",
              opacity: loading ? 1 : isAiMode ? 0.9 : compact ? 0.82 : 0.5,
              color: isAiMode ? colors.accent : compact ? colors.text : undefined,
              flexShrink: 0,
              lineHeight: 1,
              display: "flex",
              alignItems: "center",
              alignSelf: isAiMode ? "center" : undefined,
              paddingBottom: isAiMode ? "4px" : 0,
              cursor: loading && onCancel ? "pointer" : "default",
            }}
          >
            {loading ? (
              <StopGlyph color={colors.accent} />
            ) : isAI ? (
              "✦"
            ) : (
              "⌕"
            )}
          </span>

          {/* Queue depth badge */}
          {loading && queueDepth > 0 && (
            <span
              style={{
                fontSize: "10px",
                background: `${colors.accent}30`,
                color: colors.accent,
                padding: "1px 5px",
                borderRadius: "4px",
                fontWeight: 600,
                flexShrink: 0,
                marginLeft: "4px",
                lineHeight: 1,
              }}
            >
              +{queueDepth}
            </span>
          )}

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
          {isAiMode ? (
            <textarea
              autoFocus
              className="omni-ai-textarea"
              ref={inputRef as RefObject<HTMLTextAreaElement>}
              value={value}
              onChange={(e) => onChange(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  onSubmit(value, e.ctrlKey || e.metaKey);
                  return;
                }
                // Shell-style history navigation. Only steal the arrow key
                // when (a) the draft is empty so the caret has nowhere to
                // go, or (b) we're already cycling through history.
                const inHistory = (historyIdx ?? -1) >= 0;
                if (
                  e.key === "ArrowUp" &&
                  !e.shiftKey &&
                  !e.altKey &&
                  (value === "" || inHistory)
                ) {
                  e.preventDefault();
                  const newIdx = Math.min(
                    (historyIdx ?? -1) + 1,
                    (inputHistory?.length ?? 0) - 1,
                  );
                  if (newIdx >= 0 && inputHistory && inputHistory[newIdx]) {
                    onHistoryNavigate?.(newIdx, inputHistory[newIdx]);
                  }
                  return;
                }
                if (
                  e.key === "ArrowDown" &&
                  !e.shiftKey &&
                  !e.altKey &&
                  inHistory
                ) {
                  e.preventDefault();
                  const newIdx = (historyIdx ?? 0) - 1;
                  if (newIdx < 0) {
                    onHistoryNavigate?.(-1, "");
                  } else if (inputHistory && inputHistory[newIdx]) {
                    onHistoryNavigate?.(newIdx, inputHistory[newIdx]);
                  }
                }
              }}
              placeholder={placeholder}
              rows={1}
              style={{
                flex: 1,
                background: "transparent",
                border: "none",
                outline: "none",
                fontSize: "15px",
                color: colors.text,
                caretColor: colors.accent,
                fontFamily: "inherit",
                fontWeight: 400,
                letterSpacing: 0,
                resize: "none",
                lineHeight: 1.45,
                maxHeight: "160px",
                overflowY: "auto",
                padding: "6px 4px",
                alignSelf: "stretch",
              }}
            />
          ) : (
          <input
            autoFocus
            ref={inputRef as RefObject<HTMLInputElement>}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                onSubmit(value, e.ctrlKey || e.metaKey);
              }
              if (e.key === "ArrowUp" && (value === "" || (historyIdx ?? -1) >= 0)) {
                e.preventDefault();
                const newIdx = Math.min((historyIdx ?? -1) + 1, (inputHistory?.length ?? 0) - 1);
                if (newIdx >= 0 && inputHistory && inputHistory[newIdx]) {
                  onHistoryNavigate?.(newIdx, inputHistory[newIdx]);
                }
              }
              if (e.key === "ArrowDown" && (historyIdx ?? -1) >= 0) {
                e.preventDefault();
                const newIdx = (historyIdx ?? 0) - 1;
                if (newIdx < 0) {
                  onHistoryNavigate?.(-1, "");
                } else if (inputHistory && inputHistory[newIdx]) {
                  onHistoryNavigate?.(newIdx, inputHistory[newIdx]);
                }
              }
            }}
            placeholder={placeholder}
            style={{
              flex: 1,
              background: "transparent",
              border: "none",
              outline: "none",
              fontSize: compact ? "15.5px" : "16px",
              color: colors.text,
              caretColor: colors.accent,
              fontFamily: "inherit",
              fontWeight: compact ? 500 : 400,
              letterSpacing: 0,
            }}
          />
          )}

          {/* AI mode badge (right side when fully in AI mode) */}
          {isAiMode && (
            <span
              style={{
                display: "inline-flex",
                alignItems: "center",
                justifyContent: "center",
                height: "30px",
                fontSize: "11px",
                background: `${colors.accent}22`,
                color: colors.accent,
                padding: "0 10px",
                borderRadius: "8px",
                fontWeight: 600,
                letterSpacing: "0.04em",
                flexShrink: 0,
                border: `1px solid ${colors.accent}33`,
                boxSizing: "border-box",
              }}
            >
              AI
            </span>
          )}

          {isAiMode && loading && onCancel && (
            <button
              type="button"
              onClick={onCancel}
              aria-label="Cancel running AI request"
              title="Cancel running request"
              style={{
                display: "inline-flex",
                alignItems: "center",
                justifyContent: "center",
                gap: "6px",
                minHeight: "30px",
                padding: "5px 10px",
                borderRadius: "8px",
                border: `1px solid ${colors.accent}55`,
                background: `${colors.accent}20`,
                color: colors.accent,
                fontSize: "12px",
                fontWeight: 700,
                letterSpacing: 0,
                cursor: "pointer",
                flexShrink: 0,
              }}
            >
              <span style={{ fontSize: "12px", lineHeight: 1 }}>■</span>
              Cancel
            </button>
          )}

          {/* Theme toggle button */}
          {onThemeToggle && (
            <ThemeToggleButton
              resolvedTheme={resolvedTheme}
              colors={colors}
              onThemeToggle={onThemeToggle}
            />
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
              padding: compact ? "8px 2px 0" : "4px 16px 8px",
              width: compact ? "min(94%, 820px)" : "100%",
              margin: compact ? "0 auto" : undefined,
              animation: "omni-hint-fadein 240ms ease both",
            }}
          >
            {/* Core prefixes row */}
            <div
              className="omni-hint-row"
              style={{
                display: "flex",
                justifyContent: compact ? "flex-start" : "flex-start",
                gap: compact ? "5px" : "4px",
                flexWrap: wrapHints ? "wrap" : "nowrap",
                overflowX: wrapHints ? "visible" : "auto",
                overflowY: "hidden",
                paddingBottom: compact && !wrapHints ? "4px" : 0,
                scrollbarWidth: compact && !wrapHints ? "thin" : undefined,
                scrollbarColor: compact && !wrapHints
                  ? `${colors.surface2} transparent`
                  : undefined,
                marginBottom: compact ? "4px" : "4px",
              }}
            >
              {localHints.map(({ key, label }) => (
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
              className="omni-hint-row"
              style={{
                display: "flex",
                justifyContent: compact ? "flex-start" : "flex-start",
                gap: compact ? "5px" : "4px",
                flexWrap: wrapHints ? "wrap" : "nowrap",
                overflowX: wrapHints ? "visible" : "auto",
                overflowY: "hidden",
                paddingBottom: compact && !wrapHints ? "4px" : 0,
                scrollbarWidth: compact && !wrapHints ? "thin" : undefined,
                scrollbarColor: compact && !wrapHints
                  ? `${colors.surface2} transparent`
                  : undefined,
              }}
            >
              {searchHints.map(({ key, label }) => (
                <HintChip
                  key={key}
                  prefix={key}
                  label={label}
                  colors={colors}
                  compact={compact}
                />
              ))}
            </div>
            {hasCollapsedHints && (
              <div
                style={{
                  display: "flex",
                  justifyContent: compact ? "center" : "flex-start",
                  paddingTop: compact ? "4px" : "8px",
                }}
              >
                <button
                  type="button"
                  onClick={() => setShowAllHints((current) => !current)}
                  style={{
                    background: `${colors.surface}CC`,
                    border: `1px solid ${colors.surface2}`,
                    color: colors.text,
                    borderRadius: compact ? "999px" : "8px",
                    padding: compact ? "3px 10px" : "4px 9px",
                    fontSize: compact ? "10.5px" : "11px",
                    lineHeight: 1.2,
                    cursor: "pointer",
                    opacity: 0.82,
                  }}
                  onMouseEnter={(e) => (e.currentTarget.style.opacity = "1")}
                  onMouseLeave={(e) => (e.currentTarget.style.opacity = "0.82")}
                >
                  {showAllHints ? "Show fewer hints" : "Show all hints"}
                </button>
              </div>
            )}
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
        flexShrink: 0,
        gap: compact ? "4px" : "3px",
        fontSize: compact ? "10px" : "11px",
        color: compact ? `${colors.text}D0` : colors.sub,
        lineHeight: 1.25,
        userSelect: "none",
        marginRight: compact ? "0" : "4px",
      }}
    >
      <kbd
        style={{
          fontFamily: "'Cascadia Code', 'JetBrains Mono', 'Fira Code', 'Consolas', monospace",
          fontSize: compact ? "9px" : "10px",
          background: colors.surface,
          color: colors.accent,
          padding: compact ? "1px 6px" : "1px 5px",
          borderRadius: compact ? "5px" : "4px",
          border: `1px solid ${colors.surface2}`,
          lineHeight: 1.55,
          whiteSpace: "nowrap",
        }}
      >
        {prefix}
      </kbd>
      <span style={{ opacity: compact ? 0.86 : 0.7 }}>{label}</span>
    </span>
  );
}

// ─── Theme toggle button with spin animation ──────────────────────────────────

function ThemeToggleButton({
  resolvedTheme,
  colors,
  onThemeToggle,
}: {
  resolvedTheme?: "dark" | "light";
  colors: Record<string, string>;
  onThemeToggle: () => void;
}) {
  const [spinning, setSpinning] = useState(false);

  const handleClick = () => {
    setSpinning(true);
    onThemeToggle();
    setTimeout(() => setSpinning(false), 380);
  };

  return (
    <button
      onClick={handleClick}
      className={spinning ? "theme-toggle-animate" : ""}
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
        display: "inline-flex",
        alignItems: "center",
      }}
      onMouseEnter={(e) => (e.currentTarget.style.opacity = "0.75")}
      onMouseLeave={(e) => (e.currentTarget.style.opacity = "0.4")}
      title={resolvedTheme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
    >
      {resolvedTheme === "dark" ? "☀" : "🌙"}
    </button>
  );
}

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

// Spinner with a small stop-square overlay on hover; click cancels the request.
function StopGlyph({ color }: { color: string }) {
  const [hovered, setHovered] = useState(false);
  return (
    <span
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        position: "relative",
        display: "inline-flex",
        width: "15px",
        height: "15px",
        alignItems: "center",
        justifyContent: "center",
        flexShrink: 0,
      }}
    >
      {hovered ? (
        <span
          style={{
            width: "10px",
            height: "10px",
            background: color,
            borderRadius: "2px",
          }}
        />
      ) : (
        <LoadingSpinner color={color} />
      )}
    </span>
  );
}
