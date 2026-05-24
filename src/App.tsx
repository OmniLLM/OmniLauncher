import { useState, useEffect, useCallback, useRef } from 'react'
import { invoke } from '@tauri-apps/api/core'
import SearchBar from './components/SearchBar'
import ResultList from './components/ResultList'
import AIResponsePane from './components/AIResponsePane'
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
}

/**
 * Detect if the user typed an explicit AI prefix.
 * Mirrors Router::decide() in router.rs.
 *
 * Prefixes: `?`  or  `ai ` (case-insensitive)
 *
 * Everything else → local plugins (instant, no AI latency).
 */
function isAiPrefix(input: string): boolean {
  const t = input.trim()
  return t.startsWith('?') || t.toLowerCase().startsWith('ai ')
}

export default function App() {
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<QueryResult[]>([])
  const [aiResponse, setAiResponse] = useState<AiResponse | null>(null)
  const [loading, setLoading] = useState(false)
  const [showSettings, setShowSettings] = useState(false)
  const [theme, setTheme] = useState<'dark' | 'light'>('dark')
  const [conversationHistory, setConversationHistory] = useState<ConversationTurn[]>([])
  const [showHistory, setShowHistory] = useState(false)
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const doSearch = useCallback(async (q: string) => {
    if (!q.trim()) {
      setResults([])
      setAiResponse(null)
      return
    }
    // If user typed an AI prefix, don't run local search
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
    setLoading(true)
    setResults([])
    try {
      const res = await invoke<AiResponse>('ai_query', { query: q })
      setAiResponse(res)
      setConversationHistory(prev => [
        ...prev,
        { role: 'user', content: q },
        { role: 'assistant', content: res.content }
      ])
    } catch (e) {
      const errResp: AiResponse = {
        content: `Error: ${e}`,
        tools_used: [],
        results: [],
        is_ai: true
      }
      setAiResponse(errResp)
    } finally {
      setLoading(false)
    }
  }, [])

  const handleQueryChange = useCallback((value: string) => {
    setQuery(value)
    setAiResponse(null)

    if (debounceRef.current) clearTimeout(debounceRef.current)

    // Debounce local search; AI is only triggered on explicit submit
    debounceRef.current = setTimeout(() => {
      doSearch(value)
    }, 100) // tighter debounce for instant feel
  }, [doSearch])

  /**
   * Submission (Enter key):
   * - forceAi=true  → Ctrl+Enter, always AI
   * - isAiPrefix    → user typed `?` or `ai ` prefix
   * - results exist → execute top result (handled in ResultList via keyboard)
   * - no results    → do nothing (don't fall back to AI automatically)
   */
  const handleSubmit = useCallback((value: string, forceAi: boolean) => {
    if (forceAi || isAiPrefix(value)) {
      if (debounceRef.current) clearTimeout(debounceRef.current)
      doAiQuery(value)
    }
    // If there are local results, Enter is handled by ResultList (execute top item)
  }, [doAiQuery])

  const handleNewConversation = useCallback(async () => {
    try {
      await invoke('clear_conversation')
    } catch (e) {
      console.error('clear_conversation error:', e)
    }
    setConversationHistory([])
    setAiResponse(null)
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

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setQuery('')
        setResults([])
        setAiResponse(null)
      }
      if (e.key === ',' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault()
        setShowSettings(s => !s)
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])

  const colors = theme === 'dark'
    ? { bg: '#1E1E2E', surface: '#313244', text: '#CDD6F4', accent: '#CBA6F7', sub: '#6C7086' }
    : { bg: '#EFF1F5', surface: '#CCD0DA', text: '#4C4F69', accent: '#8839EF', sub: '#9CA0B0' }

  const recentHistory = conversationHistory.slice(-6) // last 3 turns

  // Derive hint text shown below the search bar
  const hintText = (() => {
    if (!query) return null
    if (isAiPrefix(query)) return '🤖 AI mode — press Enter to send'
    if (results.length > 0) return null
    return null
  })()

  return (
    <div style={{
      width: '680px',
      minHeight: '60px',
      maxHeight: '600px',
      background: colors.bg,
      color: colors.text,
      fontFamily: "'Inter', 'Segoe UI', system-ui, sans-serif",
      borderRadius: '12px',
      overflow: 'hidden',
      boxShadow: '0 20px 60px rgba(0,0,0,0.5)',
      display: 'flex',
      flexDirection: 'column'
    }}>
      {/* Conversation history strip */}
      {conversationHistory.length > 0 && (
        <div style={{ padding: '8px 16px', borderBottom: `1px solid ${colors.surface}` }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <button
              onClick={() => setShowHistory(s => !s)}
              style={{ background: 'none', border: 'none', color: colors.sub, cursor: 'pointer', fontSize: '12px' }}
            >
              {showHistory ? '▲' : '▼'} History ({Math.floor(conversationHistory.length / 2)} turns)
            </button>
            <button
              onClick={handleNewConversation}
              style={{
                background: colors.surface, border: 'none', borderRadius: '6px',
                padding: '3px 10px', color: colors.text, cursor: 'pointer', fontSize: '12px'
              }}
            >
              ✨ New conversation
            </button>
          </div>
          {showHistory && (
            <div style={{ marginTop: '8px', maxHeight: '120px', overflow: 'auto' }}>
              {recentHistory.map((turn, i) => (
                <div key={i} style={{
                  fontSize: '12px', color: turn.role === 'user' ? colors.accent : colors.text,
                  marginBottom: '4px', padding: '2px 0'
                }}>
                  <strong>{turn.role === 'user' ? '👤' : '🤖'}</strong>{' '}
                  {turn.content.slice(0, 80)}{turn.content.length > 80 ? '…' : ''}
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      <SearchBar
        value={query}
        onChange={handleQueryChange}
        onSubmit={handleSubmit}
        isAiMode={isAiPrefix(query)}
        loading={loading}
        colors={colors}
        onSettingsClick={() => setShowSettings(s => !s)}
      />

      {/* Inline hint when AI mode is active */}
      {hintText && (
        <div style={{
          padding: '4px 16px 6px',
          fontSize: '11px',
          color: colors.sub,
          borderBottom: `1px solid ${colors.surface}`
        }}>
          {hintText}
        </div>
      )}

      {showSettings ? (
        <SettingsPanel
          colors={colors}
          theme={theme}
          onThemeChange={setTheme}
          onClose={() => setShowSettings(false)}
        />
      ) : loading ? (
        <div style={{ padding: '20px', color: colors.sub, textAlign: 'center' }}>
          🤖 Thinking...
        </div>
      ) : aiResponse ? (
        <AIResponsePane response={aiResponse} colors={colors} />
      ) : results.length > 0 ? (
        <ResultList
          results={results}
          query={query}
          onExecute={handleExecute}
          colors={colors}
        />
      ) : null}
    </div>
  )
}
