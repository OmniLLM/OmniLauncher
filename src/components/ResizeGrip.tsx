export interface ResizeGripProps {
  onPointerDown: (e: React.PointerEvent<HTMLDivElement>) => void;
  onDoubleClick: () => void;
}

export default function ResizeGrip({
  onPointerDown,
  onDoubleClick,
}: ResizeGripProps) {
  return (
    <div
      onPointerDown={onPointerDown}
      onDoubleClick={onDoubleClick}
      title="Drag to resize · Double-click or Ctrl+0 to reset"
      style={{
        position: "fixed",
        right: 0,
        bottom: 0,
        width: "18px",
        height: "18px",
        cursor: "nwse-resize",
        zIndex: 9990,
        background:
          "linear-gradient(135deg, transparent 0 45%, color-mix(in srgb, var(--text) 35%, transparent) 45% 55%, transparent 55% 70%, color-mix(in srgb, var(--text) 35%, transparent) 70% 80%, transparent 80%)",
        opacity: 0.5,
      }}
      onMouseEnter={(e) => (e.currentTarget.style.opacity = "0.9")}
      onMouseLeave={(e) => (e.currentTarget.style.opacity = "0.5")}
    />
  );
}
