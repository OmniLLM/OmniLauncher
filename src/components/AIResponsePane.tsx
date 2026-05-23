interface AiResponse {
  content: string
  tools_used: string[]
  results: unknown[]
  is_ai: boolean
}

interface Props {
  response: AiResponse
  colors: Record<string, string>
}

function toolIcon(tool: string): string {
  if (tool.includes('file')) return '📁'
  if (tool.includes('web') || tool.includes('search')) return '🔍'
  if (tool.includes('calc')) return '🧮'
  if (tool.includes('shell')) return '💻'
  if (tool.includes('app')) return '🚀'
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
  const handleCopy = () => {
    navigator.clipboard.writeText(response.content).catch(() => {})
  }

  return (
    <div style={{ padding: '16px', overflow: 'auto', maxHeight: '520px' }}>
      {response.tools_used.length > 0 && (
        <div style={{ marginBottom: '12px', display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
          {response.tools_used.map((tool, i) => (
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
        dangerouslySetInnerHTML={{ __html: renderMarkdown(response.content) }}
      />

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
    </div>
  )
}
