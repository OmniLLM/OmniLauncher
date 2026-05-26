// Browser-safe wrapper for @tauri-apps/api/core
// Provides mock implementations when running outside Tauri

const isTauri = () => !!(window as any).__TAURI__;

export async function invoke<T = unknown>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri()) {
    console.warn(`[Tauri Shim] Mock invoke for: ${cmd}`);
    // Return mock data for dev mode
    if (cmd === "search") {
      return [
        {
          id: "1",
          title: "Calculator",
          subtitle: "App",
          score: 1,
          action_type: "open",
          action_data: "calc",
          icon: "🧮",
        },
        {
          id: "2",
          title: "Notepad",
          subtitle: "App",
          score: 0.8,
          action_type: "open",
          action_data: "notepad",
          icon: "📝",
        },
      ] as T;
    }
    if (cmd === "ai_query") {
      return {
        content: `AI response for: "${(args as any)?.query}" (mock - run in Tauri for real AI)`,
        tools_used: [],
        results: [],
        is_ai: true,
      } as T;
    }
    if (cmd === "get_settings") {
      return {
        aiBaseUrl: "",
        aiApiKey: "",
        aiModel: "gpt-4",
        theme: "system",
      } as T;
    }
    return {} as T;
  }
  const { invoke: realInvoke } = await import("@tauri-apps/api/core");
  return realInvoke(cmd, args);
}

export * from "@tauri-apps/api/core";
