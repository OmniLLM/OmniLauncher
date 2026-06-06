export function toolIcon(tool: string): string {
  if (tool.startsWith("🎯")) return "";
  if (tool.includes("file")) return "📁";
  if (tool.includes("web") || tool.includes("search")) return "🌐";
  if (tool.includes("calc")) return "🧮";
  if (tool.includes("shell") || tool.includes("exec")) return "🔧";
  if (tool.includes("app")) return "🚀";
  if (tool.includes("clip")) return "📋";
  return "🔧";
}
