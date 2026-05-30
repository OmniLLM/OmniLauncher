/**
 * AIResponsePane — legacy standalone pane, kept for compatibility.
 *
 * In the redesigned dual-mode layout, chat rendering lives inside App.tsx's
 * <ChatBubble> components. This file is preserved so nothing breaks if it is
 * still imported, but it renders nothing by default.
 *
 * If you need to use the streaming Tauri events in the chat bubble approach,
 * wire the `listen('ai-stream', …)` listeners inside App.tsx instead.
 */

interface AiResponse {
  content: string;
  tools_used: string[];
  results: unknown[];
  is_ai: boolean;
}

interface Props {
  response: AiResponse | null;
}

// eslint-disable-next-line @typescript-eslint/no-unused-vars
export default function AIResponsePane(_props: Props) {
  // No-op: chat rendering is handled by App.tsx ChatBubble + conversationHistory.
  return null;
}
