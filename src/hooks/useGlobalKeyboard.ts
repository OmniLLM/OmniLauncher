import { useEffect } from "react";
import { getCurrentWebviewWindow } from "../lib/runtime";
import type { ConversationTurn, QueryResult } from "../types/app";

export interface UseGlobalKeyboardArgs {
  query: string;
  setQuery: React.Dispatch<React.SetStateAction<string>>;
  setResults: React.Dispatch<React.SetStateAction<QueryResult[]>>;
  showPluginManager: boolean;
  setShowPluginManager: React.Dispatch<React.SetStateAction<boolean>>;
  showSkillManager: boolean;
  setShowSkillManager: React.Dispatch<React.SetStateAction<boolean>>;
  showCheatSheet: boolean;
  setShowCheatSheet: React.Dispatch<React.SetStateAction<boolean>>;
  setShowSettings: React.Dispatch<React.SetStateAction<boolean>>;
  isAiMode: boolean;
  setAiModeEnabled: React.Dispatch<React.SetStateAction<boolean>>;
  conversationHistory: ConversationTurn[];
  setExportToast: React.Dispatch<React.SetStateAction<boolean>>;
  loading: boolean;
  handleCancelAiRequest: () => void;
  focusInput: (select?: boolean) => void;
  resetWindowSize: () => void;
}

export function useGlobalKeyboard(args: UseGlobalKeyboardArgs): void {
  const {
    query,
    setQuery,
    setResults,
    showPluginManager,
    setShowPluginManager,
    showSkillManager,
    setShowSkillManager,
    showCheatSheet,
    setShowCheatSheet,
    setShowSettings,
    isAiMode,
    setAiModeEnabled,
    conversationHistory,
    setExportToast,
    loading,
    handleCancelAiRequest,
    focusInput,
    resetWindowSize,
  } = args;

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const active = document.activeElement as HTMLElement | null;
      const isEditableTarget =
        !!active &&
        (active.isContentEditable ||
          active.tagName === "INPUT" ||
          active.tagName === "TEXTAREA" ||
          active.tagName === "SELECT");

      if (e.key === "Escape") {
        if (showCheatSheet) {
          setShowCheatSheet(false);
          return;
        }
        if (loading && isAiMode) {
          e.preventDefault();
          handleCancelAiRequest();
          return;
        }
        if (query === "" && !showPluginManager && !showSkillManager) {
          // Already clean — hide the window
          getCurrentWebviewWindow()
            .hide()
            .catch(() => {});
        } else {
          setQuery("");
          setResults([]);
          setShowPluginManager(false);
          setShowSkillManager(false);
        }
      }

      if (
        e.key === "?" &&
        !e.ctrlKey &&
        !e.metaKey &&
        !e.altKey &&
        !e.repeat &&
        !isEditableTarget
      ) {
        e.preventDefault();
        setAiModeEnabled((prev) => !prev);
        setQuery("");
        setResults([]);
        setTimeout(() => focusInput(), 50);
      }

      if (e.key === "," && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setShowSettings((prev) => !prev);
      }

      if (e.key === "F1") {
        e.preventDefault();
        setShowCheatSheet((prev) => !prev);
      }

      if (
        e.key === "s" &&
        (e.metaKey || e.ctrlKey) &&
        isAiMode &&
        conversationHistory.length > 0
      ) {
        e.preventDefault();
        const md = conversationHistory
          .map((turn) => {
            const role = turn.role === "user" ? "**You**" : "**AI**";
            const tools = turn.tools_used?.length
              ? `\n> Tools: ${turn.tools_used.join(", ")}\n`
              : "";
            return `${role}\n${tools}${turn.content}`;
          })
          .join("\n\n---\n\n");
        navigator.clipboard.writeText(md).catch(() => {});
        setExportToast(true);
        setTimeout(() => setExportToast(false), 2000);
      }

      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setAiModeEnabled((prev) => !prev);
        setQuery("");
        setResults([]);
        setTimeout(() => focusInput(), 50);
      }

      // Reset window back to its initial auto-fitted size.
      if (e.key === "0" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        resetWindowSize();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    focusInput,
    query,
    showPluginManager,
    showSkillManager,
    showCheatSheet,
    isAiMode,
    conversationHistory,
    loading,
    handleCancelAiRequest,
  ]);
}
