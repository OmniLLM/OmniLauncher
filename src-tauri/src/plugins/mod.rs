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
                if !raw.starts_with(kw) {
                    continue;
                }
            }
            let mut r = plugin.query(&q).await;
            results.append(&mut r);
        }
        results.sort_by(|a, b| b.score.cmp(&a.score));
        results.truncate(10);
        results
    }

    pub fn all_tool_schemas(&self) -> Vec<serde_json::Value> {
        self.plugins.iter().filter_map(|p| p.tool_schema()).collect()
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

pub mod app_launcher;
pub mod calculator;
pub mod file_search;
pub mod shell_plugin;
pub mod system_commands;
pub mod web_search;
