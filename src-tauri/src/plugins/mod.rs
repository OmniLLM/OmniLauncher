use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
    pub score: i32,
    pub action_type: String,
    pub action_data: String,
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
}

pub struct PluginManager {
    pub plugins: Vec<Box<dyn Plugin>>,
    /// Index from plugin name → index in `plugins` for O(1) tool lookup.
    name_index: std::collections::HashMap<String, usize>,
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

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        Self { plugins: vec![], name_index: std::collections::HashMap::new() }
    }

    pub fn register(&mut self, p: Box<dyn Plugin>) {
        let name = p.name().to_string();
        let idx = self.plugins.len();
        self.plugins.push(p);
        self.name_index.insert(name, idx);
    }

    /// Register a plugin, evicting any existing plugin that conflicts on name or keyword.
    /// Use this instead of `register()` when later registrations should win over earlier ones
    /// (e.g. external plugins overriding built-ins, or user plugins overriding defaults).
    pub fn register_override(&mut self, p: Box<dyn Plugin>) {
        let name = p.name().to_string();
        let keyword = p.keyword().map(str::to_string);

        if self.has_name(&name) {
            log::info!(
                "Plugin '{}' overrides existing plugin with same name",
                name
            );
            self.unregister_by_name(&name);
        }
        if let Some(ref kw) = keyword {
            if self.has_keyword(kw) {
                log::info!(
                    "Plugin '{}' overrides existing plugin with keyword '{}'",
                    name, kw
                );
                self.unregister_by_keyword(kw);
            }
        }
        let idx = self.plugins.len();
        self.plugins.push(p);
        self.name_index.insert(name, idx);
    }

    fn rebuild_index(&mut self) {
        self.name_index.clear();
        for (i, p) in self.plugins.iter().enumerate() {
            self.name_index.insert(p.name().to_string(), i);
        }
    }

    /// Returns true if any registered plugin has this exact name.
    pub fn has_name(&self, name: &str) -> bool {
        self.name_index.contains_key(name)
    }

    /// Returns true if any registered plugin has this exact keyword.
    pub fn has_keyword(&self, kw: &str) -> bool {
        self.plugins
            .iter()
            .any(|p| p.keyword() == Some(kw))
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
        self.plugins
            .retain(|p| p.keyword() != Some(kw));
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
        let futures = self
            .plugins
            .iter()
            .filter(|p| p.keyword().is_none_or(|kw| keyword_matches(raw, kw)))
            .map(|p| p.query(&q));
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
        self.plugins
            .iter()
            .filter_map(|p| p.tool_schema())
            .collect()
    }

    pub async fn execute_tool(&self, name: &str, args: serde_json::Value) -> String {
        if let Some(&idx) = self.name_index.get(name) {
            return self.plugins[idx].execute_tool(args).await;
        }
        "Tool not found".to_string()
    }

    /// Dispatch a `plugin_execute` callback to the named plugin.
    pub async fn execute_action(
        &self,
        plugin_name: &str,
        id: &str,
        action_data: &str,
    ) -> Option<String> {
        if let Some(&idx) = self.name_index.get(plugin_name) {
            return self.plugins[idx].execute_action(id, action_data).await;
        }
        None
    }
}

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
pub mod scheduler;
pub mod screenshot;
pub mod script_runner;
pub mod selection;
pub mod shell_plugin;
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

#[cfg(test)]
mod plugin_manager_tests {
    use super::*;

    struct FakePlugin { name: &'static str, kw: Option<&'static str> }

    #[async_trait::async_trait]
    impl Plugin for FakePlugin {
        fn name(&self) -> &str { self.name }
        fn description(&self) -> &str { "fake" }
        fn keyword(&self) -> Option<&str> { self.kw }
        async fn query(&self, _q: &Query) -> Vec<QueryResult> { vec![] }
    }

    #[tokio::test]
    async fn test_execute_tool_finds_by_name() {
        let mut pm = PluginManager::new();
        pm.register(Box::new(FakePlugin { name: "my_tool", kw: None }));
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
        pm.register(Box::new(FakePlugin { name: "tool_a", kw: None }));
        pm.register_override(Box::new(FakePlugin { name: "tool_a", kw: None }));
        // Should still be exactly 1 plugin
        assert_eq!(pm.plugins.len(), 1);
    }
}
