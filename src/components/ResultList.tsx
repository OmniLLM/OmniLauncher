import { useState, useEffect, useCallback } from 'react'

interface QueryResult {
  id: string
  title: string
  subtitle?: string
  icon?: string
  score: number
  action_type: string
  action_data: string
}

interface Props {
  results: QueryResult[]
  query: string
  onExecute: (r: QueryResult) => void
  colors: Record<string, string>
}

export default function ResultList({ results, query, onExecute, colors }: Props) {
  const [selected, setSelected] = useState(0)

  useEffect(() => {
    setSelected(0)
  }, [results])

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setSelected(s => Math.min(s + 1, results.length - 1))
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      setSelected(s => Math.max(s - 1, 0))
    } else if (e.key === 'Enter') {
      if (results[selected]) onExecute(results[selected])
    }
  }, [results, selected, onExecute])

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [handleKeyDown])

  function highlight(text: string, query: string): string {
    if (!query) return text
    const idx = text.toLowerCase().indexOf(query.toLowerCase())
    if (idx === -1) return text
    return text.slice(0, idx) + '<mark>' + text.slice(idx, idx + query.length) + '</mark>' + text.slice(idx + query.length)
  }

  return (
    <div style={{ overflow: 'auto', maxHeight: '480px' }}>
      {results.map((r, i) => (
        <div
          key={r.id}
          onClick={() => onExecute(r)}
          onMouseEnter={() => setSelected(i)}
          style={{
            display: 'flex',
            alignItems: 'center',
            padding: '10px 16px',
            gap: '12px',
            cursor: 'pointer',
            background: i === selected ? colors.surface : 'transparent',
            transition: 'background 0.1s'
          }}
        >
          <span style={{ fontSize: '20px', width: '24px', textAlign: 'center' }}>
            {r.icon || '📄'}
          </span>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div
              style={{ fontWeight: 500, fontSize: '14px', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
              dangerouslySetInnerHTML={{ __html: highlight(r.title, query) }}
            />
            {r.subtitle && (
              <div style={{ fontSize: '12px', color: colors.sub, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {r.subtitle}
              </div>
            )}
          </div>
          <div style={{ fontSize: '11px', color: colors.sub, opacity: 0.6 }}>
            {r.action_type}
          </div>
        </div>
      ))}
    </div>
  )
}
