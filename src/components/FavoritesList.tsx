import { useState, useEffect } from "react";

interface QueryResult {
  id: string;
  title: string;
  subtitle?: string;
  icon?: string;
  score: number;
  action_type: string;
  action_data: string;
  source?: string;
}

interface Props {
  favoriteIds: string[];
  onExecute: (r: QueryResult) => void;
  onFavoritesChange: (ids: string[]) => void;
}

export default function FavoritesList({
  favoriteIds,
  onExecute,
  onFavoritesChange,
}: Props) {
  const [items, setItems] = useState<QueryResult[]>([]);

  useEffect(() => {
    if (favoriteIds.length === 0) {
      setItems([]);
      return;
    }
    try {
      const stored = localStorage.getItem("omni-favorite-items");
      const all: QueryResult[] = stored ? JSON.parse(stored) : [];
      setItems(
        favoriteIds
          .map((id) => all.find((r) => r.id === id))
          .filter(Boolean) as QueryResult[],
      );
    } catch {
      setItems([]);
    }
  }, [favoriteIds]);

  if (items.length === 0) return null;

  return (
    <div className="omni-favorites">
      <div className="omni-favorites__header">★ Favorites</div>
      {items.map((r) => (
        <div
          key={r.id}
          className="omni-favorites__row"
          onClick={() => onExecute(r)}
        >
          <span className="omni-favorites__icon">{r.icon || "📄"}</span>
          <div className="omni-favorites__main">
            <div className="omni-favorites__title">{r.title}</div>
            {r.subtitle && (
              <div className="omni-favorites__sub">{r.subtitle}</div>
            )}
          </div>
          <button
            className="omni-favorites__star"
            onClick={(e) => {
              e.stopPropagation();
              const newIds = favoriteIds.filter((id) => id !== r.id);
              try {
                localStorage.setItem("omni-favorites", JSON.stringify(newIds));
              } catch {}
              onFavoritesChange(newIds);
            }}
            title="Remove favorite"
          >
            ★
          </button>
        </div>
      ))}
    </div>
  );
}
