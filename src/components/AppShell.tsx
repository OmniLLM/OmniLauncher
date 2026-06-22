import type { ResolvedTheme } from "../utils/theme";

const SHELL_FONT =
  "'Aptos Display', 'Segoe UI Variable Display', 'Segoe UI', system-ui, sans-serif";

export interface AppShellProps {
  resolvedTheme: ResolvedTheme;
  backgroundUrl: string;
  isCompactMode: boolean;
  isAiMode: boolean;
  children: React.ReactNode;
}

/**
 * Outer wrapper div for the launcher window: sets the background gradient,
 * theme-aware colors, font, and height/transition behavior. Always fills the
 * entire viewport — native window resizing is handled by useWindowSize hook.
 */
export default function AppShell({
  resolvedTheme,
  backgroundUrl,
  isCompactMode,
  isAiMode,
  children,
}: AppShellProps) {
  return (
    <div
      style={{
        width: "100%",
        minHeight: "100vh",
        height: "100vh",
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
        outline: isAiMode
          ? `1.5px solid color-mix(in srgb, var(--accent) 20%, transparent)`
          : "none",
      }}
    >
      {children}
    </div>
  );
}
