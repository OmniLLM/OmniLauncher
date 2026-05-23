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

export default function App() {
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<QueryResult[]>([])
  const [aiResponse, setAiResponse] = useState<AiResponse | null>(null)
  const [loading, setLoading] = useState(false)
  const [showSettings, setShowSettings] = useState(false)
  const [theme, setTheme] = useState<'dark' | 'light'>('dark')
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const isNaturalLanguage = useCallback((input: string): boolean => {
    if (input.length > 20) return true
    if (input.includes(' ')) {
      const nlWords = ['find', 'show', 'open', 'search', 'what', 'how', 'why', 'who',
                       'when', 'where', 'help', 'get', 'list', 'create', 'make',
                       'tell', 'explain', 'translate', 'calculate', 'convert',
                       '找', '帮', '搜', '查', '打开']
      const lower = input.toLowerCase()
      return nlWords.some(w => lower.includes(w))
    }
    return false
  }, [])

  const doSearch = useCallback(async (q: string) => {
    if (!q.trim()) {
      setResults([])
      setAiResponse(null)
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
    } catch (e) {
      setAiResponse({
        content: `Error: ${e}`,
        tools_used: [],
        results: [],
        is_ai: true
      })
    } finally {
      setLoading(false)
    }
  }, [])

  const handleQueryChange = useCallback((value: string) => {
    setQuery(value)
    setAiResponse(null)

    if (debounceRef.current) clearTimeout(debounceRef.current)
    debounceRef.current = setTimeout(() => {
      doSearch(value)
    }, 150)
  }, [doSearch])

  const handleSubmit = useCallback((value: string, forceAi: boolean) => {
    if (forceAi || isNaturalLanguage(value)) {
      if (debounceRef.current) clearTimeout(debounceRef.current)
      doAiQuery(value)
    }
  }, [isNaturalLanguage, doAiQuery])

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
      <SearchBar
        value={query}
        onChange={handleQueryChange}
        onSubmit={handleSubmit}
        isNatural={isNaturalLanguage(query)}
        loading={loading}
        colors={colors}
        onSettingsClick={() => setShowSettings(s => !s)}
      />

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
