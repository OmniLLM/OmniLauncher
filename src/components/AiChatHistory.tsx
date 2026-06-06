import { forwardRef } from "react";
import ChatBubble from "./ChatBubble";
import QueuedPromptBubble from "./QueuedPromptBubble";
import type { ConversationTurn } from "../types/app";

export interface AiChatHistoryProps {
  conversationHistory: ConversationTurn[];
  queuedPrompts: string[];
}

const AiChatHistory = forwardRef<HTMLDivElement, AiChatHistoryProps>(
  function AiChatHistory({ conversationHistory, queuedPrompts }, ref) {
    return (
      <div
        ref={ref}
        style={{
          flex: 1,
          overflowY: "auto",
          padding: "12px 16px",
          display: "flex",
          flexDirection: "column",
          gap: "10px",
          scrollbarWidth: "thin",
          scrollbarColor: `var(--surface-2) transparent`,
        }}
      >
        {conversationHistory.length === 0 && queuedPrompts.length === 0 && (
          <div
            style={{
              flex: 1,
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              justifyContent: "center",
              color: "var(--sub)",
              gap: "8px",
              paddingBottom: "24px",
              minHeight: "360px",
            }}
          >
            <span style={{ fontSize: "32px", opacity: 0.35 }}>✦</span>
            <span
              style={{
                fontSize: "13px",
                textAlign: "center",
                maxWidth: "280px",
                lineHeight: 1.6,
              }}
            >
              Ask me anything — I can search the web, run calculations, and
              more.
            </span>
          </div>
        )}

        {conversationHistory.map((turn, i) => (
          <ChatBubble key={i} turn={turn} />
        ))}

        {queuedPrompts.map((prompt, i) => (
          <QueuedPromptBubble
            key={`queued-${i}-${prompt}`}
            prompt={prompt}
          />
        ))}
      </div>
    );
  },
);

export default AiChatHistory;
