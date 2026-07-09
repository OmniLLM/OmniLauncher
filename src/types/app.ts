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
  ai_max_retry_attempts: number;
  ai_retry_base_delay_ms: number;
  /** When true (default), the agentic tool loop halts after detecting three
   * identical (request, result) iterations in a row. Disable for advanced
   * debugging of long multi-step skills — `ai_max_tool_iterations` is still
   * the upper bound. */
  ai_loop_detector_enabled: boolean;
  theme: string;
  hotkey: string;
  max_results: number;
  background_url: string;
  /** Base URL of the separated backend the desktop shell connects to. Empty =
   * env override / built-in default `http://127.0.0.1:1422`. */
  backend_url: string;

  // ── A2A server settings ────────────────────────────────────────────────
  /** Enable the A2A (Agent-to-Agent) HTTP server. Off by default. */
  a2a_enabled: boolean;
  /** When true the A2A server binds 0.0.0.0 (LAN-accessible) instead of
   * 127.0.0.1 (local-only). Advanced setting. */
  a2a_bind_lan: boolean;
  /** TCP port for the A2A server. Default 1423. */
  a2a_port: number;
  /** Bearer token for A2A authentication. Auto-generated when enabled if
   * absent. */
  a2a_token: string | null;
  /** Public A2A URL advertised to omni-agent-hub. Empty = loopback a2a_port. */
  a2a_public_url: string;
  /** omni-agent-hub admin API URL, e.g. http://127.0.0.1:8222. */
  a2a_hub_url: string;
  /** Hub admin key; prefer env var over saving this in settings. */
  a2a_hub_admin_key: string;
  /** Upstream name registered in omni-agent-hub. */
  a2a_hub_upstream_name: string;
  /** Optional hub routing prefix, e.g. @omnilauncher. */
  a2a_hub_prefix: string;
  /** Auto-upsert this backend into omni-agent-hub on startup. */
  a2a_hub_auto_register: boolean;
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
