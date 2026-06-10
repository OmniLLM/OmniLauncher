use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod agent_delegate;
pub mod app_launcher;
pub mod bash_exec;
pub mod browser_bookmarks;
pub mod calculator;
pub mod clipboard;
pub mod code_tools;
pub mod color_picker;
pub mod cron_explainer;
pub mod emoji_picker;
pub mod env_vars;
pub mod external;
pub mod file_read;
pub mod file_search;
pub mod file_write;
pub mod flow;
pub mod git;
pub mod github;
pub mod glob;
pub mod grep;
pub mod hosts;
pub mod http_client;
pub mod ls;
pub mod network;
pub mod plugin_manager_cmd;
pub mod pomodoro;
pub mod process_manager;
pub mod raycast;
pub mod runtime_deps;
pub mod scheduler;
pub mod screenshot;
pub mod script_runner;
pub mod selection;
pub mod shell_plugin;
pub mod skill_credentials;
pub mod skill_runner;
pub mod snippets;
pub mod sys_info;
pub mod system_commands;
pub mod timer;
pub mod todo;
pub mod translate;
pub mod unit_converter;
pub mod url_opener;
pub mod vision_analyze;
pub mod web_fetch;
pub mod web_search;
pub mod window_resize;
pub mod windows_settings;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
    pub score: i32,
    pub action_type: String,
    pub action_data: String,
    /// Plugin / category that produced this result. Used by the launcher UI to
    /// group rows under section headers (Raycast-style). Plugins may leave
    /// this `None` — `query_all` will inject the plugin's `name()` automatically
    /// for every result that doesn't already specify one. Optional so existing
    /// plugins that build `QueryResult { ... }` literals keep compiling without
    /// modification.
    #[serde(default)]
    pub source: Option<String>,
}

pub struct Query {
    pub raw: String,
    pub terms: Vec<String>,
}

#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn keyword(&self) -> Option<&str>;
    async fn query(&self, q: &Query) -> Vec<QueryResult>;
    /// Cheap, synchronous pre-filter used by `query_all` (launcher UI path) to
    /// avoid creating per-keystroke futures for plugins that obviously won't
    /// match. Default returns `true` (preserves legacy behavior).
    ///
    /// IMPORTANT: this is ONLY consulted by `query_all`. AI dispatch through
    /// `execute_tool` / `all_tool_schemas` is unaffected — the plugin is still
    /// fully registered, its tool schema is always exposed, and its
    /// `execute_tool` can always be called.
    fn cheap_prefix_match(&self, _raw: &str) -> bool {
        true
    }
    fn tool_schema(&self) -> Option<serde_json::Value> {
        None
    }
    async fn execute_tool(&self, _args: serde_json::Value) -> String {
        String::new()
    }
    /// Called when the user picks a result that has `action_type: "plugin_execute"`.
    /// Returns `Some(output)` if handled, `None` if the plugin doesn't support it.
    async fn execute_action(&self, _id: &str, _action_data: &str) -> Option<String> {
        None
    }
    /// True for plugins loaded from `~/.omnilauncher/plugins/` (or extra dirs).
    /// Used by `PluginManager::reload_external_plugins` to discard and re-load
    /// only externally-sourced plugins without touching built-ins.
    fn is_external(&self) -> bool {
        false
    }
    /// True when the plugin has been quarantined after repeated failures and is
    /// short-circuiting its calls. Only meaningful for external plugins; the
    /// default is `false`.
    fn is_quarantined(&self) -> bool {
        false
    }
}

pub struct PluginManager {
    pub plugins: Vec<Box<dyn Plugin>>,
    /// Index from plugin name → index in `plugins` for O(1) lookup by name.
    name_index: std::collections::HashMap<String, usize>,
    /// Index from tool schema function name → plugin index. The AI calls tools by
    /// the `function.name` in the schema, which often differs from `plugin.name()`
    /// (e.g. plugin "PowerShell Runner" exposes tool "powershell").
    tool_index: std::collections::HashMap<String, usize>,
}

fn keyword_matches(raw: &str, keyword: &str) -> bool {
    let trimmed = raw.trim_start();
    let Some(rest) = trimmed.strip_prefix(keyword) else {
        return false;
    };

    match keyword.chars().last() {
        Some(last) if last.is_whitespace() || !last.is_alphanumeric() => true,
        Some(_) => rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace),
        None => false,
    }
}

pub(crate) fn truncate_on_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    let mut idx = max_bytes;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    &s[..idx]
}

#[cfg(test)]
mod truncate_tests {
    use super::truncate_on_char_boundary;

    #[test]
    fn ascii_truncates_exactly() {
        let s = "x".repeat(100);
        assert_eq!(truncate_on_char_boundary(&s, 60).len(), 60);
    }

    #[test]
    fn multibyte_truncates_to_previous_boundary() {
        let s = format!("{}{}", "x".repeat(5998), "\u{fffd}");
        let out = truncate_on_char_boundary(&s, 6000);
        assert_eq!(out.len(), 5998);
        assert!(out.is_char_boundary(out.len()));
    }
}

/// Extract the OpenAI-style `function.name` from a plugin's tool schema, if any.
fn tool_schema_function_name(p: &dyn Plugin) -> Option<String> {
    p.tool_schema()
        .as_ref()
        .and_then(|s| s.get("function"))
        .and_then(|f| f.get("name"))
        .and_then(|n| n.as_str())
        .map(str::to_string)
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: vec![],
            name_index: std::collections::HashMap::new(),
            tool_index: std::collections::HashMap::new(),
        }
    }

    pub fn register(&mut self, p: Box<dyn Plugin>) {
        let name = p.name().to_string();
        let keyword = p.keyword().map(str::to_string);
        let tool_name = tool_schema_function_name(p.as_ref());
        let idx = self.plugins.len();
        log::debug!(
            "PluginManager.register: name='{}' keyword={:?} tool_name={:?} idx={}",
            name,
            keyword,
            tool_name,
            idx
        );
        self.plugins.push(p);
        self.name_index.insert(name, idx);
        if let Some(t) = tool_name {
            self.tool_index.insert(t, idx);
        }
    }

    /// Register a plugin, evicting any existing plugin that conflicts on name or keyword.
    /// Use this instead of `register()` when later registrations should win over earlier ones
    /// (e.g. external plugins overriding built-ins, or user plugins overriding defaults).
    pub fn register_override(&mut self, p: Box<dyn Plugin>) {
        let name = p.name().to_string();
        let keyword = p.keyword().map(str::to_string);
        // Compute once and reuse — `tool_schema()` clones the schema value.
        let tool_name = tool_schema_function_name(p.as_ref());

        if self.has_name(&name) {
            log::info!("Plugin '{}' overrides existing plugin with same name", name);
            self.unregister_by_name(&name);
        }
        if let Some(ref kw) = keyword {
            if self.has_keyword(kw) {
                log::info!(
                    "Plugin '{}' overrides existing plugin with keyword '{}'",
                    name,
                    kw
                );
                self.unregister_by_keyword(kw);
            }
        }
        let idx = self.plugins.len();
        self.plugins.push(p);
        self.name_index.insert(name, idx);
        if let Some(t) = tool_name {
            self.tool_index.insert(t, idx);
        }
    }

    fn rebuild_index(&mut self) {
        self.name_index.clear();
        self.tool_index.clear();
        for (i, p) in self.plugins.iter().enumerate() {
            self.name_index.insert(p.name().to_string(), i);
            if let Some(t) = tool_schema_function_name(p.as_ref()) {
                self.tool_index.insert(t, i);
            }
        }
    }

    /// Returns true if any registered plugin has this exact name.
    pub fn has_name(&self, name: &str) -> bool {
        self.name_index.contains_key(name)
    }

    /// Returns true if any registered plugin has this exact keyword.
    pub fn has_keyword(&self, kw: &str) -> bool {
        self.plugins.iter().any(|p| p.keyword() == Some(kw))
    }

    /// Remove all plugins whose name matches. Returns the count removed.
    pub fn unregister_by_name(&mut self, name: &str) -> usize {
        let before = self.plugins.len();
        self.plugins.retain(|p| p.name() != name);
        self.rebuild_index();
        before - self.plugins.len()
    }

    /// Remove all plugins whose keyword matches. Returns the count removed.
    pub fn unregister_by_keyword(&mut self, kw: &str) -> usize {
        let before = self.plugins.len();
        self.plugins.retain(|p| p.keyword() != Some(kw));
        self.rebuild_index();
        before - self.plugins.len()
    }

    pub async fn query_all(&self, raw: &str) -> Vec<QueryResult> {
        let q = Query {
            raw: raw.to_string(),
            terms: raw.split_whitespace().map(String::from).collect(),
        };
        // Run all eligible plugin queries concurrently so a slow plugin (e.g.
        // an external script doing a network call) does not block the others.
        //
        // Each plugin gets a hard time budget — anything slower is dropped
        // from THIS query (it can still appear on a later keystroke when its
        // cache is warm). Without this cap one slow disk read or HTTP call
        // would stall the entire result list and feel "frozen" in launcher
        // mode.
        //
        // Built-ins are in-process and fast, so they keep a tight 1200ms cap.
        // External plugins spawn a child process (Node/Python) and frequently
        // perform a network call (currency rates, web APIs, etc). Cold spawn
        // overhead alone can approach a second, so a 1200ms cap would drop them
        // before their own 3s internal query timeout ever fires — making
        // external plugins appear dead in launcher (non-AI) mode. Give them a
        // budget that comfortably covers their internal timeout.
        const BUILTIN_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1200);
        const EXTERNAL_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(3500);
        let q_ref = &q;
        let futures = self
            .plugins
            .iter()
            .filter(|p| match p.keyword() {
                Some(kw) => keyword_matches(raw, kw),
                // For keyword-less plugins, consult the cheap prefix filter so
                // we don't spawn a future for every plugin on every keystroke.
                // Default impl returns true → behavior unchanged for plugins
                // that don't override it.
                None => p.cheap_prefix_match(raw),
            })
            .map(|p| {
                let plugin_name = p.name().to_string();
                let plugin_timeout = if p.is_external() {
                    EXTERNAL_TIMEOUT
                } else {
                    BUILTIN_TIMEOUT
                };
                async move {
                    match tokio::time::timeout(plugin_timeout, p.query(q_ref)).await {
                        Ok(mut v) => {
                            // Bound memory if a plugin misbehaves and returns a huge
                            // list — we only ever surface the top results anyway.
                            v.truncate(20);
                            // Stamp the plugin name onto every result so the
                            // launcher UI can group by source. Plugins that
                            // already set their own source (e.g. a meta-plugin
                            // surfacing results from multiple subsystems)
                            // win — we only fill in the gap.
                            for r in v.iter_mut() {
                                if r.source.is_none() {
                                    r.source = Some(plugin_name.clone());
                                }
                            }
                            v
                        }
                        Err(_) => {
                            log::warn!(
                                "plugin '{}' exceeded {}ms budget for query {:?} — dropped",
                                plugin_name,
                                plugin_timeout.as_millis(),
                                raw
                            );
                            Vec::new()
                        }
                    }
                }
            });
        let mut results: Vec<QueryResult> = futures_util::future::join_all(futures)
            .await
            .into_iter()
            .flatten()
            .collect();
        results.sort_by_key(|b| std::cmp::Reverse(b.score));
        results.truncate(10);
        results
    }

    pub fn all_tool_schemas(&self) -> Vec<serde_json::Value> {
        let schemas: Vec<serde_json::Value> = self
            .plugins
            .iter()
            .filter_map(|p| p.tool_schema())
            .collect();
        if log::log_enabled!(log::Level::Debug) {
            let names: Vec<&str> = schemas
                .iter()
                .filter_map(|s| {
                    s.get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                })
                .collect();
            log::debug!(
                "PluginManager.all_tool_schemas: {}/{} plugins expose tools: {:?}",
                schemas.len(),
                self.plugins.len(),
                names
            );
        }
        schemas
    }

    pub fn tool_display_name(&self, name: &str) -> String {
        self.tool_index
            .get(name)
            .or_else(|| self.name_index.get(name))
            .map(|idx| self.plugins[*idx].name().to_string())
            .unwrap_or_else(|| name.to_string())
    }

    pub async fn execute_tool(&self, name: &str, args: serde_json::Value) -> String {
        log::debug!(
            "PluginManager.execute_tool: name='{}' args={}",
            name,
            crate::log_masking::mask_json(&args)
        );
        // AI calls tools by their schema function name; fall back to plugin name
        // for backward compatibility with callers that pass plugin.name() directly.
        let idx = self
            .tool_index
            .get(name)
            .or_else(|| self.name_index.get(name))
            .copied();
        if let Some(idx) = idx {
            return self.plugins[idx].execute_tool(args).await;
        }
        log::warn!(
            "PluginManager.execute_tool: tool '{}' not registered (known tools: {:?})",
            name,
            self.tool_index.keys().collect::<Vec<_>>()
        );
        "Tool not found".to_string()
    }

    /// Dispatch a `plugin_execute` callback to the named plugin.
    pub async fn execute_action(
        &self,
        plugin_name: &str,
        id: &str,
        action_data: &str,
    ) -> Option<String> {
        log::debug!(
            "PluginManager.execute_action: plugin='{}' id='{}' action_data_len={}",
            plugin_name,
            id,
            action_data.len()
        );
        if let Some(&idx) = self.name_index.get(plugin_name) {
            return self.plugins[idx].execute_action(id, action_data).await;
        }
        log::warn!(
            "PluginManager.execute_action: plugin '{}' not registered",
            plugin_name
        );
        None
    }

    /// Names of all plugins currently quarantined (after repeated failures).
    pub fn quarantined_plugin_names(&self) -> Vec<String> {
        self.plugins
            .iter()
            .filter(|p| p.is_quarantined())
            .map(|p| p.name().to_string())
            .collect()
    }

    /// Drop all externally-sourced plugins and re-discover them from
    /// `~/.omnilauncher/plugins/` plus `extra_dirs`. Built-in plugins are
    /// untouched, preserving their internal state (caches, indexes, etc.).
    ///
    /// Call this after install / update / remove operations so the in-memory
    /// registry reflects what's on disk without restarting the launcher.
    pub fn reload_external_plugins(&mut self, extra_dirs: &[String]) {
        let before = self.plugins.len();
        // Drop all existing externals. Built-ins are kept (and their state).
        self.plugins.retain(|p| !p.is_external());

        // Collect the freshly-discovered external plugins up front.
        let new_plugins: Vec<Box<dyn Plugin>> = external::load_external_plugins_from(extra_dirs)
            .into_iter()
            .map(|p| Box::new(p) as Box<dyn Plugin>)
            .collect();

        // Externals win over built-ins on name/keyword conflicts. Externals were
        // already removed above, so the only possible conflicts now are with
        // built-ins — evict those in a single retain pass instead of paying
        // register_override's per-plugin rebuild_index cost.
        let new_names: std::collections::HashSet<&str> =
            new_plugins.iter().map(|p| p.name()).collect();
        let new_keywords: std::collections::HashSet<&str> =
            new_plugins.iter().filter_map(|p| p.keyword()).collect();
        self.plugins.retain(|p| {
            !new_names.contains(p.name())
                && !p.keyword().is_some_and(|kw| new_keywords.contains(kw))
        });

        // Append all new externals, then rebuild the index exactly once.
        for plugin in new_plugins {
            self.plugins.push(plugin);
        }
        self.rebuild_index();

        log::info!(
            "PluginManager.reload_external_plugins: {} → {} registered plugins",
            before,
            self.plugins.len()
        );
    }
}

#[cfg(test)]
mod plugin_manager_tests {
    use super::*;

    struct FakePlugin {
        name: &'static str,
        kw: Option<&'static str>,
    }

    #[async_trait::async_trait]
    impl Plugin for FakePlugin {
        fn name(&self) -> &str {
            self.name
        }
        fn description(&self) -> &str {
            "fake"
        }
        fn keyword(&self) -> Option<&str> {
            self.kw
        }
        async fn query(&self, _q: &Query) -> Vec<QueryResult> {
            vec![]
        }
    }

    /// Plugin whose display name and tool-schema function name intentionally diverge,
    /// matching the real-world shape of community plugins like "PowerShell Runner"
    /// exposing tool name "powershell".
    struct ToolPlugin {
        plugin_name: &'static str,
        tool_name: &'static str,
    }

    #[async_trait::async_trait]
    impl Plugin for ToolPlugin {
        fn name(&self) -> &str {
            self.plugin_name
        }
        fn description(&self) -> &str {
            "tool fake"
        }
        fn keyword(&self) -> Option<&str> {
            None
        }
        async fn query(&self, _q: &Query) -> Vec<QueryResult> {
            vec![]
        }
        fn tool_schema(&self) -> Option<serde_json::Value> {
            Some(serde_json::json!({
                "type": "function",
                "function": {
                    "name": self.tool_name,
                    "description": "t",
                    "parameters": {"type": "object", "properties": {}}
                }
            }))
        }
        async fn execute_tool(&self, _args: serde_json::Value) -> String {
            format!("ran:{}", self.tool_name)
        }
    }

    #[tokio::test]
    async fn test_execute_tool_finds_by_name() {
        let mut pm = PluginManager::new();
        pm.register(Box::new(FakePlugin {
            name: "my_tool",
            kw: None,
        }));
        let result = pm.execute_tool("my_tool", serde_json::json!({})).await;
        assert_eq!(result, "");
    }

    #[tokio::test]
    async fn test_execute_tool_not_found() {
        let pm = PluginManager::new();
        let result = pm.execute_tool("nonexistent", serde_json::json!({})).await;
        assert_eq!(result, "Tool not found");
    }

    #[test]
    fn test_register_override_replaces_by_name() {
        let mut pm = PluginManager::new();
        pm.register(Box::new(FakePlugin {
            name: "tool_a",
            kw: None,
        }));
        pm.register_override(Box::new(FakePlugin {
            name: "tool_a",
            kw: None,
        }));
        // Should still be exactly 1 plugin
        assert_eq!(pm.plugins.len(), 1);
    }

    #[tokio::test]
    async fn test_execute_tool_dispatches_by_schema_function_name() {
        // Reproduces the real bug: plugin "PowerShell Runner" exposes tool
        // schema named "powershell". AI calls execute_tool("powershell", ...)
        // and must reach the plugin.
        let mut pm = PluginManager::new();
        pm.register(Box::new(ToolPlugin {
            plugin_name: "PowerShell Runner",
            tool_name: "powershell",
        }));
        let result = pm.execute_tool("powershell", serde_json::json!({})).await;
        assert_eq!(result, "ran:powershell");
    }

    #[test]
    fn test_tool_display_name_resolves_schema_function_name() {
        let mut pm = PluginManager::new();
        pm.register(Box::new(ToolPlugin {
            plugin_name: "Browser History",
            tool_name: "F6B8C1BC8441496798D2CE2BADB0E95E",
        }));
        assert_eq!(
            pm.tool_display_name("F6B8C1BC8441496798D2CE2BADB0E95E"),
            "Browser History"
        );
    }

    #[tokio::test]
    async fn test_execute_tool_still_finds_by_plugin_name_fallback() {
        // Backward compat: when a caller (or a plugin without a schema)
        // passes plugin.name(), it should still resolve.
        let mut pm = PluginManager::new();
        pm.register(Box::new(ToolPlugin {
            plugin_name: "PowerShell Runner",
            tool_name: "powershell",
        }));
        let result = pm
            .execute_tool("PowerShell Runner", serde_json::json!({}))
            .await;
        assert_eq!(result, "ran:powershell");
    }

    #[test]
    fn test_unregister_rebuilds_tool_index() {
        let mut pm = PluginManager::new();
        pm.register(Box::new(ToolPlugin {
            plugin_name: "PowerShell Runner",
            tool_name: "powershell",
        }));
        assert_eq!(pm.unregister_by_name("PowerShell Runner"), 1);
        assert!(!pm.tool_index.contains_key("powershell"));
        assert!(!pm.name_index.contains_key("PowerShell Runner"));
    }
}
