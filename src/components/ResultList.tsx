import React, { useState, useEffect, useCallback, useRef, useMemo } from "react";
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
}

const ACTION_BADGE: Record<string, string> = {
  open: "↵ Open",
  url: "↵ Open",
  shell: "↵ Run",
  copy: "↵ Copy",
  help_command: "↵ Use",
};

// Detect mac so we show ⌘ vs Ctrl in the kbd hints.
const IS_MAC = typeof navigator !== "undefined"
  && /Mac|iPhone|iPad/i.test(navigator.platform || navigator.userAgent || "");
const MOD_KEY = IS_MAC ? "⌘" : "Ctrl+";

// Keyboard shortcut labels for first 9 results.
function kbdHint(i: number): string {
  if (i < 9) return `${MOD_KEY}${i + 1}`;
  return "";
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function highlight(text: string, q: string): string {
  if (!q) return escapeHtml(text);
  // First try exact substring (faster, cleaner output).
  const idx = text.toLowerCase().indexOf(q.toLowerCase());
  if (idx !== -1) {
    return (
      escapeHtml(text.slice(0, idx)) +
      "<mark>" + escapeHtml(text.slice(idx, idx + q.length)) + "</mark>" +
      escapeHtml(text.slice(idx + q.length))
    );
  }
  // Fuzzy: highlight individual matched chars in order.
  const lower = text.toLowerCase();
  const qLower = q.toLowerCase();
  let qi = 0;
  let out = "";
  for (let i = 0; i < text.length; i++) {
    const ch = escapeHtml(text[i]);
    if (qi < qLower.length && lower[i] === qLower[qi]) {
      out += "<mark>" + ch + "</mark>";
      qi++;
    } else {
      out += ch;
    }
  }
  if (qi === qLower.length) return out;
  return escapeHtml(text);
}

export default function ResultList({
  results,
  query,
  onExecute,
}: Props) {
  const [selected, setSelected] = useState(0);
  const [hovered, setHovered] = useState(-1);
  const [ctxMenu, setCtxMenu] = useState<
    { x: number; y: number; item: QueryResult } | null
  >(null);
  const [favorites, setFavorites] = useState<Set<string>>(() => {
    try {
      const stored = localStorage.getItem("omni-favorites");
      return new Set(stored ? JSON.parse(stored) : []);
    } catch {
      return new Set();
    }
  });

  const listRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<Array<HTMLDivElement | null>>([]);

  const toggleFavorite = (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setFavorites((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
        try {
          const stored = localStorage.getItem("omni-favorite-items");
          const all: QueryResult[] = stored ? JSON.parse(stored) : [];
          const item = results.find((r) => r.id === id);
          if (item && !all.find((r) => r.id === id)) {
            localStorage.setItem(
              "omni-favorite-items",
              JSON.stringify([...all, item]),
            );
          }
        } catch {}
      }
      try {
        localStorage.setItem("omni-favorites", JSON.stringify([...next]));
      } catch {}
      return next;
    });
  };

  // Reset selection when result set changes.
  useEffect(() => {
    setSelected(0);
  }, [results]);

  // Close context menu on any outside interaction.
  useEffect(() => {
    if (!ctxMenu) return;
    const handler = () => setCtxMenu(null);
    document.addEventListener("mousedown", handler);
    document.addEventListener("scroll", handler, true);
    window.addEventListener("blur", handler);
    return () => {
      document.removeEventListener("mousedown", handler);
      document.removeEventListener("scroll", handler, true);
      window.removeEventListener("blur", handler);
    };
  }, [ctxMenu]);

  // Keep selected row visible when arrowing through a long list.
  useEffect(() => {
    const el = itemRefs.current[selected];
    if (el && typeof el.scrollIntoView === "function") {
      el.scrollIntoView({ block: "nearest", behavior: "auto" });
    }
  }, [selected]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      // Cmd/Ctrl + 1..9 — direct invocation.
      const mod = IS_MAC ? e.metaKey : e.ctrlKey;
      if (mod && !e.shiftKey && !e.altKey && /^[1-9]$/.test(e.key)) {
        const i = parseInt(e.key, 10) - 1;
        if (results[i]) {
          e.preventDefault();
          onExecute(results[i]);
        }
        return;
      }
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelected((s) => Math.min(s + 1, results.length - 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelected((s) => Math.max(s - 1, 0));
      } else if (e.key === "Home") {
        e.preventDefault();
        setSelected(0);
      } else if (e.key === "End") {
        e.preventDefault();
        setSelected(Math.max(0, results.length - 1));
      } else if (e.key === "PageDown") {
        e.preventDefault();
        setSelected((s) => Math.min(s + 5, results.length - 1));
      } else if (e.key === "PageUp") {
        e.preventDefault();
        setSelected((s) => Math.max(s - 5, 0));
      } else if (e.key === "Enter") {
        if (results[selected]) onExecute(results[selected]);
      } else if (e.key === "Escape" && ctxMenu) {
        setCtxMenu(null);
      }
    },
    [results, selected, onExecute, ctxMenu],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  // Clamp context menu so it never overflows the viewport.
  const ctxStyle = useMemo<React.CSSProperties | null>(() => {
    if (!ctxMenu) return null;
    const PAD = 8;
    const W = 180;
    const H = 140;
    const maxX = window.innerWidth - W - PAD;
    const maxY = window.innerHeight - H - PAD;
    return {
      top: Math.max(PAD, Math.min(ctxMenu.y, maxY)),
      left: Math.max(PAD, Math.min(ctxMenu.x, maxX)),
    };
  }, [ctxMenu]);

  if (results.length === 0) {
    return (
      <div className="results-empty">
        <div className="results-empty__icon">🔍</div>
        <div className="results-empty__title">No results</div>
        <div className="results-empty__hint">
          Try a different query, or hit <kbd>?</kbd> to see available prefixes.
        </div>
      </div>
    );
  }

  const sel = results[selected];

  return (
    <>
      <div
        ref={listRef}
        className="results"
        role="listbox"
        aria-activedescendant={selected >= 0 ? `omni-opt-${selected}` : undefined}
      >
        {results.map((r, i) => {
          const isSelected = i === selected;
          const isHovered = i === hovered;
          const isFav = favorites.has(r.id);
          const kbd = kbdHint(i);

          return (
            <div
              key={r.id}
              ref={(el) => { itemRefs.current[i] = el; }}
              id={`omni-opt-${i}`}
              role="option"
              aria-selected={isSelected}
              className={`result-item${isSelected ? " result-item--selected" : ""}`}
              onClick={() => onExecute(r)}
              onContextMenu={(e) => {
                e.preventDefault();
                setCtxMenu({ x: e.clientX, y: e.clientY, item: r });
              }}
              onMouseEnter={() => {
                setHovered(i);
                setSelected(i);
              }}
              onMouseLeave={() => setHovered(-1)}
            >
              <span className="result-item__icon" aria-hidden="true">
                {r.icon || "📄"}
              </span>

              <div className="result-item__content">
                <div
                  className="result-item__title"
                  dangerouslySetInnerHTML={{ __html: highlight(r.title, query) }}
                />
                {r.subtitle && (
                  <FormattedSubtitle text={r.subtitle} color="var(--sub)" />
                )}
              </div>

              <div className="result-item__trailing">
                <button
                  type="button"
                  className={`result-item__star${isFav ? " result-item__star--on" : ""}`}
                  onClick={(e) => toggleFavorite(r.id, e)}
                  title={isFav ? "Remove favorite" : "Add favorite"}
                  aria-label={isFav ? "Remove favorite" : "Add favorite"}
                  aria-pressed={isFav}
                  tabIndex={-1}
                >
                  {isFav ? "★" : "☆"}
                </button>
                {kbd && (isHovered || isSelected) && (
                  <span className="result-item__kbd" aria-hidden="true">
                    {kbd}
                  </span>
                )}
                <span className="result-item__action-badge">
                  {ACTION_BADGE[r.action_type] ?? "↵"}
                </span>
              </div>
            </div>
          );
        })}
      </div>

      {/* Preview panel for selected item */}
      {sel && (sel.subtitle || sel.action_data) && (
        <div className="result-preview" aria-live="polite">
          <span className="result-preview__icon" aria-hidden="true">
            {sel.icon || "📄"}
          </span>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div className="result-preview__title">{sel.title}</div>
            {sel.subtitle ? (
              <div className="result-preview__body">{sel.subtitle}</div>
            ) : (
              <div className="result-preview__body result-preview__body--mono">
                {sel.action_data}
              </div>
            )}
          </div>
        </div>
      )}

      {ctxMenu && ctxStyle && (
        <div
          className="omni-ctx-menu"
          role="menu"
          onMouseDown={(e) => e.stopPropagation()}
          style={ctxStyle}
        >
          {[
            {
              icon: "↵",
              label: "Open",
              action: () => {
                onExecute(ctxMenu.item);
                setCtxMenu(null);
              },
            },
            {
              icon: "⎘",
              label: "Copy title",
              action: () => {
                navigator.clipboard.writeText(ctxMenu.item.title).catch(() => {});
                setCtxMenu(null);
              },
            },
            ...(ctxMenu.item.subtitle
              ? [{
                  icon: "⎘",
                  label: "Copy subtitle",
                  action: () => {
                    navigator.clipboard
                      .writeText(ctxMenu.item.subtitle!)
                      .catch(() => {});
                    setCtxMenu(null);
                  },
                }]
              : []),
            {
              icon: favorites.has(ctxMenu.item.id) ? "★" : "☆",
              label: favorites.has(ctxMenu.item.id)
                ? "Remove from favorites"
                : "Add to favorites",
              action: () => {
                toggleFavorite(
                  ctxMenu.item.id,
                  { stopPropagation: () => {} } as React.MouseEvent,
                );
                setCtxMenu(null);
              },
            },
          ].map(({ icon, label, action }) => (
            <div
              key={label}
              role="menuitem"
              className="omni-ctx-menu__item"
              onClick={action}
              tabIndex={0}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  action();
                }
              }}
            >
              <span className="omni-ctx-menu__item-icon" aria-hidden="true">
                {icon}
              </span>
              <span>{label}</span>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
