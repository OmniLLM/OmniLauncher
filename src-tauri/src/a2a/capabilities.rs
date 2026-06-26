use serde_json::{json, Value};

use crate::{
    plugins::{PluginManager, QueryResult},
    SkillManager,
};

use super::types::AgentSkill;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum A2aCapabilityKind {
    ToolSchemaPlugin,
    QueryPlugin,
    LauncherQuery,
    Skill,
}

#[derive(Debug, Clone)]
pub struct A2aCapability {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<Value>,
    pub tags: Vec<String>,
    pub kind: A2aCapabilityKind,
    pub target: String,
}

pub fn build_capabilities(pm: &PluginManager, skills: Option<&SkillManager>) -> Vec<A2aCapability> {
    let mut capabilities = Vec::new();

    for schema in pm.all_tool_schemas() {
        if let Some(func) = schema.get("function") {
            let name = func
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let desc = func
                .get("description")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let params = func.get("parameters").cloned();
            capabilities.push(A2aCapability {
                id: format!("plugin:tool:{name}"),
                name: name.clone(),
                description: desc,
                input_schema: params,
                tags: vec!["plugin".to_string(), "tool".to_string()],
                kind: A2aCapabilityKind::ToolSchemaPlugin,
                target: name,
            });
        }
    }

    for plugin in &pm.plugins {
        let plugin_name = plugin.name().to_string();
        let already_tool = capabilities.iter().any(|cap| {
            cap.kind == A2aCapabilityKind::ToolSchemaPlugin
                && (cap.name == plugin_name || cap.target == plugin_name)
        });
        if already_tool {
            continue;
        }

        let mut tags = vec!["plugin".to_string(), "query".to_string()];
        if let Some(keyword) = plugin.keyword() {
            tags.push(keyword.to_string());
        }

        let description = plugin.description().trim().to_string();
        capabilities.push(A2aCapability {
            id: format!("plugin:query:{plugin_name}"),
            name: plugin_name.clone(),
            description: if description.is_empty() { None } else { Some(description) },
            input_schema: Some(query_input_schema()),
            tags,
            kind: A2aCapabilityKind::QueryPlugin,
            target: plugin_name,
        });
    }

    capabilities.push(A2aCapability {
        id: "launcher:query_all".to_string(),
        name: "launcher_query_all".to_string(),
        description: Some("Search all OmniLauncher plugins and return launcher results".to_string()),
        input_schema: Some(query_input_schema()),
        tags: vec!["launcher".to_string(), "query".to_string()],
        kind: A2aCapabilityKind::LauncherQuery,
        target: "query_all".to_string(),
    });

    if let Some(skill_manager) = skills {
        for skill in skill_manager.list_meta() {
            capabilities.push(A2aCapability {
                id: format!("skill:{}", skill.name),
                name: skill.name.clone(),
                description: Some(skill.description.clone()),
                input_schema: Some(skill_input_schema()),
                tags: skill.tags.clone(),
                kind: A2aCapabilityKind::Skill,
                target: skill.name.clone(),
            });
        }
    }

    capabilities.sort_by(|a, b| a.id.cmp(&b.id));
    capabilities.dedup_by(|a, b| a.id == b.id);
    capabilities
}

pub fn capability_to_agent_skill(capability: &A2aCapability) -> AgentSkill {
    AgentSkill {
        id: capability.id.clone(),
        name: capability.name.clone(),
        description: capability.description.clone(),
        input_schema: capability.input_schema.clone(),
        tags: capability.tags.clone(),
    }
}

pub fn query_results_artifact(results: Vec<QueryResult>) -> Value {
    json!({ "results": results })
}

fn query_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Search query text"
            }
        },
        "required": ["query"]
    })
}

fn skill_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Skill request or task text"
            }
        },
        "required": ["query"]
    })
}
