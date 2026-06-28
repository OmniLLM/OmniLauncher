# A2A Agent Card Loaded Skills Design

## Goal

Expose every loaded OmniLauncher skill in the authenticated A2A Agent Card discovery response, in addition to the existing plugin-derived capabilities.

The discovery endpoint is `GET /.well-known/agent.json`. Clients should be able to discover installed skills through the card's `skills` array as `skill:<name>` entries.

## Scope

In scope:

- Include all currently loaded `SkillManager` metadata in the A2A Agent Card.
- Preserve the existing plugin tool, query plugin, and launcher capabilities.
- Keep the current authenticated discovery behavior.
- Add or strengthen Rust tests for the discovery route.

Out of scope:

- Filtering archived or unpinned skills.
- Adding new skill execution semantics.
- Changing the A2A wire schema.
- Persisting discovery output.

## Current Behavior

`A2aAdapterState` already carries a live `SkillManager`, and `build_agent_card_with_skills(...)` already knows how to add loaded skills to the card.

The live discovery route currently calls the plugin-only wrapper:

```rust
let card = adapter::build_agent_card(&base_url, &pm);
```

That means clients discover plugin-derived capabilities but do not receive the loaded skills from `SkillManager`.

## Design

Use the existing skill-aware Agent Card builder in the discovery route.

The route will:

1. Authenticate the request as it does today.
2. Lock `PluginManager`.
3. Lock `AppSettings` and compute the base URL.
4. Lock `SkillManager`.
5. Call `adapter::build_agent_card_with_skills(&base_url, &pm, Some(&skills))`.
6. Serialize the resulting card as JSON.

No new data structures are required. `build_capabilities(...)` already maps loaded skill metadata to A2A skills with:

- `id`: `skill:<name>`
- `name`: skill name
- `description`: skill description
- `input_schema`: the existing skill input schema
- `tags`: skill tags
- `kind`: `A2aCapabilityKind::Skill`
- `target`: skill name

## Data Flow

```text
client
  -> GET /.well-known/agent.json with bearer token
  -> A2A auth guard
  -> lock PluginManager
  -> lock AppSettings
  -> lock SkillManager
  -> build_agent_card_with_skills(base_url, pm, Some(skills))
  -> build_capabilities(plugin tools + query plugins + launcher + all loaded skills)
  -> AgentCard.skills
  -> JSON response
```

## Error Handling

No new error surface is introduced.

Skill discovery uses in-memory metadata via `SkillManager::list_meta()`, so the route does not perform skill filesystem I/O. If no skills are loaded, discovery still returns the existing plugin-derived capabilities.

## Testing

Strengthen the A2A discovery route test to verify the route-level behavior, not just the builder:

- Unauthorized discovery still returns `401 Unauthorized`.
- Authorized discovery still returns `200 OK`.
- The response still advertises bearer authentication and the expected base URL.
- A loaded test skill appears in `AgentCard.skills` with id `skill:<test-name>`.
- Existing plugin-derived capabilities remain present.

## Implementation Approach

Use the minimal wiring approach:

- Change `src-tauri/src/a2a/server.rs` discovery branch to lock `state.adapter.skill_manager` and call `build_agent_card_with_skills(...)`.
- Update the existing route test in the same file to seed a loaded test skill and assert the resulting card contains the `skill:*` entry.

This keeps the change focused and reuses the existing capability aggregation code.