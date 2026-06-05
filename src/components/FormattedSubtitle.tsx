/**
 * FormattedSubtitle — renders plugin result subtitles with Markdown support.
 *
 * Launcher rows are compact, so rich Markdown is *downgraded* here (full
 * rendering stays in the AI ChatBubble):
 * - Tables collapse to a one-line "first row | … (+N more)" text summary.
 * - Code blocks wrap (pre-wrap, via CSS) instead of scrolling horizontally.
 * - The capped container fades out at the bottom (mask-image, via CSS).
 */
import { hasMarkdown, renderMarkdown } from "../utils/markdown";

interface Props {
  text: string;
  color: string; // CSS color for plain text (e.g. colors.sub)
  isPath?: boolean; // hint to use monospace font
}

// Collapse Markdown tables into a single plain-text summary line so they don't
// blow up the launcher row. Non-table content is passed through untouched.
function summarizeTables(text: string): string {
  const lines = text.split("\n");
  const out: string[] = [];
  let i = 0;
  const isTableRow = (l: string) => /^\s*\|.*\|\s*$/.test(l);
  const isSeparator = (l: string) => /^\s*\|[\s\-:|]+\|\s*$/.test(l);

  while (i < lines.length) {
    if (isTableRow(lines[i])) {
      const block: string[] = [];
      while (i < lines.length && isTableRow(lines[i])) {
        block.push(lines[i]);
        i++;
      }
      const dataRows = block.filter((l) => !isSeparator(l));
      if (dataRows.length > 0) {
        const cells = dataRows[0]
          .trim()
          .replace(/^\||\|$/g, "")
          .split("|")
          .map((c) => c.trim())
          .filter((c) => c.length > 0);
        const summary = cells.join(" | ");
        const more = dataRows.length - 1;
        out.push(more > 0 ? `${summary} … (+${more} more)` : summary);
      }
      continue;
    }
    out.push(lines[i]);
    i++;
  }
  return out.join("\n");
}

export default function FormattedSubtitle({ text, color, isPath }: Props) {
  if (!text) return null;

  if (hasMarkdown(text)) {
    return (
      <div
        className="omni-subtitle-rich"
        style={{ color }}
        dangerouslySetInnerHTML={{
          __html: renderMarkdown(summarizeTables(text)),
        }}
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
