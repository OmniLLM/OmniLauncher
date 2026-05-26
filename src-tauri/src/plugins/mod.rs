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

    pub async fn query_all(&self, raw: &str) -> Vec<QueryResult> {
        let q = Query {
            raw: raw.to_string(),
            terms: raw.split_whitespace().map(String::from).collect(),
        };
        let mut results = vec![];
        for plugin in &self.plugins {
            if let Some(kw) = plugin.keyword() {
                if !keyword_matches(raw, kw) {
                    continue;
                }
            }
            let mut r = plugin.query(&q).await;
            results.append(&mut r);
        }
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
