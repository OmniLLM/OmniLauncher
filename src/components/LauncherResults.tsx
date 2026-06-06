import {
  isAiPrefix,
  isConversationResetCommand,
  isHelpHintQuery,
  isHelpQuery,
} from "../launcherConfig";
import ResultList from "./ResultList";
import type { QueryResult } from "../types/app";

export interface LauncherResultsProps {
  query: string;
  results: QueryResult[];
  searching: boolean;
  searchError: string | null;
  favorites: Set<string>;
  favoriteItems: QueryResult[];
  handleToggleFavorite: (item: QueryResult) => void;
  handleExecute: (item: QueryResult) => void;
}

export default function LauncherResults({
  query,
  results,
  searching,
  searchError,
  favorites,
  favoriteItems,
  handleToggleFavorite,
  handleExecute,
}: LauncherResultsProps) {
  return (
    <>
      {query.trim() === "" && favoriteItems.length > 0 && (
        <ResultList
          results={favoriteItems}
          query=""
          onExecute={handleExecute}
          groupTitle="★ Favorites"
          favorites={favorites}
          onToggleFavorite={handleToggleFavorite}
        />
      )}
      {results.length > 0 && (
        <div
          style={{
            margin: "0 12px 8px",
            background:
              "color-mix(in srgb, var(--surface) 60%, transparent)",
            border: "1px solid var(--border)",
            borderRadius: "12px",
            overflow: "hidden",
            backdropFilter: "blur(10px)",
            WebkitBackdropFilter: "blur(10px)",
            boxShadow: "0 8px 24px rgba(0,0,0,0.18)",
          }}
        >
          <ResultList
            results={results}
            query={query}
            onExecute={handleExecute}
            favorites={favorites}
            onToggleFavorite={handleToggleFavorite}
          />
        </div>
      )}
      {/* Loading skeleton — only when the user has typed something and
      we're waiting on the backend (no stale results to show). */}
      {results.length === 0 && searching && query.trim() !== "" && (
        <div
          className="results"
          aria-live="polite"
          aria-busy="true"
          style={{ padding: "8px 0" }}
        >
          {[0, 1, 2].map((i) => (
            <div
              key={i}
              className="result-item"
              style={{ cursor: "default", animation: "none" }}
            >
              <span
                className="skeleton"
                style={{
                  width: 22,
                  height: 22,
                  borderRadius: 6,
                  flexShrink: 0,
                }}
              />
              <div className="result-item__content">
                <span
                  className="skeleton"
                  style={{
                    display: "block",
                    height: 12,
                    width: `${70 - i * 12}%`,
                    marginBottom: 6,
                  }}
                />
                <span
                  className="skeleton"
                  style={{
                    display: "block",
                    height: 10,
                    width: `${50 - i * 8}%`,
                  }}
                />
              </div>
            </div>
          ))}
        </div>
      )}
      {/* Error state — backend search call rejected. */}
      {results.length === 0 &&
        !searching &&
        searchError &&
        query.trim() !== "" && (
          <div
            role="alert"
            style={{
              padding: "16px",
              fontSize: 13,
              color: "var(--error)",
              lineHeight: 1.55,
            }}
          >
            <div>⚠ Search failed: {searchError}</div>
            <div
              style={{
                marginTop: 6,
                fontSize: 12,
                color: "var(--sub)",
              }}
            >
              Edit your query to try again.
            </div>
          </div>
        )}
      {/* Empty state — query typed, search finished, nothing matched. */}
      {results.length === 0 &&
        !searching &&
        !searchError &&
        query.trim() !== "" &&
        !isHelpQuery(query) &&
        !isHelpHintQuery(query) &&
        !isAiPrefix(query) &&
        !isConversationResetCommand(query) && (
          <div
            style={{
              padding: "16px",
              textAlign: "center",
              fontSize: 13,
              color: "var(--sub)",
              lineHeight: 1.55,
            }}
          >
            <div style={{ fontSize: 22, marginBottom: 4 }}>🔍</div>
            No matches for{" "}
            <strong style={{ color: "var(--text)" }}>{query}</strong>
            <div style={{ marginTop: 6, fontSize: 12 }}>
              Press{" "}
              <kbd
                style={{
                  fontFamily: "monospace",
                  background:
                    "color-mix(in srgb, var(--accent) 12%, transparent)",
                  border: "1px solid var(--border)",
                  borderRadius: 4,
                  padding: "1px 6px",
                  color: "var(--accent)",
                }}
              >
                Ctrl+K
              </kbd>{" "}
              to ask AI, or{" "}
              <kbd
                style={{
                  fontFamily: "monospace",
                  background:
                    "color-mix(in srgb, var(--accent) 12%, transparent)",
                  border: "1px solid var(--border)",
                  borderRadius: 4,
                  padding: "1px 6px",
                  color: "var(--accent)",
                }}
              >
                ?
              </kbd>{" "}
              for help
            </div>
          </div>
        )}
    </>
  );
}
