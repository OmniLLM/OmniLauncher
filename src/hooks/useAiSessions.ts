import { useCallback, useEffect, useState, type MutableRefObject } from "react";
import { invoke } from "../lib/runtime";
import type { AiSessionInfo, ConversationTurn, QueryResult } from "../types/app";

export interface UseAiSessionsArgs {
  setConversationHistory: React.Dispatch<
    React.SetStateAction<ConversationTurn[]>
  >;
  pendingQueueRef: MutableRefObject<string[]>;
  setQueuedPrompts: React.Dispatch<React.SetStateAction<string[]>>;
  setQueueDepth: React.Dispatch<React.SetStateAction<number>>;
  setResults: React.Dispatch<React.SetStateAction<QueryResult[]>>;
  setQuery: React.Dispatch<React.SetStateAction<string>>;
}

export interface UseAiSessionsResult {
  sessions: AiSessionInfo[];
  currentSessionId: number | null;
  showSessionPicker: boolean;
  setShowSessionPicker: React.Dispatch<React.SetStateAction<boolean>>;
  refreshSessions: () => Promise<void>;
  handleNewConversation: () => Promise<void>;
  handleSwitchSession: (sessionId: number) => Promise<void>;
  handleDeleteSession: (sessionId: number) => Promise<void>;
}

export function useAiSessions(args: UseAiSessionsArgs): UseAiSessionsResult {
  const {
    setConversationHistory,
    pendingQueueRef,
    setQueuedPrompts,
    setQueueDepth,
    setResults,
    setQuery,
  } = args;

  const [sessions, setSessions] = useState<AiSessionInfo[]>([]);
  const [currentSessionId, setCurrentSessionId] = useState<number | null>(null);
  const [showSessionPicker, setShowSessionPicker] = useState(false);

  const refreshSessions = useCallback(async () => {
    try {
      const [list, cur] = await Promise.all([
        invoke<AiSessionInfo[]>("list_ai_sessions"),
        invoke<number>("current_ai_session"),
      ]);
      setSessions(list || []);
      setCurrentSessionId(cur || null);
    } catch (e) {
      console.error("refreshSessions error:", e);
    }
  }, []);

  // Load AI sessions on mount and rehydrate the active session's transcript.
  useEffect(() => {
    (async () => {
      try {
        const cur = await invoke<number>("current_ai_session");
        setCurrentSessionId(cur || null);
        if (cur) {
          const msgs = await invoke<Array<{ role: string; content: string }>>(
            "switch_ai_session",
            { sessionId: cur },
          );
          const turns: ConversationTurn[] = (msgs || [])
            .filter((m) => m.role === "user" || m.role === "assistant")
            .map((m) => ({
              role: m.role as "user" | "assistant",
              content: m.content,
            }));
          setConversationHistory(turns);
        }
        const list = await invoke<AiSessionInfo[]>("list_ai_sessions");
        setSessions(list || []);
      } catch (e) {
        console.error("session bootstrap error:", e);
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleNewConversation = useCallback(async () => {
    try {
      await invoke("clear_conversation");
    } catch (e) {
      console.error("clear_conversation error:", e);
    }
    setConversationHistory([]);
    pendingQueueRef.current = [];
    setQueuedPrompts([]);
    setQueueDepth(0);
    setResults([]);
    setQuery("");
    setShowSessionPicker(false);
    refreshSessions();
  }, [
    pendingQueueRef,
    refreshSessions,
    setConversationHistory,
    setQuery,
    setQueueDepth,
    setQueuedPrompts,
    setResults,
  ]);

  const handleSwitchSession = useCallback(
    async (sessionId: number) => {
      try {
        const msgs = await invoke<Array<{ role: string; content: string }>>(
          "switch_ai_session",
          { sessionId },
        );
        const turns: ConversationTurn[] = (msgs || [])
          .filter((m) => m.role === "user" || m.role === "assistant")
          .map((m) => ({
            role: m.role as "user" | "assistant",
            content: m.content,
          }));
        setConversationHistory(turns);
        pendingQueueRef.current = [];
        setQueuedPrompts([]);
        setQueueDepth(0);
        setCurrentSessionId(sessionId);
      } catch (e) {
        console.error("switch_ai_session error:", e);
      }
      setShowSessionPicker(false);
      setResults([]);
      setQuery("");
      refreshSessions();
    },
    [
      pendingQueueRef,
      refreshSessions,
      setConversationHistory,
      setQuery,
      setQueueDepth,
      setQueuedPrompts,
      setResults,
    ],
  );

  const handleDeleteSession = useCallback(
    async (sessionId: number) => {
      try {
        const newCur = await invoke<number>("delete_ai_session", { sessionId });
        if (currentSessionId === sessionId) {
          setConversationHistory([]);
          pendingQueueRef.current = [];
          setQueuedPrompts([]);
          setQueueDepth(0);
          setCurrentSessionId(newCur || null);
        }
      } catch (e) {
        console.error("delete_ai_session error:", e);
      }
      refreshSessions();
    },
    [
      currentSessionId,
      pendingQueueRef,
      refreshSessions,
      setConversationHistory,
      setQueueDepth,
      setQueuedPrompts,
    ],
  );

  return {
    sessions,
    currentSessionId,
    showSessionPicker,
    setShowSessionPicker,
    refreshSessions,
    handleNewConversation,
    handleSwitchSession,
    handleDeleteSession,
  };
}
