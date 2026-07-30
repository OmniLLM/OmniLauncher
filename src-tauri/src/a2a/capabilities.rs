use serde_json::{json, Value};

use crate::{
    plugins::{PluginManager, Query, QueryResult},
    SkillManager,
};

use super::types::{A2aArtifact, A2aMessage, A2aPart, AgentSkill, MessageSendRequest};

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

    // A plugin is exposed exactly once: as a tool capability when it publishes
    // an OpenAI tool schema, otherwise as a query capability. Matching the two
    // by name is unreliable (e.g. MCP plugins are named `MCP server/tool` while
    // their schema function is `mcp_server_tool`), which previously produced a
    // duplicate `plugin:query:*` entry for every `plugin:tool:*` entry.
    for plugin in &pm.plugins {
        let plugin_name = plugin.name().to_string();

        if let Some(func) = plugin
            .tool_schema()
            .as_ref()
            .and_then(|schema| schema.get("function").cloned())
        {
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
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            input_schema: Some(query_input_schema()),
            tags,
            kind: A2aCapabilityKind::QueryPlugin,
            target: plugin_name,
        });
    }

    capabilities.push(A2aCapability {
        id: "launcher:query_all".to_string(),
        name: "launcher_query_all".to_string(),
        description: Some(
            "Search all OmniLauncher plugins and return launcher results".to_string(),
        ),
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
    // `description` and `tags` are REQUIRED in the v1.0 proto `AgentSkill`, so
    // fall back to the skill name rather than emitting a null.
    AgentSkill {
        id: capability.id.clone(),
        name: capability.name.clone(),
        description: capability
            .description
            .clone()
            .unwrap_or_else(|| capability.name.clone()),
        tags: capability.tags.clone(),
        examples: Vec::new(),
        // Capabilities with a JSON-Schema input take structured arguments; the
        // rest are plain text. Declared per-skill, overriding the card default.
        input_modes: match capability.input_schema {
            Some(_) => vec!["application/json".to_string(), "text/plain".to_string()],
            None => vec!["text/plain".to_string()],
        },
        output_modes: vec!["text/plain".to_string(), "application/json".to_string()],
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
            },
            "op": {
                "type": "string",
                "description": "Skill runner operation: tool_call, query, or execute"
            },
            "args": {
                "type": "object",
                "description": "Structured arguments for tool_call"
            },
            "action_data": {
                "type": "string",
                "description": "Action data for execute"
            }
        }
    })
}

pub async fn execute_capability(
    pm: &PluginManager,
    skills: Option<&SkillManager>,
    capability_id: &str,
    request: &MessageSendRequest,
) -> Result<(Vec<A2aMessage>, Vec<A2aArtifact>), String> {
    let capabilities = build_capabilities(pm, skills);
    let capability = capabilities
        .iter()
        .find(|cap| cap.id == capability_id || cap.target == capability_id)
        .ok_or_else(|| format!("Tool not found: {capability_id}"))?;

    match capability.kind {
        A2aCapabilityKind::ToolSchemaPlugin => {
            execute_tool_schema_plugin(pm, capability, request).await
        }
        A2aCapabilityKind::QueryPlugin => execute_query_plugin(pm, capability, request).await,
        A2aCapabilityKind::LauncherQuery => execute_launcher_query(pm, request).await,
        A2aCapabilityKind::Skill => {
            // `skill:*` capabilities are Claude-Code-style skills (SKILL.md +
            // scripts/). They are not directly executable — the caller must
            // route them through the AI conversational path instead, which
            // understands how to load the skill and pick the right execution
            // mechanism (`shell_exec`, `code_execute`, or `execute_skill` for
            // legacy `run.py` skills). We surface this as a hard error so any
            // caller that reaches here has a bug; the adapter is expected to
            // detect `Skill` kind via `find_capability` and short-circuit.
            Err(format!(
                "skill:* capabilities must be handled conversationally, not \
                 dispatched directly: {}",
                capability.id
            ))
        }
    }
}

/// Look up a capability by id or target without executing it. Used by the
/// adapter to decide whether a `skillId` should be dispatched directly (plugin
/// tools, query plugins, launcher) or routed through the AI (`skill:*`).
///
/// Returns the matched capability (cloned) or `None` if no capability matches.
pub fn find_capability(
    pm: &PluginManager,
    skills: Option<&SkillManager>,
    capability_id: &str,
) -> Option<A2aCapability> {
    build_capabilities(pm, skills)
        .into_iter()
        .find(|cap| cap.id == capability_id || cap.target == capability_id)
}

async fn execute_tool_schema_plugin(
    pm: &PluginManager,
    capability: &A2aCapability,
    request: &MessageSendRequest,
) -> Result<(Vec<A2aMessage>, Vec<A2aArtifact>), String> {
    let args = extract_tool_args(request);
    let output = pm.execute_tool(&capability.target, args).await;
    if output == "Tool not found" {
        return Err(format!("Tool not found: {}", capability.target));
    }
    Ok(text_response(output))
}

async fn execute_query_plugin(
    pm: &PluginManager,
    capability: &A2aCapability,
    request: &MessageSendRequest,
) -> Result<(Vec<A2aMessage>, Vec<A2aArtifact>), String> {
    let query_text = extract_query_text(request);
    let query = Query {
        raw: query_text.clone(),
        terms: query_text.split_whitespace().map(str::to_string).collect(),
    };
    let plugin = pm
        .plugins
        .iter()
        .find(|plugin| plugin.name() == capability.target)
        .ok_or_else(|| format!("Tool not found: {}", capability.id))?;
    let results = plugin.query(&query).await;
    Ok(query_results_response(results))
}

async fn execute_launcher_query(
    pm: &PluginManager,
    request: &MessageSendRequest,
) -> Result<(Vec<A2aMessage>, Vec<A2aArtifact>), String> {
    let query_text = extract_query_text(request);
    let results = pm.query_all(&query_text).await;
    Ok(query_results_response(results))
}

fn text_response(output: String) -> (Vec<A2aMessage>, Vec<A2aArtifact>) {
    (vec![A2aMessage::agent_text(output)], vec![])
}

fn query_results_response(results: Vec<QueryResult>) -> (Vec<A2aMessage>, Vec<A2aArtifact>) {
    let count = results.len();
    let artifact = A2aArtifact {
        artifact_id: super::tasks::generate_task_id(),
        name: Some("query_results".to_string()),
        description: Some(format!("{count} launcher results")),
        parts: vec![A2aPart::data(query_results_artifact(results))],
        metadata: None,
    };
    let message = A2aMessage::agent_text(format!("Found {count} result(s)"));
    (vec![message], vec![artifact])
}

fn extract_tool_args(request: &MessageSendRequest) -> Value {
    request
        .messages
        .first()
        .and_then(|message| message.parts.first())
        .map(|part| match part.as_data() {
            Some(data) => data.clone(),
            None => json!({ "input": part.as_text().unwrap_or_default() }),
        })
        .unwrap_or_else(|| json!({}))
}

fn extract_query_text(request: &MessageSendRequest) -> String {
    request
        .messages
        .first()
        .and_then(|message| message.parts.first())
        .map(|part| match part.as_data() {
            Some(data) => data
                .get("query")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            None => part.as_text().unwrap_or_default().to_string(),
        })
        .unwrap_or_default()
}
