/**
 * OmniLauncher Markdown renderer — shared across ChatBubble and FormattedSubtitle.
 * Extracted from App.tsx to avoid duplication.
 */

export function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

export function safeHref(href: string): string {
  const trimmed = href.trim();
  if (/^(https?:|mailto:)/i.test(trimmed)) return escapeHtml(trimmed);
  return "#";
}

export function inlineFormat(text: string): string {
  return text
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
    .replace(/\*(.+?)\*/g, "<em>$1</em>")
    .replace(/~~(.+?)~~/g, "<del>$1</del>")
    .replace(
      /\[([^\]]+)\]\(([^)]+)\)/g,
      (_match, label, href) =>
        `<a class="md-link" href="${safeHref(href)}" target="_blank">${label}</a>`,
    )
    .replace(/`(.+?)`/g, '<code class="md-inline-code">$1</code>');
}

export function renderTable(rows: string[]): string {
  if (rows.length === 0) return "";
  const parseRow = (row: string) =>
    row
      .split("|")
      .filter((_c, i, arr) => i > 0 && i < arr.length - 1)
      .map((c) => escapeHtml(c.trim()));

  let html = '<table class="md-table"><thead><tr>';
  const header = parseRow(rows[0]);
  header.forEach((cell) => {
    html += `<th>${inlineFormat(cell)}</th>`;
  });
  html += "</tr></thead><tbody>";
  for (let i = 1; i < rows.length; i++) {
    html += "<tr>";
    parseRow(rows[i]).forEach((cell) => {
      html += `<td>${inlineFormat(cell)}</td>`;
    });
    html += "</tr>";
  }
  html += "</tbody></table>";
  return html;
}

export function renderMarkdown(text: string): string {
  let html = text.replace(/```(\w*)\n([\s\S]*?)```/g, (_match, lang, code) => {
    const escaped = escapeHtml(code.trimEnd());
    return `<pre class="md-codeblock"><code class="md-lang-${lang || "text"}">${escaped}</code></pre>`;
  });

  const lines = html.split("\n");
  const result: string[] = [];
  let inList = false;
  let listType = "";
  let inTable = false;
  let tableRows: string[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    if (line.includes('<pre class="md-codeblock">')) {
      result.push(line);
      while (i < lines.length - 1 && !lines[i].includes("</pre>")) {
        i++;
        result.push(lines[i]);
      }
      continue;
    }

    if (line.match(/^\|(.+)\|$/)) {
      if (!inTable) {
        inTable = true;
        tableRows = [];
      }
      if (!line.match(/^\|[\s\-:|]+\|$/)) tableRows.push(line);
      continue;
    } else if (inTable) {
      inTable = false;
      result.push(renderTable(tableRows));
      tableRows = [];
    }

    if (inList && !line.match(/^(\s*[-*]\s|^\s*\d+\.\s)/)) {
      result.push(listType === "ul" ? "</ul>" : "</ol>");
      inList = false;
    }

    if (line.match(/^#{1,6}\s/)) {
      const level = line.match(/^(#{1,6})\s/)![1].length;
      const content = escapeHtml(line.replace(/^#{1,6}\s/, ""));
      result.push(
        `<h${level} class="md-h${level}">${inlineFormat(content)}</h${level}>`,
      );
      continue;
    }

    if (line.match(/^\s*[-*]\s/)) {
      if (!inList || listType !== "ul") {
        if (inList) result.push("</ol>");
        result.push('<ul class="md-list">');
        inList = true;
        listType = "ul";
      }
      result.push(
        `<li>${inlineFormat(escapeHtml(line.replace(/^\s*[-*]\s/, "")))}</li>`,
      );
      continue;
    }

    if (line.match(/^\s*\d+\.\s/)) {
      if (!inList || listType !== "ol") {
        if (inList) result.push("</ul>");
        result.push('<ol class="md-list">');
        inList = true;
        listType = "ol";
      }
      result.push(
        `<li>${inlineFormat(escapeHtml(line.replace(/^\s*\d+\.\s/, "")))}</li>`,
      );
      continue;
    }

    if (line.match(/^---+$/)) {
      result.push('<hr class="md-hr"/>');
      continue;
    }
    if (line.trim() === "") {
      result.push('<div class="md-spacer"></div>');
      continue;
    }
    result.push(`<p class="md-p">${inlineFormat(escapeHtml(line))}</p>`);
  }

  if (inList) result.push(listType === "ul" ? "</ul>" : "</ol>");
  if (inTable) result.push(renderTable(tableRows));

  return result.join("\n");
}

/**
 * Detect whether a string contains Markdown formatting worth rendering:
 * tables, fenced code, lists, headers, bold/italic.
 */
export function hasMarkdown(text: string): boolean {
  return (
    /^\|.+\|/m.test(text) || // table rows
    /```/.test(text) || // fenced code
    /^#{1,6}\s/m.test(text) || // headers
    /^\s*[-*]\s/m.test(text) || // unordered list
    /^\s*\d+\.\s/m.test(text) || // ordered list
    /\*\*.+\*\*/.test(text) || // bold
    /`[^`]+`/.test(text) // inline code
  );
}
