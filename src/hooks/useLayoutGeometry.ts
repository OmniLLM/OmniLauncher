import { useWindowSize } from "./useWindowSize";
import type { QueryResult } from "../types/app";

export interface UseLayoutGeometryArgs {
  isAiMode: boolean;
  results: QueryResult[];
  showPluginManager: boolean;
  showSkillManager: boolean;
  showSettings: boolean;
}

export interface UseLayoutGeometryResult {
  launcherHasContent: boolean;
  isCompactMode: boolean;
  launcherResultsMode: boolean;
  isPanelMode: boolean;
  panelHeight: number;
  windowHeight: string;
  maxHeight: string;
  userResized: boolean;
  resetWindowSize: () => void;
  handleResizeStart: (e: React.PointerEvent<HTMLDivElement>) => void;
}

/**
 * Derives layout-related values (compact/panel modes, heights, transitions)
 * and wires them into the corner-grip resize behavior. App.tsx just consumes
 * the returned shape; everything geometric stays in this hook.
 */
export function useLayoutGeometry(
  args: UseLayoutGeometryArgs,
): UseLayoutGeometryResult {
  const { isAiMode, results, showPluginManager, showSkillManager, showSettings } = args;

  const launcherHasContent =
    results.length > 0 || showPluginManager || showSkillManager;
  const isCompactMode = !isAiMode && !launcherHasContent;
  // In launcher (non-AI) mode, once there are results to show we lift the
  // search bar to the top and render the answer in a card below it.
  const launcherResultsMode =
    !isAiMode && !showPluginManager && !showSkillManager && results.length > 0;
  const isPanelMode = showPluginManager || showSkillManager;
  const screenHeight =
    typeof window !== "undefined" ? window.screen.height : 1080;
  const compactHeight = Math.max(320, Math.round(screenHeight * 0.3));
  const aiHeight = Math.max(560, Math.round(screenHeight * 0.5));
  const panelHeight = isPanelMode
    ? Math.round(screenHeight * 0.4)
    : isAiMode
      ? aiHeight
      : launcherHasContent
        ? 520
        : compactHeight;
  const effectiveHeight = showSettings ? 560 : panelHeight;
  const windowHeight = `${effectiveHeight}px`;
  const maxHeight = `${effectiveHeight}px`;

  const { userResized, resetWindowSize, handleResizeStart } = useWindowSize({
    panelHeight,
    isAiMode,
    isPanelMode,
    showSettings,
  });

  return {
    launcherHasContent,
    isCompactMode,
    launcherResultsMode,
    isPanelMode,
    panelHeight,
    windowHeight,
    maxHeight,
    userResized,
    resetWindowSize,
    handleResizeStart,
  };
}
