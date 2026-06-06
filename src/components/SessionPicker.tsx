import type { AiSessionInfo } from "../types/app";

export interface SessionPickerProps {
  sessions: AiSessionInfo[];
  currentSessionId: number | null;
  showSessionPicker: boolean;
  setShowSessionPicker: React.Dispatch<React.SetStateAction<boolean>>;
  handleNewConversation: () => void;
  handleSwitchSession: (id: number) => void;
  handleDeleteSession: (id: number) => void;
}

export default function SessionPicker({
  sessions,
  currentSessionId,
  showSessionPicker,
  setShowSessionPicker,
  handleNewConversation,
  handleSwitchSession,
  handleDeleteSession,
}: SessionPickerProps) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: "8px",
        position: "relative",
      }}
    >
      <button
        onClick={() => setShowSessionPicker((v) => !v)}
        title="Switch sessions"
        style={{
          background: showSessionPicker
            ? "var(--surface-2)"
            : "var(--surface)",
          border: "none",
          borderRadius: "7px",
          padding: "4px 11px",
          color: "var(--text)",
          cursor: "pointer",
          fontSize: "12px",
          display: "flex",
          alignItems: "center",
          gap: "6px",
          maxWidth: "240px",
        }}
        onMouseEnter={(e) =>
          (e.currentTarget.style.background = "var(--surface-2)")
        }
        onMouseLeave={(e) =>
          (e.currentTarget.style.background = showSessionPicker
            ? "var(--surface-2)"
            : "var(--surface)")
        }
      >
        <span style={{ fontSize: "10px" }}>💬</span>
        <span
          style={{
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            maxWidth: "180px",
          }}
        >
          {(() => {
            const cur = sessions.find((s) => s.id === currentSessionId);
            return cur && cur.title
              ? cur.title
              : currentSessionId
                ? `Session #${currentSessionId}`
                : "Session";
          })()}
        </span>
        <span style={{ fontSize: "9px", opacity: 0.6 }}>▾</span>
      </button>
      <button
        onClick={handleNewConversation}
        style={{
          background: "var(--surface)",
          border: "none",
          borderRadius: "7px",
          padding: "4px 11px",
          color: "var(--text)",
          cursor: "pointer",
          fontSize: "12px",
          display: "flex",
          alignItems: "center",
          gap: "5px",
        }}
        onMouseEnter={(e) =>
          (e.currentTarget.style.background = "var(--surface-2)")
        }
        onMouseLeave={(e) =>
          (e.currentTarget.style.background = "var(--surface)")
        }
      >
        <span style={{ fontSize: "10px" }}>✦</span> New conversation
      </button>

      {showSessionPicker && (
        <div
          style={{
            position: "absolute",
            top: "calc(100% + 6px)",
            right: 0,
            width: "320px",
            maxHeight: "360px",
            overflowY: "auto",
            background: "var(--surface)",
            border: `1px solid var(--surface-2)`,
            borderRadius: "10px",
            boxShadow: "0 12px 32px rgba(0,0,0,0.35)",
            zIndex: 50,
            padding: "6px",
          }}
          onClick={(e) => e.stopPropagation()}
        >
          {sessions.length === 0 && (
            <div
              style={{
                padding: "10px 12px",
                fontSize: "12px",
                color: "var(--sub)",
              }}
            >
              No sessions yet.
            </div>
          )}
          {sessions.map((s) => {
            const active = s.id === currentSessionId;
            return (
              <div
                key={s.id}
                onClick={() => handleSwitchSession(s.id)}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "8px",
                  padding: "7px 9px",
                  borderRadius: "7px",
                  cursor: "pointer",
                  background: active ? "var(--surface-2)" : "transparent",
                }}
                onMouseEnter={(e) =>
                  (e.currentTarget.style.background = "var(--surface-2)")
                }
                onMouseLeave={(e) =>
                  (e.currentTarget.style.background = active
                    ? "var(--surface-2)"
                    : "transparent")
                }
              >
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div
                    style={{
                      fontSize: "12.5px",
                      color: "var(--text)",
                      fontWeight: active ? 600 : 500,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {s.title || `Session #${s.id}`}
                  </div>
                  <div
                    style={{
                      fontSize: "10.5px",
                      color: "var(--sub)",
                      marginTop: "2px",
                      display: "flex",
                      gap: "8px",
                    }}
                  >
                    <span>{s.message_count} msg</span>
                    <span style={{ opacity: 0.6 }}>
                      {(s.last_active_at || "").slice(0, 16)}
                    </span>
                  </div>
                </div>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    handleDeleteSession(s.id);
                  }}
                  title="Delete session"
                  style={{
                    background: "transparent",
                    border: "none",
                    color: "var(--sub)",
                    cursor: "pointer",
                    fontSize: "13px",
                    padding: "2px 6px",
                    borderRadius: "5px",
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.background = "var(--danger)";
                    e.currentTarget.style.color = "#fff";
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = "transparent";
                    e.currentTarget.style.color = "var(--sub)";
                  }}
                >
                  ×
                </button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
