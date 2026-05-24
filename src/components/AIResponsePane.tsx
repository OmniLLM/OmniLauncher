import { useState } from "react";

interface AiResponse {
  content: string;
  tools_used: string[];
  results: unknown[];
  is_ai: boolean;
}

interface Props {
  response: AiResponse | null;
}

function renderMarkdown(text: string): string {
  return text
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
    .replace(/\*(.+?)\*/g, "<em>$1</em>")
    .replace(/`(.+?)`/g, "<code>$1</code>")
    .replace(/\n/g, "<br/>");
}

export default function AIResponsePane({ response }: Props) {
  const [copied, setCopied] = useState(false);
  const displayContent = response?.content || "";
  const tools = response?.tools_used ?? [];

  const handleCopy = () => {
    navigator.clipboard
      .writeText(displayContent)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      })
      .catch(() => {});
  };

  return (
    <div className="ai-response">
      {tools.length > 0 && (
        <div className="ai-response__tools">
          {tools.map((tool, i) => (
            <span
              key={i}
              className="ai-response__tool-badge"
              style={{ animationDelay: `${i * 80}ms` }}
            >
              {tool}
            </span>
          ))}
        </div>
      )}

      <div
        className="ai-response__content"
        dangerouslySetInnerHTML={{ __html: renderMarkdown(displayContent) }}
      />

      <div className="ai-response__actions">
        <button className="ai-response__copy-btn" onClick={handleCopy}>
          {copied ? "✓ Copied" : "Copy"}
        </button>
      </div>
    </div>
  );
}
