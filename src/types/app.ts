export interface QueryResult {
  id: string;
  title: string;
  subtitle?: string;
  icon?: string;
  score: number;
  action_type: string;
  action_data: string;
  source?: string;
}

export interface AiResponse {
  content: string;
  tools_used: string[];
  results: QueryResult[];
  is_ai: boolean;
}

export interface ConversationTurn {
  role: "user" | "assistant";
  content: string;
  tools_used?: string[];
  isStreaming?: boolean;
}

export interface AiSessionInfo {
  id: number;
  title: string;
  created_at: string;
  last_active_at: string;
  message_count: number;
}

export interface AppSettings {
  ai_base_url: string;
  ai_model: string;
  ai_api_key: string;
  ai_timeout_secs: number;
  ai_max_tool_iterations: number;
  theme: string;
  hotkey: string;
  max_results: number;
  background_url: string;
  /** Base URL of the separated backend the desktop shell connects to. Empty =
   * env override / built-in default `http://127.0.0.1:1422`. */
  backend_url: string;
  /** Bearer/auth token the desktop shell sends to the separated backend.
   * Required for cross-machine deployments (e.g. WSL backend + Windows shell)
   * where the shell can't read the backend's same-machine token file. Empty =
   * fall back to env / local token file. */
  backend_token: string;
}

export interface PluginInfo {
  name: string;
  description: string;
  version: string;
  keyword?: string;
  icon?: string;
  entry: string;
  dir_name: string;
}

export interface RuntimeDependency {
  id: string;
  label: string;
  installed: boolean;
  installable: boolean;
  install_command?: string | null;
  detail: string;
}

export interface RuntimeProgressEvent {
  id: string;
  label: string;
  message: string;
}

export interface SkillInfo {
  name: string;
  description: string;
  version: string;
  triggers: string[];
  tags: string[];
  tools_hint: string[];
  path: string;
}
