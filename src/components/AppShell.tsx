import type { ResolvedTheme } from "../utils/theme";

const SHELL_FONT =
  "'Aptos Display', 'Segoe UI Variable Display', 'Segoe UI', system-ui, sans-serif";

export interface AppShellProps {
  resolvedTheme: ResolvedTheme;
  backgroundUrl: string;
  windowHeight: string;
  maxHeight: string;
  userResized: boolean;
  isCompactMode: boolean;
  isAiMode: boolean;
  children: React.ReactNode;
}

/**
 * Outer wrapper div for the launcher window: sets the background gradient,
 * theme-aware colors, font, and height/transition behavior. Stateless — all
 * geometry inputs come from props.
 */
export default function AppShell({
  resolvedTheme,
  backgroundUrl,
  windowHeight,
  maxHeight,
  userResized,
  isCompactMode,
  isAiMode,
  children,
}: AppShellProps) {
  return (
    <div
      style={{
        width: "100%",
        height: userResized ? "100vh" : windowHeight,
        maxHeight: userResized ? "100vh" : maxHeight,
        background:
          resolvedTheme === "dark"
            ? backgroundUrl
              ? `
                linear-gradient(180deg, rgba(6, 12, 24, 0.74) 0%, rgba(8, 14, 28, 0.86) 100%),
                radial-gradient(circle at 18% -6%, color-mix(in srgb, var(--accent) 12%, transparent) 0, transparent 40%),
                url("${backgroundUrl}") center top / cover no-repeat
              `
              : `linear-gradient(160deg, #0b1220 0%, #0e1930 52%, #0a1426 100%)`
            : "var(--bg)",
        color: "var(--text)",
        fontFamily: SHELL_FONT,
        borderRadius: "0",
        overflow: "hidden",
        boxShadow: "none",
        display: "flex",
        flexDirection: "column",
        justifyContent: isCompactMode ? "center" : "flex-start",
        padding: isCompactMode ? "0" : 0,
        boxSizing: "border-box",
        transition:
          "height 220ms cubic-bezier(0.4,0,0.2,1), max-height 220ms cubic-bezier(0.4,0,0.2,1)",
        outline: isAiMode
          ? `1.5px solid color-mix(in srgb, var(--accent) 20%, transparent)`
          : "none",
      }}
    >
      {children}
    </div>
  );
}
