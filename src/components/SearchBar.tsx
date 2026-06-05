import { useRef, useEffect, useState, RefObject } from "react";
import { isAiPrefix } from "../launcherConfig";

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
  onSettingsClick: () => void;
  /** Render the empty launcher as a centered card */
  compact?: boolean;
  /** External ref forwarded from App so App can imperatively focus */
  inputRef?: RefObject<HTMLInputElement | HTMLTextAreaElement>;
  inputHistory?: string[];
  historyIdx?: number;
  onHistoryNavigate?: (idx: number, value: string) => void;
  /** Current resolved theme for the toggle icon */
  resolvedTheme?: "dark" | "light";
  /** Called when the user clicks the theme toggle button */
  onThemeToggle?: () => void;
}

// ─── Layout sizing tokens ─────────────────────────────────────────────────────
//
// SearchBar renders in three visual modes:
//   • ai       — AI thread is open, input lives in the bottom dock
//   • stacked  — launcher splash on a blank screen (centered, large)
//   • inline   — launcher with results below it (compact bar at top)
//
// All "magic" pixel values are collected here so designers/tweakers only
// touch one table instead of hunting through nested ternaries in JSX.
type LayoutMode = "ai" | "stacked" | "inline";

interface ModeSizes {
  wrapPadding: string;
  wrapHeight: string | undefined;
  wrapMinHeight: string | undefined;
  wrapWidth: string;
  wrapMargin: string | undefined;
  wrapRadius: string;
  wrapShadow: string;
  inputFontSize: string;
  inputFontWeight: number;
  iconFontSize: string;
  iconOpacity: number;
  taglinePadding: string;
  taglineFontSize: string;
  taglineLetterSpacing: string;
  taglineGap: string;
  taglineOpacity: number;
}

const AI_SHADOW =
  "0 8px 28px rgba(0, 0, 0, 0.28), inset 0 1px 0 rgba(255,255,255,0.04)";
const STACKED_SHADOW =
  "0 18px 44px rgba(0, 0, 0, 0.24), inset 0 1px 0 rgba(255,255,255,0.03)";
const COMPACT_WIDTH = "min(94%, 820px)";

const SIZES: Record<LayoutMode, ModeSizes> = {
  ai: {
    wrapPadding: "8px 12px 8px 16px",
    wrapHeight: "auto",
    wrapMinHeight: "52px",
    wrapWidth: "100%",
    wrapMargin: undefined,
    wrapRadius: "18px",
    wrapShadow: AI_SHADOW,
    inputFontSize: "15px",
    inputFontWeight: 400,
    iconFontSize: "17px",
    iconOpacity: 0.9,
    taglinePadding: "12px 16px 0",
    taglineFontSize: "13px",
    taglineLetterSpacing: "0.02em",
    taglineGap: "6px",
    taglineOpacity: 1,
  },
  stacked: {
    wrapPadding: "0 18px",
    wrapHeight: "54px",
    wrapMinHeight: undefined,
    wrapWidth: COMPACT_WIDTH,
    wrapMargin: "0 auto",
    wrapRadius: "16px",
    wrapShadow: STACKED_SHADOW,
    inputFontSize: "15.5px",
    inputFontWeight: 500,
    iconFontSize: "18px",
    iconOpacity: 0.82,
    taglinePadding: "0 2px 12px",
    taglineFontSize: "16px",
    taglineLetterSpacing: "0.04em",
    taglineGap: "8px",
    taglineOpacity: 0.9,
  },
  inline: {
    wrapPadding: "0 14px",
    wrapHeight: "56px",
    wrapMinHeight: undefined,
    wrapWidth: "100%",
    wrapMargin: undefined,
    wrapRadius: "14px",
    wrapShadow: "none",
    inputFontSize: "16px",
    inputFontWeight: 400,
    iconFontSize: "17px",
    iconOpacity: 0.5,
    taglinePadding: "12px 16px 0",
    taglineFontSize: "13px",
    taglineLetterSpacing: "0.02em",
    taglineGap: "6px",
    taglineOpacity: 1,
  },
};

export default function SearchBar({
  value,
  onChange,
  onSubmit,
  isAiMode,
  loading,
  queueDepth = 0,
  onCancel,
  onSettingsClick,
  compact = false,
  inputRef: externalRef,
  inputHistory,
  historyIdx,
  onHistoryNavigate,
  resolvedTheme,
  onThemeToggle,
}: Props) {
  const internalRef = useRef<HTMLInputElement | HTMLTextAreaElement>(null);
  const inputRef = externalRef ?? internalRef;

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Re-focus whenever AI mode changes
  useEffect(() => {
    inputRef.current?.focus();
  }, [isAiMode]);

  // Auto-resize textarea (AI mode) based on content
  useEffect(() => {
    const el = inputRef.current;
    if (el && el.tagName === "TEXTAREA") {
      const ta = el as HTMLTextAreaElement;
      ta.style.height = "auto";
      ta.style.height = Math.min(ta.scrollHeight, 160) + "px";
    }
  }, [value, isAiMode]);

  const isAI = isAiPrefix(value);
  const mode: LayoutMode = isAiMode ? "ai" : compact ? "stacked" : "inline";
  const s = SIZES[mode];
  const placeholder = isAiMode
    ? "Ask AI anything…  (Shift+Enter for newline)"
    : "Type to launch, search, calculate…";

  return (
    <div
      style={{
        flexShrink: 0,
        padding: isAiMode ? "14px 16px 18px" : 0,
        background: isAiMode
          ? `linear-gradient(to top, var(--bg) 70%, transparent)`
          : "transparent",
      }}
    >
      {!isAiMode && (
        <div
          style={{
            padding: s.taglinePadding,
            animation: "omni-tagline-fadein 240ms ease both",
            textAlign: compact ? "center" : "left",
          }}
        >
          <div
            style={{
              color: "var(--text)",
              fontSize: s.taglineFontSize,
              fontWeight: 800,
              letterSpacing: s.taglineLetterSpacing,
              lineHeight: 1.2,
              opacity: s.taglineOpacity,
              display: "inline-flex",
              alignItems: "center",
              gap: s.taglineGap,
            }}
          >
            {compact && (
              <span
                aria-hidden
                style={{
                  color: "var(--accent)",
                  fontSize: "15px",
                  lineHeight: 1,
                }}
              >
                ✦
              </span>
            )}
            <span>OMNILAUNCHER</span>
          </div>
        </div>
      )}

      {/* ── Main input row ─────────────────────────────────────────── */}
      <div
        className="omni-input-wrap"
        style={{
          display: "flex",
          alignItems: isAiMode ? "flex-end" : "center",
          padding: s.wrapPadding,
          height: s.wrapHeight,
          minHeight: s.wrapMinHeight,
          width: s.wrapWidth,
          margin: s.wrapMargin,
          gap: "10px",
          boxSizing: "border-box",
          border: "1px solid var(--surface-2)",
          background: isAiMode
            ? "color-mix(in srgb, var(--surface) 50%, transparent)"
            : "var(--bg)",
          backdropFilter: isAiMode ? "blur(10px)" : undefined,
          WebkitBackdropFilter: isAiMode ? "blur(10px)" : undefined,
          borderRadius: s.wrapRadius,
          boxShadow: s.wrapShadow,
        }}
      >
        {/* Leading icon / spinner (clickable when loading to cancel the request) */}
        {loading ? (
          <button
            type="button"
            onClick={onCancel}
            className="omni-stop-glyph"
            aria-label="Stop request"
            title="Stop request"
            style={{
              background: "transparent",
              border: "none",
              padding: 0,
              color: "var(--accent)",
              alignSelf: isAiMode ? "center" : undefined,
              marginBottom: isAiMode ? "4px" : 0,
            }}
          >
            <span className="omni-stop-glyph__spinner">
              <LoadingSpinner color={"var(--accent)"} />
            </span>
            <span
              className="omni-stop-glyph__square"
              style={{
                background: "var(--accent)",
                width: "10px",
                height: "10px",
                borderRadius: "2px",
              }}
            />
          </button>
        ) : (
          <span
            aria-hidden
            style={{
              fontSize: s.iconFontSize,
              opacity: s.iconOpacity,
              color: isAiMode
                ? "var(--accent)"
                : compact
                  ? "var(--text)"
                  : undefined,
              flexShrink: 0,
              lineHeight: 1,
              display: "flex",
              alignItems: "center",
              alignSelf: isAiMode ? "center" : undefined,
              paddingBottom: isAiMode ? "4px" : 0,
            }}
          >
            {isAI ? "✦" : "⌕"}
          </span>
        )}

        {/* Queue depth badge */}
        {loading && queueDepth > 0 && (
          <span
            className="omni-badge omni-badge--outline"
            title={`${queueDepth} queued`}
          >
            +{queueDepth}
          </span>
        )}

        {/* AI badge (shown inside left of input when "?" prefix is typed) */}
        {isAI && !isAiMode && <span className="omni-badge">AI</span>}

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
              color: "var(--text)",
              caretColor: "var(--accent)",
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
              if (
                e.key === "ArrowUp" &&
                (value === "" || (historyIdx ?? -1) >= 0)
              ) {
                e.preventDefault();
                const newIdx = Math.min(
                  (historyIdx ?? -1) + 1,
                  (inputHistory?.length ?? 0) - 1,
                );
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
              fontSize: s.inputFontSize,
              color: "var(--text)",
              caretColor: "var(--accent)",
              fontFamily: "inherit",
              fontWeight: s.inputFontWeight,
              letterSpacing: 0,
            }}
          />
        )}

        {/* AI mode badge (right side when fully in AI mode) */}
        {isAiMode && (
          <span className="omni-badge omni-badge--ai-large">AI</span>
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
              border:
                "1px solid color-mix(in srgb, var(--accent) 35%, transparent)",
              background: "color-mix(in srgb, var(--accent) 18%, transparent)",
              color: "var(--accent)",
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
            onThemeToggle={onThemeToggle}
          />
        )}

        {/* Settings button */}
        <button
          type="button"
          onClick={onSettingsClick}
          className="omni-icon-btn"
          title="Settings (Ctrl+,)"
          aria-label="Open settings"
        >
          ⚙
        </button>
      </div>
    </div>
  );
}

// ─── Theme toggle button with spin animation ──────────────────────────────────

function ThemeToggleButton({
  resolvedTheme,
  onThemeToggle,
}: {
  resolvedTheme?: "dark" | "light";
  onThemeToggle: () => void;
}) {
  const [spinning, setSpinning] = useState(false);

  const handleClick = () => {
    setSpinning(true);
    onThemeToggle();
    setTimeout(() => setSpinning(false), 380);
  };

  const isDark = resolvedTheme === "dark";

  return (
    <button
      type="button"
      onClick={handleClick}
      className={"omni-icon-btn" + (spinning ? " theme-toggle-animate" : "")}
      title={isDark ? "Switch to light mode" : "Switch to dark mode"}
      aria-label={isDark ? "Switch to light mode" : "Switch to dark mode"}
    >
      <ThemeGlyph isDark={isDark} />
    </button>
  );
}

// Inline SVG so the glyph renders identically on every OS (no emoji font drift).
function ThemeGlyph({ isDark }: { isDark: boolean }) {
  if (isDark) {
    // Sun
    return (
      <svg
        width="15"
        height="15"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
      >
        <circle cx="12" cy="12" r="4" />
        <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41" />
      </svg>
    );
  }
  // Moon
  return (
    <svg
      width="15"
      height="15"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
    </svg>
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
