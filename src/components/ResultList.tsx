import React, {
  useState,
  useEffect,
  useCallback,
  useRef,
  useMemo,
} from "react";
import FormattedSubtitle from "./FormattedSubtitle";

interface QueryResult {
  id: string;
  title: string;
  subtitle?: string;
  icon?: string;
  score: number;
  action_type: string;
  action_data: string;
  /** Plugin name that produced this result. Used for section grouping. */
  source?: string;
}

interface Props {
  results: QueryResult[];
  query: string;
  onExecute: (r: QueryResult) => void;
  /** Optional group header rendered above the list (e.g. "★ Favorites"). */
  groupTitle?: string;
  /** Set of favorited result ids (source of truth lives in App / backend). */
  favorites?: Set<string>;
  /** Toggle a result's favorite status. App persists it via the backend. */
  onToggleFavorite?: (r: QueryResult) => void;
}

const ACTION_LABEL: Record<string, string> = {
  open: "Open",
  url: "Open",
  shell: "Run",
  copy: "Copy",
  help_command: "Use",
};

// Detect mac so we show ⌘ vs Ctrl in the kbd hints.
const IS_MAC =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad/i.test(navigator.platform || navigator.userAgent || "");
const MOD_KEY = IS_MAC ? "⌘" : "Ctrl+";

// Keyboard shortcut labels for first 9 results.
function kbdHint(i: number): string {
  if (i < 9) return `${MOD_KEY}${i + 1}`;
  return "";
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function highlight(text: string, q: string): string {
  if (!q) return escapeHtml(text);
  const idx = text.toLowerCase().indexOf(q.toLowerCase());
  if (idx !== -1) {
    return (
      escapeHtml(text.slice(0, idx)) +
      "<mark>" +
      escapeHtml(text.slice(idx, idx + q.length)) +
      "</mark>" +
      escapeHtml(text.slice(idx + q.length))
    );
  }
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

/** Convert a plugin slug ("file_search", "calculator") to a friendly section
 * header label ("File Search", "Calculator"). */
function prettifySource(s: string | undefined): string {
  if (!s) return "Other";
  return s.replace(/[_-]+/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

/** Group results by `source`, preserving the order in which each source
 * first appears (matches the backend's pre-sorted-by-score ordering, which
 * already approximates relevance). Returns a flat array of section-tagged
 * rows so we can keep a single list cursor across all groups (Raycast does
 * the same thing). */
type Row =
  | { kind: "header"; key: string; label: string }
  | { kind: "item"; key: string; result: QueryResult; flatIndex: number };

function buildRows(
  results: QueryResult[],
  groupTitle?: string,
): { rows: Row[]; itemRows: Row[] } {
  const byGroup = new Map<string, QueryResult[]>();
  const order: string[] = [];
  for (const r of results) {
    const g = r.source || "other";
    if (!byGroup.has(g)) {
      byGroup.set(g, []);
      order.push(g);
    }
    byGroup.get(g)!.push(r);
  }

  const rows: Row[] = [];
  const itemRows: Row[] = [];
  let flatIndex = 0;
  // If only one group, skip the auto-derived per-source header (it looks
  // redundant). An explicit `groupTitle` from the caller (e.g. "★ Favorites")
  // always wins — render it once at the top and suppress per-source headers.
  if (groupTitle) {
    rows.push({ kind: "header", key: "h:explicit", label: groupTitle });
  }
  const showHeaders = !groupTitle && order.length > 1;
  for (const g of order) {
    if (showHeaders) {
      rows.push({ kind: "header", key: `h:${g}`, label: prettifySource(g) });
    }
    for (const r of byGroup.get(g)!) {
      const row: Row = { kind: "item", key: r.id, result: r, flatIndex };
      rows.push(row);
      itemRows.push(row);
      flatIndex++;
    }
  }
  return { rows, itemRows };
}

export default function ResultList({
  results,
  query,
  onExecute,
  groupTitle,
  favorites,
  onToggleFavorite,
}: Props) {
  const [selected, setSelected] = useState(0);
  const [hovered, setHovered] = useState(-1);
  const [ctxMenu, setCtxMenu] = useState<{
    x: number;
    y: number;
    item: QueryResult;
  } | null>(null);
  // Favorites are owned by App (backed by the SQLite store). Fall back to an
  // empty set when this list isn't favorites-aware (e.g. slash suggestions).
  const favoriteIds = favorites ?? new Set<string>();

  const listRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<Array<HTMLDivElement | null>>([]);

  const { rows } = useMemo(
    () => buildRows(results, groupTitle),
    [results, groupTitle],
  );

  const toggleFavorite = (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    const item = results.find((r) => r.id === id);
    if (item) onToggleFavorite?.(item);
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

  const sel = results[selected];
  const selActionLabel = sel ? (ACTION_LABEL[sel.action_type] ?? "Run") : "";

  return (
    <>
      <div
        ref={listRef}
        className="results"
        role="listbox"
        aria-activedescendant={
          selected >= 0 ? `omni-opt-${selected}` : undefined
        }
      >
        {rows.map((row) => {
          if (row.kind === "header") {
            return (
              <div
                key={row.key}
                className="result-group__header"
                role="presentation"
              >
                <span>{row.label}</span>
                <span
                  className="result-group__header-rule"
                  aria-hidden="true"
                />
              </div>
            );
          }
          const i = row.flatIndex;
          const r = row.result;
          const isSelected = i === selected;
          const isHovered = i === hovered;
          const isFav = favoriteIds.has(r.id);
          const kbd = kbdHint(i);

          return (
            <div
              key={row.key}
              ref={(el) => {
                itemRefs.current[i] = el;
              }}
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
                  dangerouslySetInnerHTML={{
                    __html: highlight(r.title, query),
                  }}
                />
              </div>

              {/* Right-aligned accessory: subtitle as a single-line chip,
                  Raycast-style. Long subtitles are ellipsized so the row
                  height stays uniform regardless of content. */}
              {r.subtitle && (
                <div className="result-item__accessory" title={r.subtitle}>
                  <FormattedSubtitle text={r.subtitle} color="var(--sub)" />
                </div>
              )}

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
              </div>
            </div>
          );
        })}
      </div>

      {/* Footer action bar — Raycast-style. Shows the primary action of the
          currently selected row plus a hint for the (future) ⌘K action panel.
          Replaces the old bottom preview pane: subtitles already moved to the
          right of each row, so the preview was redundant. */}
      {sel && (
        <div className="result-actionbar" aria-live="polite">
          <span className="result-actionbar__icon" aria-hidden="true">
            {sel.icon || "📄"}
          </span>
          <span className="result-actionbar__title" title={sel.title}>
            {sel.title}
          </span>
          <span className="result-actionbar__spacer" />
          <span className="result-actionbar__primary">
            <span className="result-actionbar__label">{selActionLabel}</span>
            <kbd className="result-actionbar__kbd">↵</kbd>
          </span>
          <span className="result-actionbar__sep" aria-hidden="true" />
          <span
            className="result-actionbar__secondary"
            title="Open actions menu (right-click for now)"
          >
            <span className="result-actionbar__label">Actions</span>
            <kbd className="result-actionbar__kbd">{IS_MAC ? "⌘" : "Ctrl"}</kbd>
            <kbd className="result-actionbar__kbd">K</kbd>
          </span>
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
                navigator.clipboard
                  .writeText(ctxMenu.item.title)
                  .catch(() => {});
                setCtxMenu(null);
              },
            },
            ...(ctxMenu.item.subtitle
              ? [
                  {
                    icon: "⎘",
                    label: "Copy subtitle",
                    action: () => {
                      navigator.clipboard
                        .writeText(ctxMenu.item.subtitle!)
                        .catch(() => {});
                      setCtxMenu(null);
                    },
                  },
                ]
              : []),
            {
              icon: favoriteIds.has(ctxMenu.item.id) ? "★" : "☆",
              label: favoriteIds.has(ctxMenu.item.id)
                ? "Remove from favorites"
                : "Add to favorites",
              action: () => {
                toggleFavorite(ctxMenu.item.id, {
                  stopPropagation: () => {},
                } as React.MouseEvent);
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
