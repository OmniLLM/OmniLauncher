# A2A Agent Card Loaded Skills Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose every loaded OmniLauncher skill in the authenticated A2A Agent Card discovery response.

**Architecture:** Use the existing skill-aware `build_agent_card_with_skills(...)` path from the A2A discovery route. The discovery handler will lock the live `SkillManager` alongside the existing `PluginManager` and `AppSettings`, then serialize a card containing plugin-derived capabilities plus all loaded skill metadata.

**Tech Stack:** Rust, Tauri backend, Tokio async mutexes, serde JSON, existing A2A module tests.

---

## File Structure

- Modify: `src-tauri/src/a2a/server.rs`
  - Responsibility: HTTP request routing for the A2A TCP server, including authenticated `GET /.well-known/agent.json` discovery.
  - Change: Wire `SkillManager` into the Agent Card route and strengthen the route test.

No new files are required. The existing builder and capability aggregation code already support loaded skills.

## Task 1: Add a route-level failing test for loaded skills

**Files:**
- Modify: `src-tauri/src/a2a/server.rs:399-451`
- Test: `src-tauri/src/a2a/server.rs::tests::agent_card_route_requires_bearer_token_and_returns_card`

- [ ] **Step 1: Add a loaded test skill to the A2A server test state**

In `src-tauri/src/a2a/server.rs`, replace the current `test_server_state()` helper in the `#[cfg(test)] mod tests` block with this version. This keeps the same built-in plugin manager and auth token, but loads a temporary `SKILL.md` into `SkillManager` before constructing state.

```rust
    fn test_skill_manager() -> SkillManager {
        let skill_root = tempfile::tempdir().unwrap();
        let skill_dir = skill_root.path().join("route-demo-skill");
        std::fs::create_dir(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: route-demo-skill
description: Route demo skill for A2A discovery
tags: route, a2a
---

# Route Demo Skill
"#,
        )
        .unwrap();

        let mut skill_manager = SkillManager::new();
        skill_manager.load_from_dir(skill_root.path());
        skill_manager
    }

    fn test_server_state() -> A2aServerState {
        let mut settings = AppSettings::default();
        settings.a2a_port = 18123;

        A2aServerState {
            adapter: A2aAdapterState {
                plugin_manager: Arc::new(Mutex::new(create_plugin_manager_builtin_only())),
                ai_client: Arc::new(Mutex::new(AiClient::new(
                    String::new(),
                    String::new(),
                    String::new(),
                ))),
                settings: Arc::new(Mutex::new(settings)),
                conversation: Arc::new(Mutex::new(ConversationContext::new(10))),
                skill_manager: Arc::new(Mutex::new(test_skill_manager())),
                task_registry: Arc::new(Mutex::new(TaskRegistry::new(10))),
            },
            auth_token: Arc::new("test-token".to_string()),
        }
    }
```

- [ ] **Step 2: Strengthen the route test assertions**

In the same file, replace the final assertion in `agent_card_route_requires_bearer_token_and_returns_card()`:

```rust
        assert!(!card.skills.is_empty());
```

with these assertions:

```rust
        assert!(
            card.skills
                .iter()
                .any(|skill| skill.id == "plugin:tool:app_launcher"),
            "expected plugin-derived capabilities to remain exposed"
        );

        let route_skill = card
            .skills
            .iter()
            .find(|skill| skill.id == "skill:route-demo-skill")
            .expect("loaded skill should be exposed by the discovery route");
        assert_eq!(route_skill.name, "route-demo-skill");
        assert_eq!(
            route_skill.description.as_deref(),
            Some("Route demo skill for A2A discovery")
        );
        assert!(route_skill.tags.iter().any(|tag| tag == "route"));
        assert!(route_skill.input_schema.is_some());
```

- [ ] **Step 3: Run the route test and verify it fails**

Run:

```bash
cd src-tauri && cargo test a2a::server::tests::agent_card_route_requires_bearer_token_and_returns_card --lib
```

Expected result before implementation:

```text
thread 'a2a::server::tests::agent_card_route_requires_bearer_token_and_returns_card' panicked ... loaded skill should be exposed by the discovery route
```

The plugin-derived capability assertion should still pass. The `skill:route-demo-skill` assertion should fail because the route still calls `adapter::build_agent_card(&base_url, &pm)` without passing `SkillManager`.

## Task 2: Wire SkillManager into A2A discovery

**Files:**
- Modify: `src-tauri/src/a2a/server.rs:110-115`
- Test: `src-tauri/src/a2a/server.rs::tests::agent_card_route_requires_bearer_token_and_returns_card`

- [ ] **Step 1: Update the discovery route implementation**

In `src-tauri/src/a2a/server.rs`, replace the discovery branch body:

```rust
        ("GET", "/.well-known/agent.json") => {
            let pm = state.adapter.plugin_manager.lock().await;
            let settings = state.adapter.settings.lock().await;
            let base_url = a2a_base_url(&settings);
            let card = adapter::build_agent_card(&base_url, &pm);
            json_response(&card)
        }
```

with this implementation:

```rust
        ("GET", "/.well-known/agent.json") => {
            let pm = state.adapter.plugin_manager.lock().await;
            let settings = state.adapter.settings.lock().await;
            let base_url = a2a_base_url(&settings);
            let skills = state.adapter.skill_manager.lock().await;
            let card = adapter::build_agent_card_with_skills(&base_url, &pm, Some(&skills));
            json_response(&card)
        }
```

This is the entire feature implementation. `build_agent_card_with_skills(...)` already delegates to `build_capabilities(pm, Some(skill_manager))`, and `build_capabilities(...)` already adds every `SkillManager::list_meta()` entry as `skill:<name>`.

- [ ] **Step 2: Run the focused route test and verify it passes**

Run:

```bash
cd src-tauri && cargo test a2a::server::tests::agent_card_route_requires_bearer_token_and_returns_card --lib
```

Expected result:

```text
test a2a::server::tests::agent_card_route_requires_bearer_token_and_returns_card ... ok
```

- [ ] **Step 3: Run related A2A tests**

Run:

```bash
cd src-tauri && cargo test a2a:: --lib
```

Expected result:

```text
test result: ok
```

If unrelated tests fail, capture the failure output and do not hide it.

## Task 3: Final verification and cleanup

**Files:**
- Review: `src-tauri/src/a2a/server.rs`
- Review: `docs/superpowers/specs/2026-06-28-a2a-agent-card-skills-design.md`
- Review: `docs/superpowers/plans/2026-06-28-a2a-agent-card-skills.md`

- [ ] **Step 1: Check the working tree diff**

Run:

```bash
git diff -- src-tauri/src/a2a/server.rs docs/superpowers/specs/2026-06-28-a2a-agent-card-skills-design.md docs/superpowers/plans/2026-06-28-a2a-agent-card-skills.md
```

Expected result:

```text
The diff shows only:
- the route now calls build_agent_card_with_skills(..., Some(&skills))
- the route test seeds and asserts a loaded skill
- the design and implementation plan documents
```

- [ ] **Step 2: Run formatting for the Rust crate**

Run:

```bash
cd src-tauri && cargo fmt --check
```

Expected result:

```text
No output and exit code 0
```

If formatting fails, run:

```bash
cd src-tauri && cargo fmt
```

Then run the check again:

```bash
cd src-tauri && cargo fmt --check
```

Expected result after formatting:

```text
No output and exit code 0
```

- [ ] **Step 3: Run the final focused test set**

Run:

```bash
cd src-tauri && cargo test a2a:: --lib
```

Expected result:

```text
test result: ok
```

- [ ] **Step 4: Report completion**

Summarize:

```text
Implemented A2A Agent Card loaded skill discovery.

Changed:
- src-tauri/src/a2a/server.rs now passes SkillManager into build_agent_card_with_skills for /.well-known/agent.json.
- The route test now loads route-demo-skill and asserts skill:route-demo-skill appears in AgentCard.skills.

Verified:
- cargo fmt --check
- cargo test a2a:: --lib
```

Do not claim verification passed unless the commands actually passed.

---

## Self-Review

- Spec coverage: The plan covers authenticated discovery, all loaded `SkillManager` metadata, preservation of plugin-derived capabilities, no schema changes, and route-level tests.
- Placeholder scan: No placeholders remain. Every code edit includes concrete replacement code and every verification step includes commands and expected outcomes.
- Type consistency: The plan uses existing types and functions: `SkillManager`, `A2aServerState`, `A2aAdapterState`, `build_agent_card_with_skills`, and `AgentCard.skills`. The test skill id is consistently `skill:route-demo-skill`.