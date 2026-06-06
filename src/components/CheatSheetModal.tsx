export interface CheatSheetModalProps {
  open: boolean;
  onClose: () => void;
}

const SHORTCUTS: Array<[string, string]> = [
  ["Ctrl+K", "Toggle AI mode"],
  ["Ctrl+,", "Open Settings"],
  ["Ctrl+0", "Reset window size"],
  ["F1", "Show/hide this help"],
  ["Escape", "Clear / Hide window"],
  ["↑ / ↓", "Navigate results"],
  ["↑ (empty)", "Browse input history"],
  ["Enter", "Execute selected"],
  ["Ctrl+Enter", "Force AI query"],
  ["?", "Toggle AI mode (key)"],
  ["/help", "Show all commands"],
  ["/new", "New AI conversation"],
  ["/plugins", "Plugin manager"],
  ["/skills", "Skill manager"],
  ["Right-click", "Context menu on result"],
  ["★ (hover)", "Favorite a result"],
];

export default function CheatSheetModal({ open, onClose }: CheatSheetModalProps) {
  if (!open) return null;
  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 9999,
        background: "rgba(0,0,0,0.7)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        animation: "omni-fade-in 150ms ease both",
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          background: "var(--bg-elevated)",
          border: "1px solid var(--border)",
          borderRadius: 14,
          padding: "20px 28px",
          minWidth: 320,
          boxShadow: "0 24px 64px rgba(0,0,0,0.6)",
        }}
      >
        <div
          style={{
            fontSize: 13,
            fontWeight: 700,
            color: "var(--text)",
            marginBottom: 16,
            letterSpacing: "0.04em",
          }}
        >
          ⌨ Keyboard Shortcuts
        </div>
        {SHORTCUTS.map(([key, desc]) => (
          <div
            key={key}
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              padding: "5px 0",
              borderBottom: "1px solid rgba(255,255,255,0.04)",
            }}
          >
            <kbd
              style={{
                fontFamily: "monospace",
                fontSize: 11,
                background: "rgba(255,255,255,0.08)",
                border: "1px solid rgba(255,255,255,0.15)",
                borderRadius: 5,
                padding: "2px 8px",
                color: "var(--accent)",
              }}
            >
              {key}
            </kbd>
            <span
              style={{
                fontSize: 12,
                color: "var(--text-secondary)",
                marginLeft: 16,
              }}
            >
              {desc}
            </span>
          </div>
        ))}
        <div
          style={{
            fontSize: 11,
            color: "var(--sub)",
            marginTop: 12,
            textAlign: "center",
          }}
        >
          Press F1 or click outside to close
        </div>
      </div>
    </div>
  );
}
