import { useCallback } from "react";
import { invoke, listen } from "../lib/runtime";
import type {
  AiResponse,
  ConversationTurn,
  QueryResult,
} from "../types/app";

export interface UseAiQueryArgs {
  loading: boolean;
  setLoading: React.Dispatch<React.SetStateAction<boolean>>;
  setResults: React.Dispatch<React.SetStateAction<QueryResult[]>>;
  setConversationHistory: React.Dispatch<
    React.SetStateAction<ConversationTurn[]>
  >;
  refreshSessions: () => Promise<void>;
  focusInput: (select?: boolean) => void;
  // Queue state lives in App so both useAiSessions and useAiQuery can mutate it
  // without creating a circular dep between the two hooks.
  pendingQueueRef: React.MutableRefObject<string[]>;
  setQueuedPrompts: React.Dispatch<React.SetStateAction<string[]>>;
  setQueueDepth: React.Dispatch<React.SetStateAction<number>>;
  cancelRequestedRef: React.MutableRefObject<boolean>;
  aiCleanupRef: React.MutableRefObject<(() => void) | null>;
}

export interface UseAiQueryResult {
  doAiQuery: (q: string) => Promise<void>;
  enqueueAiQuery: (value: string) => void;
  handleCancelAiRequest: () => void;
}

export function useAiQuery(args: UseAiQueryArgs): UseAiQueryResult {
  const {
    loading,
    setLoading,
    setResults,
    setConversationHistory,
    refreshSessions,
    focusInput,
    pendingQueueRef,
    setQueuedPrompts,
    setQueueDepth,
    cancelRequestedRef,
    aiCleanupRef,
  } = args;

  const doAiQuery = useCallback(
    async (q: string) => {
      if (!q.trim() || loading) return;
      cancelRequestedRef.current = false;

      const userTurn: ConversationTurn = { role: "user", content: q };
      const pendingAiTurn: ConversationTurn = {
        role: "assistant",
        content: "",
        tools_used: [],
        isStreaming: true,
      };
      setConversationHistory((prev) => [...prev, userTurn, pendingAiTurn]);
      setLoading(true);
      setResults([]);

      // Register listeners FIRST and await registration so we never miss
      // the ai-done / ai-error event that the backend may emit promptly.
      const unlisteners: (() => void)[] = [];

      const cleanup = () => {
        unlisteners.forEach((fn) => fn());
        unlisteners.length = 0;
        aiCleanupRef.current = null;
      };
      aiCleanupRef.current = cleanup;

      const finish = (content: string, tools_used?: string[]) => {
        const wasCancelled =
          cancelRequestedRef.current || content === "Error: Cancelled by user";
        cleanup();
        setConversationHistory((prev) => {
          const next = [...prev];
          const last = next[next.length - 1];
          if (last?.role === "assistant" && last.isStreaming) {
            next[next.length - 1] = {
              role: "assistant",
              content: wasCancelled ? "Cancelled." : content,
              tools_used: wasCancelled ? [] : tools_used,
              isStreaming: false,
            };
          }
          return next;
        });
        setLoading(false);
        if (!wasCancelled) {
          // Drain next queued prompt after loading state settles
          setTimeout(() => {
            const next = pendingQueueRef.current.shift();
            if (next) {
              setQueuedPrompts((prev) => prev.slice(1));
              setQueueDepth(pendingQueueRef.current.length);
              doAiQuery(next);
            }
          }, 50);
        }
        setTimeout(() => focusInput(), 50);
      };

      try {
        const [unToolCall, unDone, unError] = await Promise.all([
          listen<{ tool: string; iteration: number }>(
            "omnilauncher://ai-tool-call",
            (event) => {
              const toolName = event.payload.tool;
              setConversationHistory((prev) => {
                const next = [...prev];
                const last = next[next.length - 1];
                if (last?.role === "assistant" && last.isStreaming) {
                  next[next.length - 1] = {
                    ...last,
                    content: `🔧 Calling **${toolName}**…`,
                    tools_used: [...(last.tools_used ?? []), toolName],
                  };
                }
                return next;
              });
            },
          ),
          listen<AiResponse>("omnilauncher://ai-done", (event) => {
            finish(event.payload.content, event.payload.tools_used);
            refreshSessions();
          }),
          listen<string>("omnilauncher://ai-error", (event) => {
            finish(`Error: ${event.payload}`);
          }),
        ]);
        unlisteners.push(unToolCall, unDone, unError);
        if (cancelRequestedRef.current) {
          cleanup();
          return;
        }
      } catch (e) {
        finish(`Error: ${e}`);
        return;
      }

      try {
        await invoke("ai_query", { query: q });
      } catch (e) {
        finish(`Error: ${e}`);
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [focusInput, loading, refreshSessions],
  );

  const enqueueAiQuery = useCallback((value: string) => {
    pendingQueueRef.current.push(value);
    setQueuedPrompts((prev) => [...prev, value]);
    setQueueDepth(pendingQueueRef.current.length);
  }, []);

  const handleCancelAiRequest = useCallback(() => {
    cancelRequestedRef.current = true;
    pendingQueueRef.current = [];
    setQueuedPrompts([]);
    setQueueDepth(0);
    aiCleanupRef.current?.();

    setConversationHistory((prev) => {
      const next = [...prev];
      const last = next[next.length - 1];
      if (last?.role === "assistant" && last.isStreaming) {
        next[next.length - 1] = {
          role: "assistant",
          content: "Cancelled.",
          tools_used: [],
          isStreaming: false,
        };
      }
      return next;
    });
    setLoading(false);
    setTimeout(() => focusInput(), 50);

    invoke("ai_cancel").catch(() => {
      // The UI is already settled; backend cancellation is best-effort here.
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focusInput]);

  return {
    doAiQuery,
    enqueueAiQuery,
    handleCancelAiRequest,
  };
}
