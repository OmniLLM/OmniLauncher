/**
 * FormattedSubtitle — renders plugin result subtitles with Markdown support.
 *
 * - If the subtitle contains Markdown (tables, lists, bold, code…), it renders
 *   it as rich HTML via renderMarkdown(), expanding the row height automatically.
 * - Plain text subtitles fall back to the original single-line ellipsis style.
 */
import { hasMarkdown, renderMarkdown } from "../utils/markdown";

interface Props {
  text: string;
  color: string; // CSS color for plain text (e.g. colors.sub)
  isPath?: boolean; // hint to use monospace font
}

export default function FormattedSubtitle({ text, color, isPath }: Props) {
  if (!text) return null;

  if (hasMarkdown(text)) {
    return (
      <div
        className="omni-subtitle-rich"
        style={{ color }}
        dangerouslySetInnerHTML={{ __html: renderMarkdown(text) }}
      />
    );
  }

  // Plain text — original single-line style
  return (
    <div
      style={{
        fontSize: "12px",
        color,
        whiteSpace: "nowrap",
        overflow: "hidden",
        textOverflow: "ellipsis",
        fontFamily:
          isPath || text.startsWith("/") || text.includes("\\")
            ? "'JetBrains Mono', 'Fira Code', monospace"
            : "inherit",
        marginTop: "1px",
      }}
    >
      {text}
    </div>
  );
}
