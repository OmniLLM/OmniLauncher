import { lazy, Suspense, forwardRef } from "react";
import { isSlashPrefix } from "../launcherConfig";
import SearchBar from "./SearchBar";
import ResultList from "./ResultList";
import AiTopBar from "./AiTopBar";
import AiChatHistory from "./AiChatHistory";
import LauncherResults from "./LauncherResults";
import ResizeGrip from "./ResizeGrip";
import type {
  AiSessionInfo,
  ConversationTurn,
  QueryResult,
} from "../types/app";
import type { ResolvedTheme } from "../utils/theme";

// Code-split heavy on-demand panels so they don't bloat the initial launcher bundle.
const SettingsWindow = lazy(() => import("./SettingsWindow"));
const PluginManager = lazy(() => import("./PluginManager"));
const SkillManager = lazy(() => import("./SkillManager"));

export interface LauncherBodyProps {
  // Mode flags
  isAiMode: boolean;
  isCompactMode: boolean;
  launcherResultsMode: boolean;
  // Panel toggles
  showSettings: boolean;
  setShowSettings: React.Dispatch<React.SetStateAction<boolean>>;
  showPluginManager: boolean;
  setShowPluginManager: React.Dispatch<React.SetStateAction<boolean>>;
  showSkillManager: boolean;
  setShowSkillManager: React.Dispatch<React.SetStateAction<boolean>>;
  // AI session picker
  sessions: AiSessionInfo[];
  currentSessionId: number | null;
  showSessionPicker: boolean;
  setShowSessionPicker: React.Dispatch<React.SetStateAction<boolean>>;
  handleNewConversation: () => void;
  handleSwitchSession: (id: number) => void;
  handleDeleteSession: (id: number) => void;
  // AI conversation
  conversationHistory: ConversationTurn[];
  queuedPrompts: string[];
  // Launcher results
  query: string;
  setQuery: React.Dispatch<React.SetStateAction<string>>;
  results: QueryResult[];
  searching: boolean;
  searchError: string | null;
  favorites: Set<string>;
  favoriteItems: QueryResult[];
  handleToggleFavorite: (item: QueryResult) => void;
  handleExecute: (item: QueryResult) => void;
  // Search bar
  handleQueryChange: (value: string) => void;
  handleSubmit: (value: string, forceAi: boolean) => void;
  loading: boolean;
  queueDepth: number;
  handleCancelAiRequest: () => void;
  resolvedTheme: ResolvedTheme;
  handleThemeToggle: () => void;
  inputRef: React.MutableRefObject<HTMLInputElement | HTMLTextAreaElement | null>;
  inputHistory: string[];
  historyIdx: number;
  setHistoryIdx: React.Dispatch<React.SetStateAction<number>>;
  // Resize
  handleResizeStart: (e: React.PointerEvent<HTMLDivElement>) => void;
  resetWindowSize: () => void;
}

/**
 * The interior of AppShell — every panel and overlay that sits between the
 * background gradient and the resize grip. Pure presentational composition;
 * App.tsx owns all the state and handlers.
 */
const LauncherBody = forwardRef<HTMLDivElement, LauncherBodyProps>(
  function LauncherBody(props, chatScrollRef) {
    const {
      isAiMode,
      isCompactMode,
      launcherResultsMode,
      showSettings,
      setShowSettings,
      showPluginManager,
      setShowPluginManager,
      showSkillManager,
      setShowSkillManager,
      sessions,
      currentSessionId,
      showSessionPicker,
      setShowSessionPicker,
      handleNewConversation,
      handleSwitchSession,
      handleDeleteSession,
      conversationHistory,
      queuedPrompts,
      query,
      setQuery,
      results,
      searching,
      searchError,
      favorites,
      favoriteItems,
      handleToggleFavorite,
      handleExecute,
      handleQueryChange,
      handleSubmit,
      loading,
      queueDepth,
      handleCancelAiRequest,
      resolvedTheme,
      handleThemeToggle,
      inputRef,
      inputHistory,
      historyIdx,
      setHistoryIdx,
      handleResizeStart,
      resetWindowSize,
    } = props;

    if (showSettings) {
      return (
        <Suspense fallback={null}>
          <SettingsWindow onClose={() => setShowSettings(false)} />
        </Suspense>
      );
    }

    return (
      <>
        {/* ── AI MODE: top bar ─────────────────────────────────────────── */}
        {isAiMode && (
          <AiTopBar
            sessions={sessions}
            currentSessionId={currentSessionId}
            showSessionPicker={showSessionPicker}
            setShowSessionPicker={setShowSessionPicker}
            handleNewConversation={handleNewConversation}
            handleSwitchSession={handleSwitchSession}
            handleDeleteSession={handleDeleteSession}
          />
        )}

        {/* ── AI MODE: scrollable chat history ─────────────────────────── */}
        {isAiMode && !showSkillManager && (
          <AiChatHistory
            ref={chatScrollRef}
            conversationHistory={conversationHistory}
            queuedPrompts={queuedPrompts}
          />
        )}

        {/* ── PLUGIN MANAGER panel ─────────────────────────────────────── */}
        {showPluginManager && !isAiMode && (
          <Suspense fallback={null}>
            <PluginManager onClose={() => setShowPluginManager(false)} />
          </Suspense>
        )}

        {/* ── SKILL MANAGER panel ──────────────────────────────────────── */}
        {showSkillManager && (
          <Suspense fallback={null}>
            <SkillManager onClose={() => setShowSkillManager(false)} />
          </Suspense>
        )}

        {/* ── LAUNCHER MODE: results list ───────────────────────────────── */}
        {!isAiMode && !showPluginManager && !showSkillManager && (
          <LauncherResults
            query={query}
            results={results}
            searching={searching}
            searchError={searchError}
            favorites={favorites}
            favoriteItems={favoriteItems}
            handleToggleFavorite={handleToggleFavorite}
            handleExecute={handleExecute}
          />
        )}

        {/* ── AI MODE: slash command suggestions overlay ────────────────── */}
        {isAiMode && results.length > 0 && isSlashPrefix(query) && (
          <ResultList
            results={results}
            query={query}
            onExecute={handleExecute}
            favorites={favorites}
            onToggleFavorite={handleToggleFavorite}
          />
        )}

        <div
          style={{
            flexShrink: 0,
            paddingBottom: "2px",
            paddingTop: launcherResultsMode ? "10px" : undefined,
            paddingLeft: launcherResultsMode ? "12px" : undefined,
            paddingRight: launcherResultsMode ? "12px" : undefined,
            order: launcherResultsMode ? -1 : undefined,
            transform: isCompactMode ? "translateY(-18px)" : undefined,
          }}
        >
          {/* ── Search / input bar (always at bottom) ────────────────────── */}
          <SearchBar
            value={query}
            onChange={handleQueryChange}
            onSubmit={handleSubmit}
            isAiMode={isAiMode}
            loading={loading}
            queueDepth={queueDepth}
            onCancel={handleCancelAiRequest}
            onSettingsClick={() => setShowSettings(true)}
            resolvedTheme={resolvedTheme}
            onThemeToggle={handleThemeToggle}
            compact={isCompactMode}
            inputRef={inputRef}
            inputHistory={inputHistory}
            historyIdx={historyIdx}
            onHistoryNavigate={(idx, val) => {
              setHistoryIdx(idx);
              setQuery(val);
            }}
          />
        </div>

        {/* ── Bottom-right corner resize grip (keeps window centered) ──── */}
        <ResizeGrip
          onPointerDown={handleResizeStart}
          onDoubleClick={resetWindowSize}
        />
      </>
    );
  },
);

export default LauncherBody;
