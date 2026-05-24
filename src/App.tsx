import { useState, useEffect, useCallback, useRef } from 'react'
import { invoke } from '@tauri-apps/api/core'
import SearchBar from './components/SearchBar'
import ResultList from './components/ResultList'
import SettingsPanel from './components/SettingsPanel'

interface QueryResult {
  id: string
  title: string
  subtitle?: string
  icon?: string
  score: number
  action_type: string
  action_data: string
}

interface AiResponse {
  content: string
  tools_used: string[]
  results: QueryResult[]
  is_ai: boolean
}

interface ConversationTurn {
  role: 'user' | 'assistant'
  content: string
  tools_used?: string[]
  isStreaming?: boolean
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

export default function App() {
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<QueryResult[]>([])
  const [loading, setLoading] = useState(false)
  const [showSettings, setShowSettings] = useState(false)
  const [theme, setTheme] = useState<'dark' | 'light'>('dark')
  const [conversationHistory, setConversationHistory] = useState<ConversationTurn[]>([])
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const chatScrollRef = useRef<HTMLDivElement>(null)

  const colors = theme === 'dark' ? DARK_COLORS : LIGHT_COLORS

  const isAiMode = isAiPrefix(query)

  const doSearch = useCallback(async (q: string) => {
    if (!q.trim()) {
      setResults([])
      return
    }
    if (isAiPrefix(q)) {
      setResults([])
      return
    }
    try {
      const res = await invoke<QueryResult[]>('search', { query: q })
      setResults(res)
    } catch (e) {
      console.error('Search error:', e)
    }
  }, [])

  const doAiQuery = useCallback(async (q: string) => {
    if (!q.trim()) return

    // Append user message immediately
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
        // replace the last (streaming) assistant turn
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
      setQuery('') // clear input after sending
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

  // ─── Layout geometry ────────────────────────────────────────────────────────
  // Launcher mode: compact, grows with results.
  // AI chat mode: tall, fixed.
  const launcherHasContent = results.length > 0 || showSettings
  const windowHeight = isAiMode ? '560px' : launcherHasContent ? 'auto' : '64px'
  const maxHeight = isAiMode ? '560px' : '520px'

  return (
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
        // accent ring in AI mode
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
            // Custom scrollbar
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
              }}
            >
              <span style={{ fontSize: '32px', opacity: 0.4 }}>✦</span>
              <span style={{ fontSize: '13px' }}>Ask me anything — I can search the web, run calculations, and more.</span>
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
          colors={colors}
          theme={theme}
          onThemeChange={setTheme}
          onClose={() => setShowSettings(false)}
        />
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
      />
    </div>
  )
}

// ─── Chat bubble sub-component ─────────────────────────────────────────────

function toolIcon(tool: string): string {
  if (tool.includes('file')) return '📁'
  if (tool.includes('web') || tool.includes('search')) return '🔍'
  if (tool.includes('calc')) return '🧮'
  if (tool.includes('shell')) return '💻'
  if (tool.includes('app')) return '🚀'
  if (tool.includes('clip')) return '📋'
  return '🔧'
}

function renderMarkdown(text: string): string {
  return text
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/\*(.+?)\*/g, '<em>$1</em>')
    .replace(/`(.+?)`/g, '<code style="background:rgba(255,255,255,0.12);padding:2px 6px;border-radius:4px;font-size:0.92em">$1</code>')
    .replace(/\n/g, '<br/>')
}

function ChatBubble({ turn, colors }: { turn: ConversationTurn; colors: typeof DARK_COLORS }) {
  const isUser = turn.role === 'user'

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: isUser ? 'flex-end' : 'flex-start',
        gap: '4px',
      }}
    >
      {/* Tool chips — only for assistant */}
      {!isUser && turn.tools_used && turn.tools_used.length > 0 && (
        <div style={{ display: 'flex', gap: '6px', flexWrap: 'wrap', paddingLeft: '4px' }}>
          {turn.tools_used.map((tool, i) => (
            <span
              key={i}
              style={{
                fontSize: '11px',
                background: colors.surface2,
                padding: '2px 7px',
                borderRadius: '10px',
                color: colors.sub,
              }}
            >
              {toolIcon(tool)} {tool}
            </span>
          ))}
        </div>
      )}

      <div
        style={{
          maxWidth: '78%',
          padding: '9px 13px',
          borderRadius: isUser ? '14px 14px 4px 14px' : '14px 14px 14px 4px',
          background: isUser ? colors.userBubble : colors.aiBubble,
          color: isUser ? colors.userBubbleText : colors.aiText,
          fontSize: '14px',
          lineHeight: '1.65',
          wordBreak: 'break-word',
        }}
      >
        {turn.isStreaming ? (
          <LoadingDots color={colors.sub} />
        ) : isUser ? (
          <span>{turn.content}</span>
        ) : (
          <span dangerouslySetInnerHTML={{ __html: renderMarkdown(turn.content) }} />
        )}
      </div>
    </div>
  )
}

function LoadingDots({ color }: { color: string }) {
  return (
    <>
      <style>{`
        @keyframes omni-dot-bounce {
          0%, 80%, 100% { transform: translateY(0); opacity: 0.4; }
          40% { transform: translateY(-5px); opacity: 1; }
        }
        .omni-dot {
          display: inline-block;
          width: 6px; height: 6px;
          border-radius: 50%;
          margin: 0 2px;
          animation: omni-dot-bounce 1.2s infinite ease-in-out;
        }
        .omni-dot:nth-child(1) { animation-delay: 0s; }
        .omni-dot:nth-child(2) { animation-delay: 0.2s; }
        .omni-dot:nth-child(3) { animation-delay: 0.4s; }
      `}</style>
      <span aria-label="AI is thinking">
        <span className="omni-dot" style={{ background: color }} />
        <span className="omni-dot" style={{ background: color }} />
        <span className="omni-dot" style={{ background: color }} />
      </span>
    </>
  )
}
