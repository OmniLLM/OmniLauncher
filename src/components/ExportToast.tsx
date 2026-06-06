export interface ExportToastProps {
  open: boolean;
}

export default function ExportToast({ open }: ExportToastProps) {
  if (!open) return null;
  return (
    <div
      style={{
        position: "fixed",
        bottom: 16,
        left: "50%",
        transform: "translateX(-50%)",
        background: "var(--bg-elevated)",
        border:
          "1px solid color-mix(in srgb, var(--accent) 40%, transparent)",
        borderRadius: 8,
        padding: "8px 16px",
        fontSize: 12,
        color: "var(--accent)",
        fontWeight: 600,
        zIndex: 9998,
        animation: "omni-fade-in 150ms ease both",
        pointerEvents: "none",
      }}
    >
      ✓ Conversation copied to clipboard
    </div>
  );
}
