export default function QueuedPromptBubble({ prompt }: { prompt: string }) {
  return (
    <div
      className="omni-bubble-enter"
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "flex-end",
        gap: "5px",
        opacity: 0.72,
      }}
    >
      <span
        style={{
          fontSize: "11px",
          color: "var(--sub)",
          paddingRight: "6px",
          letterSpacing: 0,
        }}
      >
        Queued
      </span>
      <div
        style={{
          maxWidth: "78%",
          padding: "9px 14px",
          borderRadius: "16px 16px 4px 16px",
          background: `color-mix(in srgb, var(--user-bubble) 50%, transparent)`,
          color: "var(--user-bubble-text)",
          fontSize: "14px",
          lineHeight: "1.65",
          wordBreak: "break-word",
          border: `1px dashed color-mix(in srgb, var(--accent) 40%, transparent)`,
        }}
      >
        {prompt}
      </div>
    </div>
  );
}
