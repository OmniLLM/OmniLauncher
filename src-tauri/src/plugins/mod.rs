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
}

pub struct PluginManager {
    pub plugins: Vec<Box<dyn Plugin>>,
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
        Self { plugins: vec![] }
    }

    pub fn register(&mut self, p: Box<dyn Plugin>) {
        self.plugins.push(p);
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
        self.plugins.push(p);
    }

    /// Returns true if any registered plugin has this exact name.
    pub fn has_name(&self, name: &str) -> bool {
        self.plugins.iter().any(|p| p.name() == name)
    }

    /// Returns true if any registered plugin has this exact keyword.
    pub fn has_keyword(&self, kw: &str) -> bool {
        self.plugins
            .iter()
            .any(|p| p.keyword().map_or(false, |k| k == kw))
    }

    /// Remove all plugins whose name matches. Returns the count removed.
    pub fn unregister_by_name(&mut self, name: &str) -> usize {
        let before = self.plugins.len();
        self.plugins.retain(|p| p.name() != name);
        before - self.plugins.len()
    }

    /// Remove all plugins whose keyword matches. Returns the count removed.
    pub fn unregister_by_keyword(&mut self, kw: &str) -> usize {
        let before = self.plugins.len();
        self.plugins
            .retain(|p| p.keyword().map_or(true, |k| k != kw));
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
            .filter(|p| p.keyword().map_or(true, |kw| keyword_matches(raw, kw)))
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
        for p in &self.plugins {
            if p.name() == name {
                return p.execute_tool(args).await;
            }
        }
        "Tool not found".to_string()
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
