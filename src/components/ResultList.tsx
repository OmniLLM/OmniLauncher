import { useState, useEffect, useCallback } from "react";

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

export default function ResultList({ results, query, onExecute }: Props) {
  const [selected, setSelected] = useState(0);

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

  function actionBadge(type: string): string {
    switch (type) {
      case "open": return "↵ Open";
      case "url": return "↵ Open";
      case "shell": return "↵ Run";
      case "copy": return "↵ Copy";
      default: return "↵";
    }
  }

  return (
    <div className="results">
      {results.map((r, i) => (
        <div
          key={r.id}
          className={`result-item${i === selected ? " result-item--selected" : ""}`}
          style={{ animationDelay: `${i * 30}ms` }}
          onClick={() => onExecute(r)}
          onMouseEnter={() => setSelected(i)}
        >
          <span className="result-item__icon">{r.icon || "📄"}</span>
          <div className="result-item__content">
            <div
              className="result-item__title"
              dangerouslySetInnerHTML={{ __html: highlight(r.title, query) }}
            />
            {r.subtitle && (
              <div className="result-item__subtitle">{r.subtitle}</div>
            )}
          </div>
          <span className="result-item__badge">{actionBadge(r.action_type)}</span>
        </div>
      ))}
    </div>
  );
}
