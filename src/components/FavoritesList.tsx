import { useState, useEffect } from "react";

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
  favoriteIds: string[];
  onExecute: (r: QueryResult) => void;
  colors: Record<string, string>;
  onFavoritesChange: (ids: string[]) => void;
}

export default function FavoritesList({ favoriteIds, onExecute, colors, onFavoritesChange }: Props) {
  const [items, setItems] = useState<QueryResult[]>([]);

  useEffect(() => {
    if (favoriteIds.length === 0) { setItems([]); return; }
    try {
      const stored = localStorage.getItem("omni-favorite-items");
      const all: QueryResult[] = stored ? JSON.parse(stored) : [];
      setItems(favoriteIds.map(id => all.find(r => r.id === id)).filter(Boolean) as QueryResult[]);
    } catch { setItems([]); }
  }, [favoriteIds]);

  if (items.length === 0) return null;

  return (
    <div style={{ borderBottom: `1px solid rgba(255,255,255,0.06)`, paddingBottom: 4 }}>
      <div style={{
        fontSize: 10, fontWeight: 700, letterSpacing: "0.08em",
        color: colors.sub, textTransform: "uppercase" as const,
        padding: "6px 16px 4px", opacity: 0.7,
      }}>
        ★ Favorites
      </div>
      {items.map((r) => (
        <div
          key={r.id}
          onClick={() => onExecute(r)}
          style={{
            display: "flex", alignItems: "center", gap: 12,
            padding: "7px 14px", cursor: "pointer",
            transition: "background 150ms",
          }}
          onMouseEnter={(e) => (e.currentTarget.style.background = `${colors.surface}CC`)}
          onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
        >
          <span style={{ fontSize: 18, width: 22, textAlign: "center" as const, flexShrink: 0 }}>
            {r.icon || "📄"}
          </span>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 14, fontWeight: 500, color: colors.text, whiteSpace: "nowrap" as const, overflow: "hidden", textOverflow: "ellipsis" }}>
              {r.title}
            </div>
            {r.subtitle && (
              <div style={{ fontSize: 12, color: colors.sub, whiteSpace: "nowrap" as const, overflow: "hidden", textOverflow: "ellipsis" }}>
                {r.subtitle}
              </div>
            )}
          </div>
          <button
            onClick={(e) => {
              e.stopPropagation();
              const newIds = favoriteIds.filter(id => id !== r.id);
              try { localStorage.setItem("omni-favorites", JSON.stringify(newIds)); } catch {}
              onFavoritesChange(newIds);
            }}
            style={{ background: "none", border: "none", cursor: "pointer", color: "#f5c842", fontSize: 13, padding: "0 2px", opacity: 0.8 }}
            title="Remove favorite"
          >★</button>
        </div>
      ))}
    </div>
  );
}
