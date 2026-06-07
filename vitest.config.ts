import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    // jsdom gives us a browser-shaped DOM so React Testing Library can mount
    // components. Pure-function tests (markdown.test.ts, runtime.test.ts) work
    // fine in jsdom too — no need for a separate node environment.
    environment: "jsdom",
    globals: false, // keep explicit imports — easier to grep, no magic globals
    setupFiles: ["./src/test/setup.ts"],
    include: [
      "src/**/*.test.ts",
      "src/**/*.test.tsx",
    ],
    // Speeds up cold runs in CI: don't transform node_modules' test helpers.
    css: false,
  },
});
