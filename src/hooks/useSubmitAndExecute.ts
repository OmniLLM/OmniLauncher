import { useCallback } from "react";
import {
  isAiPrefix,
  isConversationResetCommand,
  isHelpQuery,
  isSlashPrefix,
  slashSuggestions,
  helpResults,
} from "../launcherConfig";
import { invoke } from "../lib/runtime";
import type { ConversationTurn, QueryResult } from "../types/app";

export interface UseSubmitAndExecuteArgs {
  aiModeEnabled: boolean;
  setAiModeEnabled: React.Dispatch<React.SetStateAction<boolean>>;
  setQuery: React.Dispatch<React.SetStateAction<string>>;
  setResults: React.Dispatch<React.SetStateAction<QueryResult[]>>;
  setShowPluginManager: React.Dispatch<React.SetStateAction<boolean>>;
  setShowSkillManager: React.Dispatch<React.SetStateAction<boolean>>;
  setLoading: React.Dispatch<React.SetStateAction<boolean>>;
  setConversationHistory: React.Dispatch<
    React.SetStateAction<ConversationTurn[]>
  >;
  debounceRef: React.MutableRefObject<ReturnType<typeof setTimeout> | null>;
  inputHistoryRef: React.MutableRefObject<string[]>;
  setInputHistory: React.Dispatch<React.SetStateAction<string[]>>;
  setHistoryIdx: React.Dispatch<React.SetStateAction<number>>;
  loading: boolean;
  enqueueAiQuery: (value: string) => void;
  doAiQuery: (q: string) => Promise<void>;
  handleNewConversation: () => Promise<void>;
  focusInput: (select?: boolean) => void;
}

export interface UseSubmitAndExecuteResult {
  handleSubmit: (value: string, forceAi: boolean) => Promise<void>;
  handleExecute: (result: QueryResult) => Promise<void>;
}

export function useSubmitAndExecute(
  args: UseSubmitAndExecuteArgs,
): UseSubmitAndExecuteResult {
  const {
    aiModeEnabled,
    setAiModeEnabled,
    setQuery,
    setResults,
    setShowPluginManager,
    setShowSkillManager,
    setLoading,
    setConversationHistory,
    debounceRef,
    inputHistoryRef,
    setInputHistory,
    setHistoryIdx,
    loading,
    enqueueAiQuery,
    doAiQuery,
    handleNewConversation,
    focusInput,
  } = args;

  const handleSubmit = useCallback(
    async (value: string, forceAi: boolean) => {
      if (isConversationResetCommand(value)) {
        handleNewConversation();
        return;
      }

      if (isHelpQuery(value)) {
        setResults(helpResults());
        return;
      }

      if (value.trim() === "?") {
        setAiModeEnabled((prev) => !prev);
        setQuery("");
        setResults([]);
        return;
      }

      if (isSlashPrefix(value)) {
        setResults(slashSuggestions(value));
        return;
      }

      const slashCommand = value.trim().toLowerCase();

      if (slashCommand === "/plugins" || slashCommand === "/pm") {
        setShowPluginManager(true);
        setResults([]);
        setQuery("");
        return;
      }

      if (slashCommand === "/skills") {
        setShowSkillManager(true);
        setResults([]);
        setQuery("");
        return;
      }

      if (forceAi || isAiPrefix(value)) {
        if (debounceRef.current) clearTimeout(debounceRef.current);
        setAiModeEnabled(true);
        if (value.trim() && inputHistoryRef.current[0] !== value.trim()) {
          inputHistoryRef.current = [
            value.trim(),
            ...inputHistoryRef.current,
          ].slice(0, 50);
          setInputHistory([...inputHistoryRef.current]);
        }
        setHistoryIdx(-1);
        if (loading) {
          enqueueAiQuery(value);
        } else {
          doAiQuery(value);
        }
        setQuery("");
      } else if (aiModeEnabled && value.trim()) {
        if (debounceRef.current) clearTimeout(debounceRef.current);
        if (value.trim() && inputHistoryRef.current[0] !== value.trim()) {
          inputHistoryRef.current = [
            value.trim(),
            ...inputHistoryRef.current,
          ].slice(0, 50);
          setInputHistory([...inputHistoryRef.current]);
        }
        setHistoryIdx(-1);
        if (loading) {
          enqueueAiQuery(value);
        } else {
          doAiQuery(value);
        }
        setQuery("");
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [aiModeEnabled, doAiQuery, enqueueAiQuery, handleNewConversation, loading],
  );

  const handleExecute = useCallback(
    async (result: QueryResult) => {
      if (result.action_type === "open_plugin_manager") {
        setShowPluginManager(true);
        setResults([]);
        setQuery("");
        return;
      }

      if (result.action_type === "help_command") {
        setQuery(result.action_data);
        setResults([]);
        setTimeout(() => focusInput(), 50);
        return;
      }

      if (result.action_type === "slash_complete") {
        setQuery(result.action_data);
        setResults([]);
        setTimeout(() => focusInput(), 50);
        return;
      }

      if (result.action_type === "copy") {
        try {
          await navigator.clipboard.writeText(result.action_data);
        } catch {
          console.log("Copy:", result.action_data);
        }
        return;
      }

      if (result.action_type === "vision_analyze") {
        const prompt = result.action_data;
        const userLabel = prompt.trim() || "Describe what you see";
        const userTurn: ConversationTurn = {
          role: "user",
          content: `👁 Vision: ${userLabel}`,
        };
        const pendingAiTurn: ConversationTurn = {
          role: "assistant",
          content: "",
          tools_used: [],
          isStreaming: true,
        };
        setAiModeEnabled(true);
        setConversationHistory((prev) => [...prev, userTurn, pendingAiTurn]);
        setLoading(true);
        setResults([]);
        setQuery("");
        try {
          // Capture the screenshot locally (only the shell has a screen), then
          // send it to the backend for the AI vision call.
          const imageBase64 = await invoke<string>(
            "capture_vision_screenshot",
          );
          const response = await invoke<string>("vision_analyze", {
            prompt,
            imageBase64,
          });
          setConversationHistory((prev) => {
            const next = [...prev];
            next[next.length - 1] = {
              role: "assistant",
              content: response,
              tools_used: ["vision"],
              isStreaming: false,
            };
            return next;
          });
        } catch (e) {
          setConversationHistory((prev) => {
            const next = [...prev];
            next[next.length - 1] = {
              role: "assistant",
              content: `Vision analysis failed: ${e}`,
              isStreaming: false,
            };
            return next;
          });
        } finally {
          setLoading(false);
          setTimeout(() => focusInput(), 150);
        }
        return;
      }

      try {
        await invoke("execute_result", { result });
      } catch (e) {
        console.error("Execute error:", e);
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [focusInput],
  );

  return { handleSubmit, handleExecute };
}
