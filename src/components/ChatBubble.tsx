import { memo, useMemo } from "react";
import { renderMarkdown } from "../utils/markdown";
import { toolIcon } from "../features/ai/toolIcon";
import type { ConversationTurn } from "../types/app";

const ChatBubble = memo(function ChatBubble({
  turn,
}: {
  turn: ConversationTurn;
}) {
  const isUser = turn.role === "user";
  // Memoize the expensive markdown render so streaming a sibling bubble doesn't re-render this one.
  const renderedHtml = useMemo(
    () => (isUser ? null : renderMarkdown(turn.content)),
    [isUser, turn.content],
  );

  return (
    <div
      className="omni-bubble-enter"
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: isUser ? "flex-end" : "flex-start",
        gap: "5px",
      }}
    >
      {/* Tool chips — only for assistant, shown above the bubble */}
      {!isUser && turn.tools_used && turn.tools_used.length > 0 && (
        <div
          style={{
            display: "flex",
            gap: "5px",
            flexWrap: "wrap",
            paddingLeft: "4px",
          }}
        >
          {turn.tools_used.map((tool, i) => {
            const isSkill = tool.startsWith("🎯");
            const isActiveLast =
              turn.isStreaming && i === turn.tools_used!.length - 1;
            return (
              <span
                key={i}
                className={
                  isActiveLast
                    ? "chat-msg__tool-badge chat-msg__tool-badge--active"
                    : isSkill
                      ? "chat-msg__tool-badge chat-msg__tool-badge--skill"
                      : "chat-msg__tool-badge"
                }
              >
                {isSkill ? tool : `${toolIcon(tool)} ${tool}`}
              </span>
            );
          })}
        </div>
      )}

      {/* Bubble */}
      <div
        style={{
          maxWidth: "78%",
          padding: isUser ? "9px 14px" : "10px 14px",
          borderRadius: isUser ? "16px 16px 4px 16px" : "4px 16px 16px 16px",
          background: isUser ? "var(--user-bubble)" : "var(--ai-bubble)",
          color: isUser ? "var(--user-bubble-text)" : "var(--ai-text)",
          fontSize: "14px",
          lineHeight: "1.65",
          wordBreak: "break-word",
          // Assistant bubble gets a subtle accent left border
          borderLeft: !isUser
            ? `3px solid color-mix(in srgb, var(--accent) 33%, transparent)`
            : "none",
          boxShadow: isUser
            ? `0 2px 8px color-mix(in srgb, var(--user-bubble) 27%, transparent)`
            : "0 1px 4px rgba(0,0,0,0.15)",
        }}
      >
        {turn.isStreaming ? (
          <LoadingDots color="var(--sub)" />
        ) : isUser ? (
          <span>{turn.content}</span>
        ) : (
          <span
            className={turn.isStreaming ? "omni-cursor" : ""}
            dangerouslySetInnerHTML={{ __html: renderedHtml || "" }}
          />
        )}
      </div>
    </div>
  );
});

export default ChatBubble;

// ─── Loading dots (3-dot pulse) ────────────────────────────────────────────

export function LoadingDots({ color }: { color: string }) {
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "5px",
        padding: "2px 0",
        height: "20px",
      }}
    >
      {[0, 1, 2].map((i) => (
        <span
          key={i}
          style={{
            width: "7px",
            height: "7px",
            borderRadius: "50%",
            background: color,
            display: "inline-block",
            animation: `omni-dot-pulse 1.4s ease-in-out ${i * 0.2}s infinite`,
          }}
        />
      ))}
    </span>
  );
}
