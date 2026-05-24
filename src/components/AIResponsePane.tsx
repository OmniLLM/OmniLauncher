import { useState, useEffect, useRef } from 'react'
import { listen } from '@tauri-apps/api/event'

interface AiResponse {
  content: string
  tools_used: string[]
  results: unknown[]
  is_ai: boolean
}

interface Props {
  response: AiResponse | null
  colors: Record<string, string>
}

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
    .replace(/`(.+?)`/g, '<code style="background:rgba(255,255,255,0.1);padding:2px 6px;border-radius:4px">$1</code>')
    .replace(/\n/g, '<br/>')
}

export default function AIResponsePane({ response, colors }: Props) {
  const [streamedContent, setStreamedContent] = useState('')
  const [isStreaming, setIsStreaming] = useState(false)
  const [streamTools, setStreamTools] = useState<string[]>([])
  const unlistenRefs = useRef<Array<() => void>>([])

  useEffect(() => {
    // Clean up previous listeners
    unlistenRefs.current.forEach(fn => fn())
    unlistenRefs.current = []

    // Reset state
    setStreamedContent('')
    setIsStreaming(true)
    setStreamTools([])

    const setupListeners = async () => {
      const unlistenStream = await listen<string>('ai-stream', (event) => {
        setStreamedContent(prev => prev + event.payload)
      })

      const unlistenDone = await listen<string>('ai-stream-done', () => {
        setIsStreaming(false)
      })

      const unlistenTool = await listen<string>('ai-tool-call', (event) => {
        setStreamTools(prev => [...prev, event.payload])
      })

      unlistenRefs.current = [unlistenStream, unlistenDone, unlistenTool]
    }

    setupListeners()

    return () => {
      unlistenRefs.current.forEach(fn => fn())
    }
  }, [response])

  const allTools = [...(response?.tools_used ?? []), ...streamTools]
  const displayContent = streamedContent || response?.content || ''

  const handleCopy = () => {
    navigator.clipboard.writeText(displayContent).catch(() => {})
  }

  return (
    <div style={{ padding: '16px', overflow: 'auto', maxHeight: '520px' }}>
      {allTools.length > 0 && (
        <div style={{ marginBottom: '12px', display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
          {allTools.map((tool, i) => (
            <span
              key={i}
              style={{
                fontSize: '12px',
                background: colors.surface,
                padding: '3px 8px',
                borderRadius: '12px',
                color: colors.sub
              }}
            >
              {toolIcon(tool)} {tool}
            </span>
          ))}
        </div>
      )}

      <div
        style={{
          fontSize: '14px',
          lineHeight: '1.7',
          color: colors.text,
          whiteSpace: 'pre-wrap'
        }}
        dangerouslySetInnerHTML={{
          __html: renderMarkdown(displayContent) + (isStreaming ? '<span class="blink-cursor">▋</span>' : '')
        }}
      />

      {isStreaming && (
        <div style={{ color: colors.sub, fontSize: '12px', marginTop: '8px' }}>
          ✨ Streaming...
        </div>
      )}

      <div style={{ marginTop: '12px', display: 'flex', justifyContent: 'flex-end' }}>
        <button
          onClick={handleCopy}
          style={{
            background: colors.surface,
            border: 'none',
            borderRadius: '6px',
            padding: '6px 12px',
            color: colors.text,
            cursor: 'pointer',
            fontSize: '12px'
          }}
        >
          📋 Copy
        </button>
      </div>

      <style>{`
        @keyframes blink { 0%, 100% { opacity: 1; } 50% { opacity: 0; } }
        .blink-cursor { animation: blink 1s step-end infinite; }
      `}</style>
    </div>
  )
}
