import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import SettingsPanel from "./components/SettingsPanel";

interface QueryResult {
  id: string;
  title: string;
  subtitle?: string;
  icon?: string;
  score: number;
  action_type: string;
  action_data: string;
}

interface AiResponse {
  content: string;
  tools_used: string[];
  results: QueryResult[];
  is_ai: boolean;
}

interface AppSettings {
  ai_base_url: string;
  ai_model: string;
  ai_api_key: string;
  theme: string;
  hotkey: string;
  max_results: number;
}

interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  tools?: string[];
}

function renderMarkdown(text: string): string {
  // Process code blocks first (``` ... ```)
  let html = text.replace(/```(\w*)\n([\s\S]*?)```/g, (_match, lang, code) => {
    const escaped = escapeHtml(code.trimEnd());
    return `<pre class="md-codeblock"><code class="md-lang-${lang || 'text'}">${escaped}</code></pre>`;
  });

  // Split by lines for block-level processing
  const lines = html.split('\n');
  const result: string[] = [];
  let inList = false;
  let listType = '';
  let inTable = false;
  let tableRows: string[] = [];

  for (let i = 0; i < lines.length; i++) {
    let line = lines[i];

    // Skip if inside a pre block
    if (line.includes('<pre class="md-codeblock">')) {
      // Find end of pre block
      result.push(line);
      while (i < lines.length - 1 && !lines[i].includes('</pre>')) {
        i++;
        result.push(lines[i]);
      }
      continue;
    }

    // Table detection
    if (line.match(/^\|(.+)\|$/)) {
      if (!inTable) {
        inTable = true;
        tableRows = [];
      }
      // Skip separator rows
      if (!line.match(/^\|[\s\-:|]+\|$/)) {
        tableRows.push(line);
      }
      continue;
    } else if (inTable) {
      inTable = false;
      result.push(renderTable(tableRows));
      tableRows = [];
    }

    // Close list if needed
    if (inList && !line.match(/^(\s*[-*]\s|^\s*\d+\.\s)/)) {
      result.push(listType === 'ul' ? '</ul>' : '</ol>');
      inList = false;
    }

    // Headers
    if (line.match(/^#{1,6}\s/)) {
      const level = line.match(/^(#{1,6})\s/)![1].length;
      const content = line.replace(/^#{1,6}\s/, '');
      result.push(`<h${level} class="md-h${level}">${inlineFormat(content)}</h${level}>`);
      continue;
    }

    // Unordered list
    if (line.match(/^\s*[-*]\s/)) {
      if (!inList || listType !== 'ul') {
        if (inList) result.push('</ol>');
        result.push('<ul class="md-list">');
        inList = true;
        listType = 'ul';
      }
      const content = line.replace(/^\s*[-*]\s/, '');
      result.push(`<li>${inlineFormat(content)}</li>`);
      continue;
    }

    // Ordered list
    if (line.match(/^\s*\d+\.\s/)) {
      if (!inList || listType !== 'ol') {
        if (inList) result.push('</ul>');
        result.push('<ol class="md-list">');
        inList = true;
        listType = 'ol';
      }
      const content = line.replace(/^\s*\d+\.\s/, '');
      result.push(`<li>${inlineFormat(content)}</li>`);
      continue;
    }

    // Horizontal rule
    if (line.match(/^---+$/)) {
      result.push('<hr class="md-hr"/>');
      continue;
    }

    // Empty line
    if (line.trim() === '') {
      result.push('<br/>');
      continue;
    }

    // Regular paragraph
    result.push(`<p class="md-p">${inlineFormat(line)}</p>`);
  }

  if (inList) result.push(listType === 'ul' ? '</ul>' : '</ol>');
  if (inTable) result.push(renderTable(tableRows));

  return result.join('\n');
}

function inlineFormat(text: string): string {
  return text
    .replace(/`(.+?)`/g, '<code class="md-inline-code">$1</code>')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/\*(.+?)\*/g, '<em>$1</em>')
    .replace(/~~(.+?)~~/g, '<del>$1</del>')
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a class="md-link" href="$2" target="_blank">$1</a>');
}

function renderTable(rows: string[]): string {
  if (rows.length === 0) return '';
  const parseRow = (row: string) =>
    row.split('|').filter((_c, i, arr) => i > 0 && i < arr.length - 1).map(c => c.trim());

  let html = '<table class="md-table"><thead><tr>';
  const header = parseRow(rows[0]);
  header.forEach(cell => { html += `<th>${inlineFormat(cell)}</th>`; });
  html += '</tr></thead><tbody>';
  for (let i = 1; i < rows.length; i++) {
    html += '<tr>';
    parseRow(rows[i]).forEach(cell => { html += `<td>${inlineFormat(cell)}</td>`; });
    html += '</tr>';
  }
  html += '</tbody></table>';
  return html;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

interface SlashCommand {
  cmd: string;
  shortcut?: string;
  description: string;
  usage: string;
  examples: string[];
}

const SLASH_COMMANDS: SlashCommand[] = [
  { cmd: "/run", shortcut: "/r", description: "Execute a shell command", usage: "/run <command>", examples: ["/run dir", "/run git status", "/r npm test"] },
  { cmd: "/open", shortcut: "/o", description: "Open app, file, or URL", usage: "/open <target>", examples: ["/open notepad", "/o https://google.com", "/open C:\\Users"] },
  { cmd: "/app", shortcut: "/a", description: "Search & launch applications", usage: "/app <query>", examples: ["/app chrome", "/a code", "/app firefox"] },
  { cmd: "/find", shortcut: "/f", description: "Search files by name", usage: "/find <filename>", examples: ["/find readme", "/f .gitignore", "/find *.rs"] },
  { cmd: "/grep", shortcut: "/g", description: "Search file contents with regex", usage: "/grep <pattern> [path]", examples: ["/grep TODO src", "/g \"fn main\" .", "/grep error logs/"] },
  { cmd: "/cat", description: "Read and display a file", usage: "/cat <filepath>", examples: ["/cat package.json", "/cat ~/.ssh/config", "/cat Cargo.toml"] },
  { cmd: "/ls", description: "List directory contents", usage: "/ls [path]", examples: ["/ls", "/ls src", "/ls C:\\Users\\jzhu\\repos"] },
  { cmd: "/git", description: "Run git commands", usage: "/git [subcommand]", examples: ["/git", "/git log --oneline -5", "/git branch -a", "/git diff"] },
  { cmd: "/calc", shortcut: "/c", description: "Quick calculator", usage: "/calc <expression>", examples: ["/calc 2^10", "/c 15% of 200", "/calc sqrt(144)"] },
  { cmd: "/todo", shortcut: "/t", description: "Manage todo list", usage: "/todo [text]", examples: ["/todo", "/t buy groceries", "/todo review PR #42"] },
  { cmd: "/web", shortcut: "/w", description: "Search the web (Google)", usage: "/web <query>", examples: ["/web rust async tutorial", "/w tauri v2 docs"] },
  { cmd: "/ip", description: "Show your public IP address", usage: "/ip", examples: ["/ip"] },
  { cmd: "/ports", description: "Show listening network ports", usage: "/ports", examples: ["/ports"] },
  { cmd: "/ps", description: "Top processes by CPU usage", usage: "/ps", examples: ["/ps"] },
  { cmd: "/kill", description: "Kill a process by name or PID", usage: "/kill <name or PID>", examples: ["/kill node", "/kill 1234", "/kill chrome"] },
  { cmd: "/env", description: "Get an environment variable", usage: "/env <variable>", examples: ["/env PATH", "/env HOME", "/env JAVA_HOME"] },
  { cmd: "/color", description: "Convert color formats (hex/rgb/name)", usage: "/color <value>", examples: ["/color #ff6600", "/color rgb(0,128,255)", "/color teal"] },
  { cmd: "/sys", description: "System commands: lock, sleep, shutdown, restart", usage: "/sys <action>", examples: ["/sys lock", "/sys sleep", "/sys shutdown"] },
  { cmd: "/clip", shortcut: "/cb", description: "Search clipboard history", usage: "/clip [term]", examples: ["/clip", "/cb password", "/clip url"] },
  { cmd: "/help", shortcut: "/?", description: "Show all available commands", usage: "/help", examples: ["/help"] },
];

export default function App() {
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [theme, setTheme] = useState<"dark" | "light">("dark");
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const [slashIdx, setSlashIdx] = useState(-1); // selected index in slash dropdown
  const slashDropdownRef = useRef<HTMLDivElement>(null);
  const [liveResults, setLiveResults] = useState<QueryResult[]>([]);
  const [liveIdx, setLiveIdx] = useState(-1);
  const liveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Compute filtered slash commands
  const slashFilter = query.startsWith("/") ? query.split(" ")[0].toLowerCase() : "";
  const showSlashDropdown = query.startsWith("/") && !query.includes(" ") && !loading;
  const filteredSlashCmds = showSlashDropdown
    ? SLASH_COMMANDS.filter(
        (c) =>
          c.cmd.startsWith(slashFilter) ||
          (c.shortcut && c.shortcut.startsWith(slashFilter))
      )
    : [];

  // Live search: when user types `/app <query>`, `/find <query>`, etc., show results
  const liveSearchPrefixes = ["/app ", "/a ", "/find ", "/f ", "/open ", "/o "];
  const isLiveSearch = liveSearchPrefixes.some((p) => query.toLowerCase().startsWith(p));
  const showLiveResults = isLiveSearch && liveResults.length > 0 && !loading;

  useEffect(() => {
    if (!isLiveSearch) {
      setLiveResults([]);
      setLiveIdx(-1);
      return;
    }
    // Extract search term after the command prefix
    const spaceIdx = query.indexOf(" ");
    const searchTerm = spaceIdx >= 0 ? query.slice(spaceIdx + 1).trim() : "";
    if (searchTerm.length === 0) {
      setLiveResults([]);
      return;
    }

    // Debounce: wait 150ms before searching
    if (liveTimerRef.current) clearTimeout(liveTimerRef.current);
    liveTimerRef.current = setTimeout(async () => {
      try {
        const results = await invoke<QueryResult[]>("search", { query: searchTerm });
        setLiveResults(results);
        setLiveIdx(-1);
      } catch {
        setLiveResults([]);
      }
    }, 150);

    return () => {
      if (liveTimerRef.current) clearTimeout(liveTimerRef.current);
    };
  }, [query, isLiveSearch]);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages, loading]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Re-focus input when window becomes visible
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWebviewWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused) {
          inputRef.current?.focus();
        }
      })
      .then((fn) => { unlisten = fn; })
      .catch(() => {});
    return () => { unlisten?.(); };
  }, []);

  const doAiQuery = useCallback(async (q: string) => {
    if (!q.trim()) return;
    setMessages((prev) => [...prev, { role: "user", content: q }]);
    setQuery("");
    setLoading(true);
    try {
      const res = await invoke<AiResponse>("ai_query", { query: q });
      setMessages((prev) => [
        ...prev,
        { role: "assistant", content: res.content, tools: res.tools_used },
      ]);
    } catch (e) {
      setMessages((prev) => [
        ...prev,
        { role: "assistant", content: `Error: ${e}` },
      ]);
    } finally {
      setLoading(false);
      // Re-focus input after response
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, []);

  const handleSubmit = () => {
    if (query.trim() && !loading) {
      doAiQuery(query);
    }
  };

  const handleNewChat = useCallback(async () => {
    try {
      await invoke("clear_conversation");
    } catch (e) {
      console.error("clear_conversation error:", e);
    }
    setMessages([]);
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (showSettings) setShowSettings(false);
      }
      if (e.key === "," && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setShowSettings((s) => !s);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [showSettings]);

  useEffect(() => {
    invoke<AppSettings>("get_settings")
      .then((s) => {
        setSettings(s);
        setTheme(s.theme as "dark" | "light");
      })
      .catch((e) => console.error("Failed to load settings:", e));
  }, []);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  if (showSettings) {
    return (
      <div className="launcher">
        <SettingsPanel
          theme={theme}
          onThemeChange={setTheme}
          onClose={() => setShowSettings(false)}
          initialSettings={settings}
        />
      </div>
    );
  }

  return (
    <div className="launcher chat-layout">
      {/* Header */}
      <div className="chat-header">
        <div className="chat-header__left">
          <span className="chat-header__logo">
            <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/></svg>
          </span>
          <span className="chat-header__title">OmniLauncher</span>
        </div>
        <div className="chat-header__center">
          <span className="chat-header__model">{settings?.ai_model || "auto"}</span>
        </div>
        <div className="chat-header__right">
          <button className="chat-header__btn" onClick={handleNewChat} title="New chat">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M12 5v14M5 12h14"/></svg>
          </button>
          <button className="chat-header__btn" onClick={() => setShowSettings(true)} title="Settings">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
          </button>
        </div>
      </div>

      {/* Messages area */}
      <div className="chat-messages">
        {messages.length === 0 && (
          <div className="chat-empty">
            <div className="chat-empty__icon">◆</div>
            <div className="chat-empty__text">Ask anything or search for apps...</div>
            <div className="chat-empty__hint">Press Enter to send · Ctrl+, for settings</div>
          </div>
        )}
        {messages.map((msg, i) => (
          <div key={i} className={`chat-msg chat-msg--${msg.role}`}>
            {msg.role === "user" ? (
              <div className="chat-msg__user-bubble">{msg.content}</div>
            ) : (
              <div className="chat-msg__assistant">
                {msg.tools && msg.tools.length > 0 && (
                  <div className="chat-msg__tools">
                    {msg.tools.map((t, j) => (
                      <span key={j} className="chat-msg__tool-badge">{t}</span>
                    ))}
                  </div>
                )}
                <div
                  className="chat-msg__content"
                  dangerouslySetInnerHTML={{
                    __html: renderMarkdown(msg.content),
                  }}
                />
              </div>
            )}
          </div>
        ))}
        {loading && (
          <div className="chat-msg chat-msg--assistant">
            <div className="chat-msg__assistant">
              <div className="chat-msg__thinking">
                <span className="loading__dot" />
                <span className="loading__dot" />
                <span className="loading__dot" />
              </div>
            </div>
          </div>
        )}
        <div ref={messagesEndRef} />
      </div>

      {/* Input bar at bottom */}
      <div className="chat-input">
        {/* Slash command dropdown */}
        {showSlashDropdown && filteredSlashCmds.length > 0 && (
          <div className="slash-dropdown" ref={slashDropdownRef}>
            {filteredSlashCmds.map((cmd, i) => (
              <div
                key={cmd.cmd}
                className={`slash-dropdown__item${i === slashIdx ? " slash-dropdown__item--active" : ""}`}
                onClick={() => {
                  setQuery(cmd.cmd + " ");
                  setSlashIdx(-1);
                  inputRef.current?.focus();
                }}
                onMouseEnter={() => setSlashIdx(i)}
              >
                <div className="slash-dropdown__header">
                  <span className="slash-dropdown__cmd">{cmd.cmd}</span>
                  {cmd.shortcut && <span className="slash-dropdown__shortcut">{cmd.shortcut}</span>}
                </div>
                <div className="slash-dropdown__desc">{cmd.description}</div>
              </div>
            ))}
          </div>
        )}
        {/* Live search results dropdown (e.g. /app vs → shows matching apps) */}
        {showLiveResults && (
          <div className="slash-dropdown">
            {liveResults.map((r, i) => (
              <div
                key={r.id}
                className={`slash-dropdown__item${i === liveIdx ? " slash-dropdown__item--active" : ""}`}
                onClick={() => {
                  invoke("execute_result", { result: r }).catch(console.error);
                  setQuery("");
                  setLiveResults([]);
                }}
                onMouseEnter={() => setLiveIdx(i)}
              >
                <div className="slash-dropdown__header">
                  <span className="slash-dropdown__cmd">{r.icon || "📄"} {r.title}</span>
                </div>
                {r.subtitle && <div className="slash-dropdown__desc">{r.subtitle}</div>}
              </div>
            ))}
          </div>
        )}
        <input
          ref={inputRef}
          className="chat-input__field"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setSlashIdx(-1);
          }}
          onKeyDown={(e) => {
            if (showSlashDropdown && filteredSlashCmds.length > 0) {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setSlashIdx((prev) => (prev + 1) % filteredSlashCmds.length);
                return;
              }
              if (e.key === "ArrowUp") {
                e.preventDefault();
                setSlashIdx((prev) => (prev <= 0 ? filteredSlashCmds.length - 1 : prev - 1));
                return;
              }
              if (e.key === "Tab" || (e.key === "Enter" && slashIdx >= 0)) {
                e.preventDefault();
                const selected = filteredSlashCmds[slashIdx >= 0 ? slashIdx : 0];
                setQuery(selected.cmd + " ");
                setSlashIdx(-1);
                return;
              }
            }
            if (showLiveResults && liveResults.length > 0) {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setLiveIdx((prev) => (prev + 1) % liveResults.length);
                return;
              }
              if (e.key === "ArrowUp") {
                e.preventDefault();
                setLiveIdx((prev) => (prev <= 0 ? liveResults.length - 1 : prev - 1));
                return;
              }
              if (e.key === "Enter" && liveIdx >= 0) {
                e.preventDefault();
                const selected = liveResults[liveIdx];
                invoke("execute_result", { result: selected }).catch(console.error);
                setQuery("");
                setLiveResults([]);
                return;
              }
            }
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              handleSubmit();
            }
          }}
          placeholder="Type / for commands, or ask anything..."
          disabled={loading}
        />
        <button
          className={`chat-input__send${query.trim() && !loading ? " chat-input__send--active" : ""}`}
          onClick={handleSubmit}
          disabled={!query.trim() || loading}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M5 12h14M12 5l7 7-7 7"/>
          </svg>
        </button>
      </div>
    </div>
  );
}
