import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "../lib/runtime";
import type { QueryResult } from "../types/app";

export interface UseFavoritesResult {
  favoriteItems: QueryResult[];
  favorites: Set<string>;
  refreshFavorites: () => Promise<void>;
  handleToggleFavorite: (item: QueryResult) => Promise<void>;
}

export function useFavorites(): UseFavoritesResult {
  // Favorites are persisted in the backend (SQLite). App owns the source of
  // truth and passes it down to ResultList; the component only renders + calls
  // the toggle callback.
  const [favoriteItems, setFavoriteItems] = useState<QueryResult[]>([]);
  const favorites = useMemo(
    () => new Set(favoriteItems.map((f) => f.id)),
    [favoriteItems],
  );

  const refreshFavorites = useCallback(async () => {
    try {
      const items = await invoke<QueryResult[]>("list_favorites");
      setFavoriteItems(items || []);
    } catch (e) {
      console.error("list_favorites error:", e);
    }
  }, []);

  const handleToggleFavorite = useCallback(
    async (item: QueryResult) => {
      const isFav = favorites.has(item.id);
      // Optimistic update so the star flips instantly.
      setFavoriteItems((prev) =>
        isFav ? prev.filter((f) => f.id !== item.id) : [...prev, item],
      );
      try {
        if (isFav) {
          await invoke("remove_favorite", { id: item.id });
        } else {
          await invoke("add_favorite", { result: item });
        }
      } catch (e) {
        console.error("toggle favorite error:", e);
        refreshFavorites(); // reconcile on failure
      }
    },
    [favorites, refreshFavorites],
  );

  // Hydrate favorites from the backend, running a one-time migration of any
  // favorites that were previously stored in localStorage.
  useEffect(() => {
    (async () => {
      try {
        if (!localStorage.getItem("omni-favorites-migrated")) {
          const ids: string[] = JSON.parse(
            localStorage.getItem("omni-favorites") || "[]",
          );
          const items: QueryResult[] = JSON.parse(
            localStorage.getItem("omni-favorite-items") || "[]",
          );
          for (const id of ids) {
            const item = items.find((r) => r.id === id);
            if (item) {
              try {
                await invoke("add_favorite", { result: item });
              } catch (e) {
                console.error("favorite migration error:", e);
              }
            }
          }
          localStorage.setItem("omni-favorites-migrated", "1");
          localStorage.removeItem("omni-favorites");
          localStorage.removeItem("omni-favorite-items");
        }
      } catch (e) {
        console.error("favorite migration failed:", e);
      }
      refreshFavorites();
    })();
  }, [refreshFavorites]);

  return { favoriteItems, favorites, refreshFavorites, handleToggleFavorite };
}
