import { useCallback, useEffect, useRef } from "react";
import { listen, getCurrentWebviewWindow } from "../lib/runtime";

export interface UseFocusArgs {
  setQuery: React.Dispatch<React.SetStateAction<string>>;
  showPluginManager: boolean;
  showSkillManager: boolean;
}

export interface UseFocusResult {
  inputRef: React.MutableRefObject<HTMLInputElement | HTMLTextAreaElement | null>;
  focusInput: (select?: boolean) => void;
}

export function useFocus(args: UseFocusArgs): UseFocusResult {
  const { setQuery, showPluginManager, showSkillManager } = args;

  const inputRef = useRef<HTMLInputElement | HTMLTextAreaElement | null>(null);

  const focusInput = useCallback((select = false) => {
    inputRef.current?.focus();
    if (select) inputRef.current?.select();
  }, []);

  // Focus input on mount
  useEffect(() => {
    focusInput();
  }, [focusInput]);

  // Re-focus input when the native window is shown or focused
  useEffect(() => {
    const focusVisibleInput = () => {
      focusInput();
      setTimeout(() => focusInput(true), 50);
      setTimeout(() => focusInput(), 150);
    };

    let unlistenFocus: (() => void) | undefined;
    let unlistenShown: (() => void) | undefined;

    listen<string>("omnilauncher://shown", (event) => {
      const selection = event.payload ?? "";
      if (selection.trim()) {
        // Auto-populate with selected text from the previously focused app.
        // The selection plugin will detect the "__sel__:" prefix and show actions.
        setQuery("__sel__:" + selection.trim());
        setTimeout(() => focusInput(true), 50);
      } else {
        focusVisibleInput();
      }
    })
      .then((fn) => {
        unlistenShown = fn;
      })
      .catch(() => {});

    getCurrentWebviewWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused) focusVisibleInput();
      })
      .then((fn) => {
        unlistenFocus = fn;
      })
      .catch(() => {});

    return () => {
      unlistenFocus?.();
      unlistenShown?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focusInput]);

  // Browser-level fallback for focus restore (useful in dev/web context)
  useEffect(() => {
    const shouldFocusLauncherInput = () =>
      !showPluginManager && !showSkillManager;

    const restoreFocus = () => {
      if (!shouldFocusLauncherInput()) return;
      setTimeout(() => focusInput(), 0);
    };

    const onVisibilityChange = () => {
      if (document.visibilityState === "visible") {
        restoreFocus();
      }
    };

    window.addEventListener("focus", restoreFocus);
    document.addEventListener("visibilitychange", onVisibilityChange);

    return () => {
      window.removeEventListener("focus", restoreFocus);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focusInput, showPluginManager]);

  return { inputRef, focusInput };
}
