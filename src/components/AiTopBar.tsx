import SessionPicker, { type SessionPickerProps } from "./SessionPicker";

export interface AiTopBarProps extends SessionPickerProps {}

export default function AiTopBar(props: AiTopBarProps) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "10px 16px 0",
        flexShrink: 0,
      }}
    >
      <span
        style={{
          fontSize: "13px",
          color: "var(--accent)",
          fontWeight: 600,
          letterSpacing: "0.03em",
        }}
      >
        OMNILAUNCHER AI MODE
      </span>
      <SessionPicker {...props} />
    </div>
  );
}
