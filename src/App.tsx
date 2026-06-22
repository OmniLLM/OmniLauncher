import { useEffect, useRef, useState } from "react";
import { isAiPrefix } from "./launcherConfig";
import CheatSheetModal from "./components/CheatSheetModal";
import ExportToast from "./components/ExportToast";
import GlobalKeyframes from "./components/GlobalKeyframes";
import AppShell from "./components/AppShell";
import LauncherBody from "./components/LauncherBody";
import type { AppSettings, ConversationTurn } from "./types/app";
import { useTheme } from "./hooks/useTheme";
import { useFavorites } from "./hooks/useFavorites";
import { useAiSessions } from "./hooks/useAiSessions";
import { useLayoutGeometry } from "./hooks/useLayoutGeometry";
import { useFocus } from "./hooks/useFocus";
import { useInputHistory } from "./hooks/useInputHistory";
import { useSearch } from "./hooks/useSearch";
import { useAiQuery } from "./hooks/useAiQuery";
import { useGlobalKeyboard } from "./hooks/useGlobalKeyboard";
import { useSubmitAndExecute } from "./hooks/useSubmitAndExecute";
import { useAppBootstrap } from "./hooks/useAppBootstrap";

// Theme colors are defined in styles/index.css via [data-theme] attributes; components
// read them via var(--bg), var(--accent), etc.

export default function App() {
  const [loading, setLoading] = useState(false);
  const [aiModeEnabled, setAiModeEnabled] = useState(false);
  const [showPluginManager, setShowPluginManager] = useState(false);
  const [showSkillManager, setShowSkillManager] = useState(false);
  const [backgroundUrl, setBackgroundUrl] = useState<string>("");
  const [conversationHistory, setConversationHistory] = useState<
    ConversationTurn[]
  >([]);
  const [, setSettings] = useState<AppSettings | null>(null);
  const [queueDepth, setQueueDepth] = useState(0);
  const [queuedPrompts, setQueuedPrompts] = useState<string[]>([]);
  const [showCheatSheet, setShowCheatSheet] = useState(false);
  const [exportToast, setExportToast] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

  const chatScrollRef = useRef<HTMLDivElement>(null);
  const pendingQueueRef = useRef<string[]>([]);
  const cancelRequestedRef = useRef(false);
  const aiCleanupRef = useRef<(() => void) | null>(null);

  const { setTheme, resolvedTheme, handleThemeToggle } = useTheme();
  const {
    inputHistory,
    setInputHistory,
    historyIdx,
    setHistoryIdx,
    inputHistoryRef,
  } = useInputHistory();
  const {
    query,
    setQuery,
    results,
    setResults,
    searching,
    searchError,
    debounceRef,
    handleQueryChange,
  } = useSearch({ aiModeEnabled, setAiModeEnabled, setHistoryIdx });
  const { favoriteItems, favorites, handleToggleFavorite } = useFavorites();
  const {
    sessions,
    currentSessionId,
    showSessionPicker,
    setShowSessionPicker,
    refreshSessions,
    handleNewConversation,
    handleSwitchSession,
    handleDeleteSession,
  } = useAiSessions({
    setConversationHistory,
    pendingQueueRef,
    setQueuedPrompts,
    setQueueDepth,
    setResults,
    setQuery,
  });
  const { inputRef, focusInput } = useFocus({
    setQuery,
    showPluginManager,
    showSkillManager,
  });

  const isAiMode = aiModeEnabled || isAiPrefix(query);

  useAppBootstrap({ setSettings, setBackgroundUrl, setTheme, setShowSettings });

  const { doAiQuery, enqueueAiQuery, handleCancelAiRequest } = useAiQuery({
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
  });

  const { handleSubmit, handleExecute } = useSubmitAndExecute({
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
  });

  // Auto-scroll chat to bottom whenever new turns or queued prompts arrive.
  useEffect(() => {
    if (isAiMode && chatScrollRef.current) {
      chatScrollRef.current.scrollTop = chatScrollRef.current.scrollHeight;
    }
  }, [conversationHistory, queuedPrompts, isAiMode]);

  const {
    isCompactMode,
    launcherResultsMode,
    resetWindowSize,
    handleResizeStart,
  } = useLayoutGeometry({
    isAiMode,
    results,
    showPluginManager,
    showSkillManager,
    showSettings,
  });

  useGlobalKeyboard({
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
  });

  return (
    <>
      <GlobalKeyframes />
      <AppShell
        resolvedTheme={resolvedTheme}
        backgroundUrl={backgroundUrl}
        isCompactMode={isCompactMode}
        isAiMode={isAiMode}
      >
        <LauncherBody
          ref={chatScrollRef}
          isAiMode={isAiMode}
          isCompactMode={isCompactMode}
          launcherResultsMode={launcherResultsMode}
          showSettings={showSettings}
          setShowSettings={setShowSettings}
          showPluginManager={showPluginManager}
          setShowPluginManager={setShowPluginManager}
          showSkillManager={showSkillManager}
          setShowSkillManager={setShowSkillManager}
          sessions={sessions}
          currentSessionId={currentSessionId}
          showSessionPicker={showSessionPicker}
          setShowSessionPicker={setShowSessionPicker}
          handleNewConversation={handleNewConversation}
          handleSwitchSession={handleSwitchSession}
          handleDeleteSession={handleDeleteSession}
          conversationHistory={conversationHistory}
          queuedPrompts={queuedPrompts}
          query={query}
          setQuery={setQuery}
          results={results}
          searching={searching}
          searchError={searchError}
          favorites={favorites}
          favoriteItems={favoriteItems}
          handleToggleFavorite={handleToggleFavorite}
          handleExecute={handleExecute}
          handleQueryChange={handleQueryChange}
          handleSubmit={handleSubmit}
          loading={loading}
          queueDepth={queueDepth}
          handleCancelAiRequest={handleCancelAiRequest}
          resolvedTheme={resolvedTheme}
          handleThemeToggle={handleThemeToggle}
          inputRef={inputRef}
          inputHistory={inputHistory}
          historyIdx={historyIdx}
          setHistoryIdx={setHistoryIdx}
          handleResizeStart={handleResizeStart}
          resetWindowSize={resetWindowSize}
        />
      </AppShell>
      <ExportToast open={exportToast} />
      <CheatSheetModal
        open={showCheatSheet}
        onClose={() => setShowCheatSheet(false)}
      />
    </>
  );
}

