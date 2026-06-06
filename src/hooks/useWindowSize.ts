import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, getCurrentWebviewWindow } from "../lib/runtime";

export interface UseWindowSizeArgs {
  panelHeight: number;
  isAiMode: boolean;
  isPanelMode: boolean;
  showSettings: boolean;
}

export interface UseWindowSizeResult {
  userResized: boolean;
  userResizedRef: React.MutableRefObject<boolean>;
  resetWindowSize: () => void;
  handleResizeStart: (e: React.PointerEvent<HTMLDivElement>) => void;
}

export function useWindowSize(args: UseWindowSizeArgs): UseWindowSizeResult {
  const { panelHeight, isAiMode, isPanelMode, showSettings } = args;

  // Tracks whether the user has manually resized the window via the corner
  // grip. While true we stop auto-fitting the window to its content height so
  // the manual size sticks (remembered across hide/show via Ctrl+Shift+O).
  const userResizedRef = useRef(false);
  const [userResized, setUserResized] = useState(false);

  // ── Layout geometry ────────────────────────────────────────────────────────
  useEffect(() => {
    let saveTimer: ReturnType<typeof setTimeout>;
    const unlisten = getCurrentWebviewWindow().onMoved(({ payload }) => {
      clearTimeout(saveTimer);
      saveTimer = setTimeout(() => {
        invoke("save_window_position", { x: payload.x, y: payload.y }).catch(
          () => {},
        );
      }, 500);
    });
    return () => {
      clearTimeout(saveTimer);
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    // Skip auto-fitting while the user is manually controlling the window size.
    if (userResizedRef.current) return;
    invoke("set_window_geometry", {
      height: showSettings ? 560 : panelHeight,
      aiMode: isAiMode && !showSettings,
      panelMode: isPanelMode && !showSettings,
    }).catch(() => {});
  }, [panelHeight, isAiMode, isPanelMode, showSettings, userResized]);

  // Corner resize grip: drag the bottom-right corner to resize the window while
  // keeping it centered on screen. Width/height grow at 2× the cursor delta so
  // the grabbed corner tracks the pointer exactly under centered layout.
  const resetWindowSize = useCallback(() => {
    if (!userResizedRef.current) return;
    // Clearing the manual-resize flag re-enables the content auto-fit effect,
    // which immediately re-applies the initial centered geometry and relayout.
    userResizedRef.current = false;
    setUserResized(false);
  }, []);

  const handleResizeStart = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      e.preventDefault();
      e.stopPropagation();
      userResizedRef.current = true;
      setUserResized(true);
      const startW = window.innerWidth;
      const startH = window.innerHeight;
      const startX = e.screenX;
      const startY = e.screenY;
      const minW = 360;
      const minH = 120;
      const maxW = window.screen.width;
      const maxH = window.screen.height;

      let raf = 0;
      let pending: { w: number; h: number } | null = null;
      const flush = () => {
        raf = 0;
        if (pending) {
          invoke("set_window_size_centered", {
            width: pending.w,
            height: pending.h,
          }).catch(() => {});
          pending = null;
        }
      };
      const onMove = (ev: PointerEvent) => {
        const w = Math.max(
          minW,
          Math.min(maxW, startW + 2 * (ev.screenX - startX)),
        );
        const h = Math.max(
          minH,
          Math.min(maxH, startH + 2 * (ev.screenY - startY)),
        );
        pending = { w, h };
        if (!raf) raf = requestAnimationFrame(flush);
      };
      const onUp = () => {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
        if (raf) {
          cancelAnimationFrame(raf);
          flush();
        }
      };
      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
    },
    [],
  );

  return { userResized, userResizedRef, resetWindowSize, handleResizeStart };
}
