import { useState, useEffect, useCallback } from "react";
import FormattedSubtitle from "./FormattedSubtitle";

interface QueryResult {
  id: string;
  title: string;
  subtitle?: string;
  icon?: string;
  score: number;
  action_type: string;
  action_data: string;
}

interface Props {
  results: QueryResult[];
  query: string;
  onExecute: (r: QueryResult) => void;
  colors: Record<string, string>;
}

const ACTION_BADGE: Record<string, string> = {
  open: "↵ Open",
  url: "↵ Open",
  shell: "↵ Run",
  copy: "↵ Copy",
  help_command: "↵ Use",
};

export default function ResultList({
  results,
  query,
  onExecute,
  colors,
}: Props) {
  const [selected, setSelected] = useState(0);
  const [hovered, setHovered] = useState(-1);

  useEffect(() => {
    setSelected(0);
  }, [results]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelected((s) => Math.min(s + 1, results.length - 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelected((s) => Math.max(s - 1, 0));
      } else if (e.key === "Enter") {
        if (results[selected]) onExecute(results[selected]);
      }
    },
    [results, selected, onExecute],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  function highlight(text: string, q: string): string {
    if (!q) return text;
    const idx = text.toLowerCase().indexOf(q.toLowerCase());
    if (idx === -1) return text;
    return (
      text.slice(0, idx) +
      "<mark>" +
      text.slice(idx, idx + q.length) +
      "</mark>" +
      text.slice(idx + q.length)
    );
  }

  // Keyboard shortcut labels for first 9 results
  function kbdHint(i: number): string {
    if (i < 9) return `⌘${i + 1}`;
    return "";
  }

  return (
    <div
      style={{
        overflowY: "auto",
        maxHeight: "400px",
        scrollbarWidth: "thin",
        scrollbarColor: `${colors.surface2} transparent`,
      }}
    >
      {results.map((r, i) => {
        const isSelected = i === selected;
        const isHovered = i === hovered;
        const highlighted = isSelected || isHovered;

        return (
          <div
            key={r.id}
            onClick={() => onExecute(r)}
            onMouseEnter={() => {
              setHovered(i);
              setSelected(i);
            }}
            onMouseLeave={() => setHovered(-1)}
            style={{
              display: "flex",
              alignItems: "center",
              gap: "12px",
              padding: "8px 14px",
              cursor: "pointer",
              transition: "background 150ms ease, transform 120ms ease",
              background: highlighted ? `${colors.surface}CC` : "transparent",
              transform: highlighted ? "translateX(2px)" : "translateX(0)",
              borderLeft: isSelected
                ? `3px solid ${colors.accent}`
                : "3px solid transparent",
              // Staggered fade-in
              animation: `omni-fade-in 180ms ease both`,
              animationDelay: `${i * 25}ms`,
            }}
          >
            {/* Icon */}
            <span
              style={{
                fontSize: "18px",
                width: "22px",
                textAlign: "center",
                flexShrink: 0,
                lineHeight: 1,
              }}
            >
              {r.icon || "📄"}
            </span>

            {/* Title + subtitle */}
            <div style={{ flex: 1, minWidth: 0 }}>
              <div
                style={{
                  fontSize: "14px",
                  fontWeight: 500,
                  color: isSelected ? colors.accent : colors.text,
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  transition: "color 150ms",
                }}
                dangerouslySetInnerHTML={{ __html: highlight(r.title, query) }}
              />
              {r.subtitle && (
                <FormattedSubtitle
                  text={r.subtitle}
                  color={colors.sub}
                />
              )}
            </div>

            {/* Right-side: keyboard hint + action badge */}
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: "8px",
                flexShrink: 0,
              }}
            >
              {kbdHint(i) && (
                <span
                  style={{
                    fontSize: "10px",
                    color: colors.sub,
                    fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
                    opacity: 0.55,
                  }}
                >
                  {kbdHint(i)}
                </span>
              )}
              <span
                style={{
                  fontSize: "11px",
                  color: isSelected ? colors.accent : colors.sub,
                  background: isSelected ? `${colors.accent}18` : "transparent",
                  padding: "2px 7px",
                  borderRadius: "5px",
                  fontWeight: 500,
                  transition: "color 150ms, background 150ms",
                  border: isSelected
                    ? `1px solid ${colors.accent}33`
                    : "1px solid transparent",
                }}
              >
                {ACTION_BADGE[r.action_type] ?? "↵"}
              </span>
            </div>
          </div>
        );
      })}
    </div>
  );
}
