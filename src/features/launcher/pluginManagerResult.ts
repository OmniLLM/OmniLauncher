import type { QueryResult } from "../../types/app";

/** Synthetic result that opens the Plugin Manager panel. */
export function pluginManagerResult(): QueryResult {
  return {
    id: "builtin:plugin-manager",
    title: "Manage Plugins",
    subtitle: "Install, list, and remove external plugins",
    icon: "🔌",
    score: 100,
    action_type: "open_plugin_manager",
    action_data: "",
  };
}
