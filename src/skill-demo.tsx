import { createRoot } from "react-dom/client";
import SkillManager from "./components/SkillManager";

// Mock Tauri invoke before anything else
(window as any).__TAURI_INTERNALS__ = {
  invoke: async (cmd: string) => {
    if (cmd === "list_skills") {
      return [
        {
          name: "web-summarizer",
          description: "Summarizes web pages and articles concisely.",
          version: "1.2.0",
          triggers: ["summarize", "summary", "tldr"],
          tags: ["web", "reading", "ai"],
          tools_hint: ["fetch", "summarize"],
          path: "~/.omnilauncher/skills/web-summarizer/SKILL.md",
        },
        {
          name: "code-reviewer",
          description: "Reviews code for bugs, style, and best practices.",
          version: "0.9.1",
          triggers: ["review", "code review", "check code"],
          tags: ["code", "dev"],
          tools_hint: ["read_file", "grep"],
          path: "~/.omnilauncher/skills/code-reviewer/SKILL.md",
        },
        {
          name: "brainstorm",
          description: "Helps brainstorm ideas creatively.",
          version: "1.0.0",
          triggers: ["brainstorm", "ideas"],
          tags: ["creativity"],
          tools_hint: [],
          path: "~/.omnilauncher/skills/brainstorm/SKILL.md",
        },
      ];
    }
    if (cmd === "install_skill") return "Skill installed successfully.";
    if (cmd === "update_skill") return "Skill updated.";
    if (cmd === "delete_skill") return "Skill deleted.";
    if (cmd === "reload_skills") return true;
    return null;
  },
  transformCallback: () => Math.random(),
  convertFileSrc: (s: string) => s,
};

const DARK_COLORS = {
  bg: "#0B1220",
  surface: "#16233B",
  surface2: "#203355",
  text: "#EAF3FF",
  accent: "#00AEFF",
  accentDim: "#5ED0FF",
  sub: "#8AA0C2",
};

const root = document.getElementById("root")!;
root.style.cssText = `background:${DARK_COLORS.bg};height:100vh;display:flex;flex-direction:column;font-family:'Segoe UI',system-ui,sans-serif;`;

createRoot(root).render(
  <SkillManager onClose={() => alert("Close clicked")} />,
);
