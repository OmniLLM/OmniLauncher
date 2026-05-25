import { useState, useEffect, useCallback, useRef } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import SearchBar from './components/SearchBar'
import ResultList from './components/ResultList'
import SettingsPanel from './components/SettingsPanel'

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

interface ConversationTurn {
  role: 'user' | 'assistant'
  content: string
  tools_used?: string[]
  isStreaming?: boolean
}

interface AppSettings {
  ai_base_url: string;
  ai_model: string;
  ai_api_key: string;
  theme: string;
  hotkey: string;
  max_results: number;
}

/**
 * Detect if the user typed an explicit AI prefix.
 * Mirrors Router::decide() in router.rs.
 *
 * Prefixes: `?`  or  `ai ` (case-insensitive)
 *
 * Everything else → local plugins (instant, no AI latency).
 */
export function isAiPrefix(input: string): boolean {
  const t = input.trim()
  return t.startsWith('?') || t.toLowerCase().startsWith('ai ')
}

const DARK_COLORS = {
  bg: '#1E1E2E',
  surface: '#313244',
  surface2: '#45475A',
  text: '#CDD6F4',
  accent: '#CBA6F7',
  accentDim: '#9B76C7',
  sub: '#6C7086',
  userBubble: '#CBA6F7',
  userBubbleText: '#1E1E2E',
  aiBubble: '#313244',
  aiText: '#CDD6F4',
}

const LIGHT_COLORS = {
  bg: '#EFF1F5',
  surface: '#CCD0DA',
  surface2: '#BCC0CC',
  text: '#4C4F69',
  accent: '#8839EF',
  accentDim: '#6E28CF',
  sub: '#9CA0B0',
  userBubble: '#8839EF',
  userBubbleText: '#EFF1F5',
  aiBubble: '#CCD0DA',
  aiText: '#4C4F69',
}

// ─── Markdown renderer ────────────────────────────────────────────────────────

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
}

function inlineFormat(text: string): string {
  return text
    .replace(/`(.+?)`/g, '<code class="md-inline-code">$1</code>')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/\*(.+?)\*/g, '<em>$1</em>')
    .replace(/~~(.+?)~~/g, '<del>$1</del>')
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a class="md-link" href="$2" target="_blank">$1</a>')
}

function renderTable(rows: string[]): string {
  if (rows.length === 0) return ''
  const parseRow = (row: string) =>
    row.split('|').filter((_c, i, arr) => i > 0 && i < arr.length - 1).map(c => c.trim())

  let html = '<table class="md-table"><thead><tr>'
  const header = parseRow(rows[0])
  header.forEach(cell => { html += `<th>${inlineFormat(cell)}</th>` })
  html += '</tr></thead><tbody>'
  for (let i = 1; i < rows.length; i++) {
    html += '<tr>'
    parseRow(rows[i]).forEach(cell => { html += `<td>${inlineFormat(cell)}</td>` })
    html += '</tr>'
  }
  html += '</tbody></table>'
  return html
}

function renderMarkdown(text: string): string {
  let html = text.replace(/```(\w*)\n([\s\S]*?)```/g, (_match, lang, code) => {
    const escaped = escapeHtml(code.trimEnd())
    return `<pre class="md-codeblock"><code class="md-lang-${lang || 'text'}">${escaped}</code></pre>`
  })

  const lines = html.split('\n')
  const result: string[] = []
  let inList = false
  let listType = ''
  let inTable = false
  let tableRows: string[] = []

  for (let i = 0; i < lines.length; i++) {
    let line = lines[i]

    if (line.includes('<pre class="md-codeblock">')) {
      result.push(line)
      while (i < lines.length - 1 && !lines[i].includes('</pre>')) {
        i++
        result.push(lines[i])
      }
      continue
    }

    if (line.match(/^\|(.+)\|$/)) {
      if (!inTable) { inTable = true; tableRows = [] }
      if (!line.match(/^\|[\s\-:|]+\|$/)) tableRows.push(line)
      continue
    } else if (inTable) {
      inTable = false
      result.push(renderTable(tableRows))
      tableRows = []
    }

    if (inList && !line.match(/^(\s*[-*]\s|^\s*\d+\.\s)/)) {
      result.push(listType === 'ul' ? '</ul>' : '</ol>')
      inList = false
    }

    if (line.match(/^#{1,6}\s/)) {
      const level = line.match(/^(#{1,6})\s/)![1].length
      const content = line.replace(/^#{1,6}\s/, '')
      result.push(`<h${level} class="md-h${level}">${inlineFormat(content)}</h${level}>`)
      continue
    }

    if (line.match(/^\s*[-*]\s/)) {
      if (!inList || listType !== 'ul') {
        if (inList) result.push('</ol>')
        result.push('<ul class="md-list">')
        inList = true; listType = 'ul'
      }
      result.push(`<li>${inlineFormat(line.replace(/^\s*[-*]\s/, ''))}</li>`)
      continue
    }

    if (line.match(/^\s*\d+\.\s/)) {
      if (!inList || listType !== 'ol') {
        if (inList) result.push('</ul>')
        result.push('<ol class="md-list">')
        inList = true; listType = 'ol'
      }
      result.push(`<li>${inlineFormat(line.replace(/^\s*\d+\.\s/, ''))}</li>`)
      continue
    }

    if (line.match(/^---+$/)) { result.push('<hr class="md-hr"/>'); continue }
    if (line.trim() === '') { result.push('<div class="md-spacer"></div>'); continue }
    result.push(`<p class="md-p">${inlineFormat(line)}</p>`)
  }

  if (inList) result.push(listType === 'ul' ? '</ul>' : '</ol>')
  if (inTable) result.push(renderTable(tableRows))

  return result.join('\n')
}

// ─── Slash commands ───────────────────────────────────────────────────────────

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
  { cmd: "/skill", description: "Manage skills (list, view, install, reload)", usage: "/skill [list|view|install|reload|help]", examples: ["/skill list", "/skill view web-summarizer", "/skill help"] },
  { cmd: "/help", shortcut: "/?", description: "Show all available commands", usage: "/help", examples: ["/help"] },
]

// ─── App ──────────────────────────────────────────────────────────────────────

export default function App() {
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<QueryResult[]>([])
  const [loading, setLoading] = useState(false)
  const [showSettings, setShowSettings] = useState(false)
  const [theme, setTheme] = useState<'dark' | 'light'>('dark')
  const [conversationHistory, setConversationHistory] = useState<ConversationTurn[]>([])
  const [settings, setSettings] = useState<AppSettings | null>(null)
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const chatScrollRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  const colors = theme === 'dark' ? DARK_COLORS : LIGHT_COLORS

  const isAiMode = isAiPrefix(query)

  // Load settings on mount
  useEffect(() => {
    invoke<AppSettings>('get_settings')
      .then(s => {
        setSettings(s)
        if (s.theme === 'light') setTheme('light')
      })
      .catch(() => {})
  }, [])

  const doSearch = useCallback(async (q: string) => {
    if (!q.trim() || isAiPrefix(q)) {
      setResults([])
      return
    }
    try {
      const res = await invoke<QueryResult[]>('search', { query: q })
      setResults(res)
    } catch {
      setResults([])
    }
  }, [])

  // Focus input on mount
  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  // Re-focus input when window becomes visible
  useEffect(() => {
    let unlisten: (() => void) | undefined
    getCurrentWebviewWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused) {
          inputRef.current?.focus()
          setTimeout(() => {
            inputRef.current?.focus()
            inputRef.current?.select()
          }, 50)
          setTimeout(() => inputRef.current?.focus(), 150)
        }
      })
      .then(fn => { unlisten = fn })
      .catch(() => {})
    return () => { unlisten?.() }
  }, [])

  const doAiQuery = useCallback(async (q: string) => {
    if (!q.trim()) return

    const userTurn: ConversationTurn = { role: 'user', content: q }
    const pendingAiTurn: ConversationTurn = {
      role: 'assistant',
      content: '',
      tools_used: [],
      isStreaming: true,
    }
    setConversationHistory(prev => [...prev, userTurn, pendingAiTurn])
    setLoading(true)
    setResults([])

    try {
      const res = await invoke<AiResponse>('ai_query', { query: q })
      setConversationHistory(prev => {
        const next = [...prev]
        next[next.length - 1] = {
          role: 'assistant',
          content: res.content,
          tools_used: res.tools_used,
          isStreaming: false,
        }
        return next
      })
    } catch (e) {
      setConversationHistory(prev => {
        const next = [...prev]
        next[next.length - 1] = {
          role: 'assistant',
          content: `Error: ${e}`,
          isStreaming: false,
        }
        return next
      })
    } finally {
      setLoading(false)
      setTimeout(() => inputRef.current?.focus(), 50)
    }
  }, [])

  const handleQueryChange = useCallback((value: string) => {
    setQuery(value)
    if (debounceRef.current) clearTimeout(debounceRef.current)
    debounceRef.current = setTimeout(() => {
      doSearch(value)
    }, 100)
  }, [doSearch])

  const handleSubmit = useCallback((value: string, forceAi: boolean) => {
    if (forceAi || isAiPrefix(value)) {
      if (debounceRef.current) clearTimeout(debounceRef.current)
      doAiQuery(value)
      setQuery('')
    }
  }, [doAiQuery])

  const handleNewConversation = useCallback(async () => {
    try {
      await invoke('clear_conversation')
    } catch (e) {
      console.error('clear_conversation error:', e)
    }
    setConversationHistory([])
    setResults([])
    setQuery('')
  }, [])

  const handleExecute = useCallback(async (result: QueryResult) => {
    if (result.action_type === 'copy') {
      try {
        await navigator.clipboard.writeText(result.action_data)
      } catch {
        console.log('Copy:', result.action_data)
      }
      return
    }
    try {
      await invoke('execute_result', { result })
    } catch (e) {
      console.error('Execute error:', e)
    }
  }, [])

  // Scroll chat to bottom on new messages
  useEffect(() => {
    if (isAiMode && chatScrollRef.current) {
      chatScrollRef.current.scrollTop = chatScrollRef.current.scrollHeight
    }
  }, [conversationHistory, isAiMode])

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setQuery('')
        setResults([])
      }
      if (e.key === ',' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault()
        setShowSettings(s => !s)
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])

  // ── Layout geometry ────────────────────────────────────────────────────────
  const launcherHasContent = results.length > 0 || showSettings
  const windowHeight = isAiMode ? '560px' : launcherHasContent ? 'auto' : '64px'
  const maxHeight = isAiMode ? '560px' : '520px'

  // Empty launcher state
  const showEmptyState = !isAiMode && !showSettings && results.length === 0 && query === ''

  return (
    <>
      {/* Global keyframe animations injected once */}
      <style>{`
        @keyframes omni-fade-in {
          from { opacity: 0; transform: translateY(8px); }
          to   { opacity: 1; transform: translateY(0); }
        }
        @keyframes omni-dot-pulse {
          0%, 80%, 100% { opacity: 0.3; transform: scale(0.8); }
          40%            { opacity: 1;   transform: scale(1); }
        }
        @keyframes omni-blink {
          0%, 100% { opacity: 1; }
          50%       { opacity: 0; }
        }
        @keyframes omni-spin {
          to { transform: rotate(360deg); }
        }
        .omni-bubble-enter {
          animation: omni-fade-in 200ms ease both;
        }
        .omni-cursor::after {
          content: '|';
          animation: omni-blink 1s step-start infinite;
          color: inherit;
          margin-left: 1px;
        }
      `}</style>

      <div
        style={{
          width: '680px',
          height: windowHeight,
          maxHeight,
          background: colors.bg,
          color: colors.text,
          fontFamily: "'Inter', 'Segoe UI', system-ui, sans-serif",
          borderRadius: '14px',
          overflow: 'hidden',
          boxShadow: '0 24px 64px rgba(0,0,0,0.55)',
          display: 'flex',
          flexDirection: 'column',
          transition: 'height 220ms cubic-bezier(0.4,0,0.2,1), max-height 220ms cubic-bezier(0.4,0,0.2,1)',
          outline: isAiMode ? `1.5px solid ${colors.accent}33` : 'none',
        }}
      >
        {/* ── AI MODE: top bar ─────────────────────────────────────────── */}
        {isAiMode && (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              padding: '10px 16px 0',
              flexShrink: 0,
            }}
          >
            <span style={{ fontSize: '13px', color: colors.accent, fontWeight: 600, letterSpacing: '0.03em' }}>
              ✦ AI Chat
            </span>
            <button
              onClick={handleNewConversation}
              style={{
                background: colors.surface,
                border: 'none',
                borderRadius: '7px',
                padding: '4px 11px',
                color: colors.text,
                cursor: 'pointer',
                fontSize: '12px',
                display: 'flex',
                alignItems: 'center',
                gap: '5px',
              }}
              onMouseEnter={e => (e.currentTarget.style.background = colors.surface2)}
              onMouseLeave={e => (e.currentTarget.style.background = colors.surface)}
            >
              <span style={{ fontSize: '10px' }}>✦</span> New conversation
            </button>
          </div>
        )}

        {/* ── AI MODE: scrollable chat history ─────────────────────────── */}
        {isAiMode && (
          <div
            ref={chatScrollRef}
            style={{
              flex: 1,
              overflowY: 'auto',
              padding: '12px 16px',
              display: 'flex',
              flexDirection: 'column',
              gap: '10px',
              scrollbarWidth: 'thin',
              scrollbarColor: `${colors.surface2} transparent`,
            }}
          >
            {conversationHistory.length === 0 && (
              <div
                style={{
                  flex: 1,
                  display: 'flex',
                  flexDirection: 'column',
                  alignItems: 'center',
                  justifyContent: 'center',
                  color: colors.sub,
                  gap: '8px',
                  paddingBottom: '24px',
                  minHeight: '360px',
                }}
              >
                <span style={{ fontSize: '32px', opacity: 0.35 }}>✦</span>
                <span style={{ fontSize: '13px', textAlign: 'center', maxWidth: '280px', lineHeight: 1.6 }}>
                  Ask me anything — I can search the web, run calculations, and more.
                </span>
              </div>
            )}

            {conversationHistory.map((turn, i) => (
              <ChatBubble key={i} turn={turn} colors={colors} />
            ))}
          </div>
        )}

        {/* ── SETTINGS panel ───────────────────────────────────────────── */}
        {showSettings && !isAiMode && (
          <SettingsPanel
            theme={theme}
            onThemeChange={setTheme}
            onClose={() => setShowSettings(false)}
            initialSettings={settings}
          />
        )}

        {/* ── LAUNCHER MODE: empty state ────────────────────────────────── */}
        {showEmptyState && (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              padding: '0 24px 16px',
              opacity: 0.45,
            }}
          >
            <span style={{ fontSize: '11.5px', color: colors.sub, textAlign: 'center', letterSpacing: '0.01em' }}>
              OmniLauncher — type to search, ? for AI, /skill for skills
            </span>
          </div>
        )}

        {/* ── LAUNCHER MODE: results list ───────────────────────────────── */}
        {!isAiMode && !showSettings && results.length > 0 && (
          <ResultList
            results={results}
            query={query}
            onExecute={handleExecute}
            colors={colors}
          />
        )}

        {/* ── Search / input bar (always at bottom) ────────────────────── */}
        <SearchBar
          value={query}
          onChange={handleQueryChange}
          onSubmit={handleSubmit}
          isAiMode={isAiMode}
          loading={loading}
          colors={colors}
          onSettingsClick={() => setShowSettings(s => !s)}
          showHintBar={!isAiMode && query === '' && !showSettings}
          inputRef={inputRef}
        />
      </div>
    </>
  )
}

// ─── Chat bubble sub-component ─────────────────────────────────────────────

function toolIcon(tool: string): string {
  if (tool.startsWith('🎯')) return ''
  if (tool.includes('file')) return '📁'
  if (tool.includes('web') || tool.includes('search')) return '🌐'
  if (tool.includes('calc')) return '🧮'
  if (tool.includes('shell') || tool.includes('exec')) return '🔧'
  if (tool.includes('app')) return '🚀'
  if (tool.includes('clip')) return '📋'
  return '🔧'
}

function ChatBubble({ turn, colors }: { turn: ConversationTurn; colors: typeof DARK_COLORS }) {
  const isUser = turn.role === 'user'

  return (
    <div
      className="omni-bubble-enter"
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: isUser ? 'flex-end' : 'flex-start',
        gap: '5px',
      }}
    >
      {/* Tool chips — only for assistant, shown above the bubble */}
      {!isUser && turn.tools_used && turn.tools_used.length > 0 && (
        <div style={{ display: 'flex', gap: '5px', flexWrap: 'wrap', paddingLeft: '4px' }}>
          {turn.tools_used.map((tool, i) => {
            const isSkill = tool.startsWith('🎯')
            return (
              <span
                key={i}
                style={{
                  fontSize: '11px',
                  background: isSkill ? `${colors.accent}20` : `${colors.surface2}CC`,
                  border: isSkill ? `1px solid ${colors.accent}55` : `1px solid ${colors.surface2}`,
                  padding: '2px 8px',
                  borderRadius: '10px',
                  color: isSkill ? colors.accent : colors.sub,
                  fontWeight: 500,
                  letterSpacing: '0.02em',
                }}
              >
                {isSkill ? tool : `${toolIcon(tool)} ${tool}`}
              </span>
            )
          })}
        </div>
      )}

      {/* Bubble */}
      <div
        style={{
          maxWidth: '78%',
          padding: isUser ? '9px 14px' : '10px 14px',
          borderRadius: isUser ? '16px 16px 4px 16px' : '4px 16px 16px 16px',
          background: isUser ? colors.userBubble : colors.aiBubble,
          color: isUser ? colors.userBubbleText : colors.aiText,
          fontSize: '14px',
          lineHeight: '1.65',
          wordBreak: 'break-word',
          // Assistant bubble gets a subtle accent left border
          borderLeft: !isUser ? `3px solid ${colors.accent}55` : 'none',
          boxShadow: isUser
            ? `0 2px 8px ${colors.userBubble}44`
            : '0 1px 4px rgba(0,0,0,0.15)',
        }}
      >
        {turn.isStreaming ? (
          <LoadingDots color={colors.sub} />
        ) : isUser ? (
          <span>{turn.content}</span>
        ) : (
          <span
            className={turn.isStreaming ? 'omni-cursor' : ''}
            dangerouslySetInnerHTML={{ __html: renderMarkdown(turn.content) }}
          />
        )}
      </div>
    </div>
  )
}

// ─── Loading dots (3-dot pulse) ────────────────────────────────────────────

function LoadingDots({ color }: { color: string }) {
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: '5px', padding: '2px 0', height: '20px' }}>
      {[0, 1, 2].map(i => (
        <span
          key={i}
          style={{
            width: '7px',
            height: '7px',
            borderRadius: '50%',
            background: color,
            display: 'inline-block',
            animation: `omni-dot-pulse 1.4s ease-in-out ${i * 0.2}s infinite`,
          }}
        />
      ))}
    </span>
  )
}
