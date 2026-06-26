# A2A Full Capability Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make A2A discovery and direct execution expose the same plugin, skill, and agent capabilities OmniLauncher agents can use.

**Architecture:** Add a focused `a2a/capabilities.rs` module that derives normalized A2A capabilities from `PluginManager` and `SkillManager`, converts them into Agent Card skills, and dispatches direct execution by capability id. Keep the existing A2A server/task registry intact; route only discovery and direct `message:send` through the new registry.

**Tech Stack:** Rust 2021, Tokio, serde/serde_json, existing OmniLauncher plugin/skill traits, existing A2A adapter/server modules.

---

## File Structure

- Create `src-tauri/src/a2a/capabilities.rs`
  - Owns `A2aCapability`, `A2aCapabilityKind`, capability id helpers, discovery conversion, and direct execution.
  - Keeps A2A-specific normalization out of `adapter.rs`.
- Modify `src-tauri/src/a2a/mod.rs`
  - Export the new module.
- Modify `src-tauri/src/a2a/adapter.rs`
  - Replace local Agent Card skill construction with `capabilities::build_capabilities` + `capability_to_agent_skill`.
  - Replace `execute_direct_tool` internals with `capabilities::execute_capability`.
- Modify tests in `src-tauri/src/a2a/adapter.rs`
  - Add failing tests for query-only capability discovery and execution.
  - Update existing Agent Card tests to expect parity behavior.
- Optionally modify tests in `src-tauri/src/a2a/server.rs`
  - Keep existing discovery route test valid after Agent Card generation changes.

---

### Task 1: Add failing tests for query-only capability discovery

**Files:**
- Modify: `src-tauri/src/a2a/adapter.rs`
- Test: `src-tauri/src/a2a/adapter.rs` module tests

- [ ] **Step 1: Add a query-only test plugin in the adapter test module**

Add this inside `#[cfg(test)] mod tests` in `src-tauri/src/a2a/adapter.rs`, near the existing test helpers/imports:

```rust
use async_trait::async_trait;
use crate::plugins::{Plugin, Query, QueryResult};

struct QueryOnlyPlugin;

#[async_trait]
impl Plugin for QueryOnlyPlugin {
    fn name(&self) -> &str {
        "Query Only Test"
    }

    fn description(&self) -> &str {
        "Searches query-only test data"
    }

    fn keyword(&self) -> Option<&str> {
        Some("qo")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        if q.raw.contains("needle") {
            vec![QueryResult {
                id: "query-only-hit".to_string(),
                title: "Needle Result".to_string(),
                subtitle: Some("Found by query-only plugin".to_string()),
                icon: None,
                score: 100,
                action_type: "none".to_string(),
                action_data: String::new(),
                source: Some("Query Only Test".to_string()),
            }]
        } else {
            vec![]
        }
    }
}
```

- [ ] **Step 2: Add the failing discovery test**

Add this test to the same module:

```rust
#[test]
fn agent_card_includes_query_only_plugin_capability() {
    let mut pm = PluginManager::new();
    pm.register(Box::new(QueryOnlyPlugin));

    let card = build_agent_card("http://127.0.0.1:1423", &pm);

    let query_skill = card
        .skills
        .iter()
        .find(|skill| skill.id == "plugin:query:Query Only Test")
        .expect("query-only plugin should be exposed as an A2A capability");
    assert_eq!(query_skill.name, "Query Only Test");
    assert_eq!(
        query_skill.description.as_deref(),
        Some("Searches query-only test data")
    );
    assert!(query_skill.tags.iter().any(|tag| tag == "qo"));
    assert!(query_skill.input_schema.is_some());
}
```

- [ ] **Step 3: Run the test and verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml a2a::adapter::tests::agent_card_includes_query_only_plugin_capability
```

Expected: FAIL because the current Agent Card id is not `plugin:query:Query Only Test` and no normalized capability input schema exists.

- [ ] **Step 4: Commit nothing yet**

This task intentionally leaves a failing test for Task 2.

---

### Task 2: Implement capability discovery registry

**Files:**
- Create: `src-tauri/src/a2a/capabilities.rs`
- Modify: `src-tauri/src/a2a/mod.rs`
- Modify: `src-tauri/src/a2a/adapter.rs`

- [ ] **Step 1: Create `src-tauri/src/a2a/capabilities.rs`**

Create the file with this implementation:

```rust
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
                target: skill.name,
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
```

- [ ] **Step 2: Export the module**

Modify `src-tauri/src/a2a/mod.rs` to include:

```rust
pub mod capabilities;
```

Keep existing module exports unchanged.

- [ ] **Step 3: Update `build_agent_card` in `adapter.rs`**

Change imports at the top of `adapter.rs` to include capabilities:

```rust
use super::{
    capabilities::{build_capabilities, capability_to_agent_skill},
    tasks::TaskRegistry,
    types::*,
};
```

Replace the body section that manually builds `skills` from `pm.all_tool_schemas()` and `pm.plugins` with:

```rust
    let skills = build_capabilities(pm, None)
        .iter()
        .map(capability_to_agent_skill)
        .collect();
```

Leave the returned `AgentCard` fields unchanged.

- [ ] **Step 4: Run the discovery test and verify it passes**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml a2a::adapter::tests::agent_card_includes_query_only_plugin_capability
```

Expected: PASS.

- [ ] **Step 5: Run all A2A adapter tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml a2a::adapter::tests::
```

Expected: PASS.

- [ ] **Step 6: Commit discovery registry**

```bash
git add src-tauri/src/a2a/capabilities.rs src-tauri/src/a2a/mod.rs src-tauri/src/a2a/adapter.rs
git commit --author="James Zhu <zhujian0805@gmail.com>" -m "feat: expose A2A capability registry"
```

Do not add `Co-Authored-By`.

---

### Task 3: Add failing tests for direct query capability execution

**Files:**
- Modify: `src-tauri/src/a2a/adapter.rs`

- [ ] **Step 1: Add an A2A adapter state test helper**

Inside `adapter.rs` tests, add:

```rust
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::{
    ai::{client::AiClient, router::ConversationContext},
    AppSettings, SkillManager,
};

fn test_adapter_state_with_plugin(plugin: Box<dyn Plugin>) -> A2aAdapterState {
    let mut pm = PluginManager::new();
    pm.register(plugin);

    A2aAdapterState {
        plugin_manager: Arc::new(Mutex::new(pm)),
        ai_client: Arc::new(Mutex::new(AiClient::new(
            String::new(),
            String::new(),
            String::new(),
        ))),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        conversation: Arc::new(Mutex::new(ConversationContext::new(10))),
        skill_manager: Arc::new(Mutex::new(SkillManager::new())),
        task_registry: Arc::new(Mutex::new(TaskRegistry::new(10))),
    }
}
```

- [ ] **Step 2: Add the failing direct execution test**

Add:

```rust
#[tokio::test]
async fn message_send_invokes_query_only_capability() {
    let state = test_adapter_state_with_plugin(Box::new(QueryOnlyPlugin));
    let request = MessageSendRequest {
        tool: Some("plugin:query:Query Only Test".to_string()),
        messages: vec![A2aMessage {
            role: "user".to_string(),
            parts: vec![A2aPart::Data {
                data: serde_json::json!({ "query": "needle" }),
            }],
        }],
    };

    let task = handle_message_send(&state, request).await.unwrap();

    assert_eq!(task.status.state, A2aTaskState::Completed);
    let artifact = task.artifacts.first().expect("query results artifact");
    let A2aPart::Data { data } = &artifact.parts[0] else {
        panic!("query results artifact should be structured data");
    };
    assert_eq!(data["results"][0]["title"], "Needle Result");
}
```

- [ ] **Step 3: Run the test and verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml a2a::adapter::tests::message_send_invokes_query_only_capability
```

Expected: FAIL because direct execution still calls `PluginManager::execute_tool` and cannot resolve `plugin:query:Query Only Test`.

---

### Task 4: Implement direct capability execution for tool and query capabilities

**Files:**
- Modify: `src-tauri/src/a2a/capabilities.rs`
- Modify: `src-tauri/src/a2a/adapter.rs`

- [ ] **Step 1: Add execution helpers to `capabilities.rs`**

Append these functions to `capabilities.rs`:

```rust
use super::types::{A2aArtifact, A2aMessage, A2aPart, MessageSendRequest};
use crate::plugins::Query;

pub async fn execute_capability(
    pm: &PluginManager,
    capability_id: &str,
    request: &MessageSendRequest,
) -> Result<(Vec<A2aMessage>, Vec<A2aArtifact>), String> {
    let capabilities = build_capabilities(pm, None);
    let capability = capabilities
        .iter()
        .find(|cap| cap.id == capability_id || cap.target == capability_id)
        .ok_or_else(|| format!("Tool not found: {capability_id}"))?;

    match capability.kind {
        A2aCapabilityKind::ToolSchemaPlugin => execute_tool_schema_plugin(pm, capability, request).await,
        A2aCapabilityKind::QueryPlugin => execute_query_plugin(pm, capability, request).await,
        A2aCapabilityKind::LauncherQuery => execute_launcher_query(pm, request).await,
        A2aCapabilityKind::Skill => Err(format!(
            "Skill capability execution is not available through direct A2A dispatch yet: {}",
            capability.name
        )),
    }
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
    Ok((
        vec![A2aMessage {
            role: "agent".to_string(),
            parts: vec![A2aPart::Text { text: output }],
        }],
        vec![],
    ))
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

fn query_results_response(
    results: Vec<QueryResult>,
) -> (Vec<A2aMessage>, Vec<A2aArtifact>) {
    let count = results.len();
    let artifact = A2aArtifact {
        name: Some("query_results".to_string()),
        description: Some(format!("{count} launcher results")),
        parts: vec![A2aPart::Data {
            data: query_results_artifact(results),
        }],
        index: 0,
    };
    let message = A2aMessage {
        role: "agent".to_string(),
        parts: vec![A2aPart::Text {
            text: format!("Found {count} result(s)"),
        }],
    };
    (vec![message], vec![artifact])
}

fn extract_tool_args(request: &MessageSendRequest) -> Value {
    request
        .messages
        .first()
        .and_then(|message| message.parts.first())
        .map(|part| match part {
            A2aPart::Data { data } => data.clone(),
            A2aPart::Text { text } => json!({ "input": text }),
        })
        .unwrap_or_else(|| json!({}))
}

fn extract_query_text(request: &MessageSendRequest) -> String {
    request
        .messages
        .first()
        .and_then(|message| message.parts.first())
        .map(|part| match part {
            A2aPart::Data { data } => data
                .get("query")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            A2aPart::Text { text } => text.clone(),
        })
        .unwrap_or_default()
}
```

- [ ] **Step 2: Update `execute_direct_tool` in `adapter.rs`**

Replace its body with:

```rust
    let pm = state.plugin_manager.lock().await;
    capabilities::execute_capability(&pm, tool_name, request).await
```

Ensure the imports include `capabilities`:

```rust
use super::{capabilities, tasks::TaskRegistry, types::*};
```

- [ ] **Step 3: Remove duplicate local `extract_tool_args` if unused**

If `adapter.rs` local `extract_tool_args` becomes unused and Rust warns or fails, delete the local function and its tests, or keep it only if other code still uses it. Prefer moving its tests to `capabilities.rs` if needed.

- [ ] **Step 4: Run the query execution test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml a2a::adapter::tests::message_send_invokes_query_only_capability
```

Expected: PASS.

- [ ] **Step 5: Run all A2A tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml a2a::
```

Expected: PASS.

- [ ] **Step 6: Commit direct query execution**

```bash
git add src-tauri/src/a2a/capabilities.rs src-tauri/src/a2a/adapter.rs
git commit --author="James Zhu <zhujian0805@gmail.com>" -m "feat: execute A2A query capabilities"
```

Do not add `Co-Authored-By`.

---

### Task 5: Add skill metadata discovery through A2A server state

**Files:**
- Modify: `src-tauri/src/a2a/adapter.rs`
- Modify: `src-tauri/src/a2a/server.rs`

- [ ] **Step 1: Add a new Agent Card builder that accepts skills**

In `adapter.rs`, change `build_agent_card` to delegate to a new function:

```rust
pub fn build_agent_card(base_url: &str, pm: &PluginManager) -> AgentCard {
    build_agent_card_with_skills(base_url, pm, None)
}

pub fn build_agent_card_with_skills(
    base_url: &str,
    pm: &PluginManager,
    skill_manager: Option<&SkillManager>,
) -> AgentCard {
    let skills = build_capabilities(pm, skill_manager)
        .iter()
        .map(capability_to_agent_skill)
        .collect();

    AgentCard {
        name: "OmniLauncher".to_string(),
        description: "OmniLauncher desktop agent — launcher, AI chat, and developer tools".to_string(),
        url: base_url.to_string(),
        version: Some("0.1.0".to_string()),
        capabilities: AgentCapabilities {
            streaming: false,
            push_notifications: false,
            state_transition_history: false,
        },
        authentication: AgentAuthentication {
            schemes: vec!["bearer".to_string()],
        },
        skills,
        default_input_modes: vec!["text/plain".to_string()],
        default_output_modes: vec!["text/plain".to_string()],
    }
}
```

- [ ] **Step 2: Update A2A server discovery route to pass skills**

In `server.rs`, route `GET /.well-known/agent.json`, lock the skill manager as well:

```rust
let pm = state.adapter.plugin_manager.lock().await;
let skill_manager = state.adapter.skill_manager.lock().await;
let settings = state.adapter.settings.lock().await;
let base_url = a2a_base_url(&settings);
let card = adapter::build_agent_card_with_skills(&base_url, &pm, Some(&skill_manager));
json_response(&card)
```

- [ ] **Step 3: Add/update a discovery test for skills**

If direct construction of `SkillManager` with loaded skills is too filesystem-dependent, add a unit test for `build_capabilities(&pm, Some(&SkillManager::new()))` that asserts it does not remove plugin capabilities. Then rely on manual/runtime verification for non-empty installed skills.

Use this test:

```rust
#[test]
fn build_capabilities_with_empty_skill_manager_keeps_plugin_capabilities() {
    let mut pm = PluginManager::new();
    pm.register(Box::new(QueryOnlyPlugin));
    let skill_manager = SkillManager::new();

    let capabilities = capabilities::build_capabilities(&pm, Some(&skill_manager));

    assert!(capabilities
        .iter()
        .any(|cap| cap.id == "plugin:query:Query Only Test"));
}
```

- [ ] **Step 4: Run A2A tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml a2a::
```

Expected: PASS.

- [ ] **Step 5: Commit skill-aware discovery**

```bash
git add src-tauri/src/a2a/adapter.rs src-tauri/src/a2a/server.rs
git commit --author="James Zhu <zhujian0805@gmail.com>" -m "feat: include skills in A2A discovery"
```

Do not add `Co-Authored-By`.

---

### Task 6: Final verification and push

**Files:**
- No source changes expected.

- [ ] **Step 1: Run relevant Rust A2A tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml a2a::
```

Expected: all A2A tests pass.

- [ ] **Step 2: Run runtime A2A verification**

Use a running OmniLauncher backend with A2A enabled. Fetch the Agent Card:

```bash
curl -isS -H "Authorization: Bearer <A2A_TOKEN>" http://127.0.0.1:1423/.well-known/agent.json
```

Expected: `HTTP/1.1 200 OK`, JSON Agent Card, and skills containing ids with prefixes:

- `plugin:tool:`
- `plugin:query:`
- `launcher:query_all`
- `skill:` when installed skills are loaded

- [ ] **Step 3: Invoke a query capability over A2A**

```bash
curl -isS \
  -H "Authorization: Bearer <A2A_TOKEN>" \
  -H 'Content-Type: application/json' \
  -X POST \
  http://127.0.0.1:1423/message:send \
  -d '{"tool":"launcher:query_all","messages":[{"role":"user","parts":[{"type":"data","data":{"query":"calc"}}]}]}'
```

Expected: `HTTP/1.1 200 OK`, completed task, and a `query_results` artifact.

- [ ] **Step 4: Check git status**

```bash
git status --short --branch
```

Expected: clean working tree on the feature branch.

- [ ] **Step 5: Push branch**

```bash
git push
```

Expected: branch updates on origin.
