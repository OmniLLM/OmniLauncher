# Provider Picker & Model Selection

Date: 2026-07-09
Status: Approved

## Problem

OmniLauncher has exactly one AI provider, stored as three flat fields on
`AppSettings`:

    src-tauri/src/settings.rs:98-100
    pub ai_base_url: String,
    pub ai_model: String,
    pub ai_api_key: String,

`AiClient` always POSTs to `{base_url}/v1/chat/completions` with an
optional `Authorization: Bearer {api_key}` (src-tauri/src/ai/client.rs).
There is no notion of a provider *kind*, no way to save more than one
provider, and no authentication mechanism other than a static bearer
token. A user who wants to use GitHub Copilot (OAuth), an Azure AI
Foundry endpoint, and a local vLLM must reconfigure the single provider
by hand each time.

The sibling project **omni-pilot** (`~/repos/omni-pilot`, a browser
extension in the same OmniLLM ecosystem) already solves this with a
modular provider picker supporting GitHub Copilot, Custom Provider, and
Azure Foundry. This design ports omni-pilot's proven approach to
OmniLauncher: **configure any number of providers, then select a model
from that provider's list.**

## Goals

- Replace the single flat provider with a **list of saved providers**
  plus an **active provider** pointer, stored in `AppSettings`.
- Support three provider **kinds**, matching omni-pilot:
  - **Custom** — any OpenAI-compatible endpoint; auto-lists models via
    `GET {base}/v1/models` (works today).
  - **Azure Foundry** — an OpenAI-compatible Foundry endpoint; models
    are **entered manually** (no clean list endpoint).
  - **GitHub Copilot** — OAuth device-flow sign-in; auto-lists models
    via the Copilot `/models` endpoint.
- Port omni-pilot's **GitHub Copilot device flow**: request a device
  code, show the user a code + verification URL, poll for the GitHub
  OAuth token, then exchange it for a short-lived Copilot API token
  (with expiry + refresh).
- Manage providers in the **Settings window**, and add a compact
  **active-provider + model quick-switch** control to the AI top bar.
- **Zero-loss migration**: an existing install with `ai_base_url` set
  upgrades silently to one Custom provider.

## Non-goals

- **Multiple API wire formats.** omni-pilot also supports Anthropic
  Messages and OpenAI Responses shapes. OmniLauncher stays
  **OpenAI-compatible chat-completions only**. The entire agentic tool
  loop in `src-tauri/src/ai/router.rs` is built on OpenAI-style
  `tool_calls`; adding alternate shapes is a much larger, separate
  effort not required to "configure a provider and pick a model."
  Copilot's own `/responses`-only models are therefore out of scope —
  Copilot is driven through `/chat/completions` like everything else.
- **A dedicated Azure OpenAI deployment adapter.** An earlier draft
  proposed building classic Azure URLs from
  `resourceName + deployment + api-version` with an `api-key` header.
  omni-pilot instead treats Azure Foundry as a plain OpenAI-compatible
  endpoint authenticated with `Authorization: Bearer {key}`, and that
  is what we follow. The only Azure-specific behavior is **manual model
  entry**.
- **Copilot per-model shape routing / parameter quirks.** omni-pilot's
  `copilot-model-shapes` work (choosing `max_completion_tokens` vs
  `max_tokens`, dropping `temperature` for reasoning models, retrying on
  `/responses`) is tied to its multi-shape support and is out of scope
  here. We send standard chat-completions bodies.
- **Streaming for Copilot beyond what exists.** `AiClient::chat_stream`
  keeps its current behavior; Copilot uses the same non-shape-aware
  path as other providers.
- **Cross-device sync / secret vault.** Provider secrets live in the
  existing settings JSON alongside today's `ai_api_key`, with the same
  protections (see `crate::log_masking`). No new secret store.

## Design Overview

The change has three layers:

1. **Data model** (`settings.rs`) — a `Provider` struct + `ProviderKind`
   enum, a registry of per-kind capability flags, `providers: Vec<Provider>`
   and `active_provider_id: String` on `AppSettings`, and a migration
   from the legacy `ai_*` fields.
2. **Request resolution** (`ai/` + new `ai/copilot.rs`) — a
   `resolve_request(provider)` helper that returns the concrete
   `{ url, headers }` for a provider, a light `AiClient` refactor to
   accept those instead of hardcoding URL + bearer, and a Copilot module
   porting omni-pilot's device flow, token exchange, headers, and model
   listing.
3. **Frontend** (`SettingsWindow.tsx`, `AiTopBar.tsx`) — a provider list
   manager with kind-driven field visibility and a Copilot login panel,
   plus a quick-switch dropdown in the top bar.

The organizing idea, borrowed directly from omni-pilot, is a **provider
registry of capability flags** that both the backend (request routing,
model listing) and the frontend (which fields to show) read from, so
adding a kind is a data change rather than scattered conditionals.

## Data Model

### `ProviderKind` and the registry

```rust
// settings.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    Custom,        // "custom-provider"
    GithubCopilot, // "github-copilot"
    AzureFoundry,  // "azure-foundry"
}

pub struct ProviderCaps {
    pub uses_copilot_auth: bool,   // Copilot two-step token + editor headers
    pub requires_api_key: bool,    // Custom + Azure
    pub auto_list_models: bool,    // Custom (via /v1/models) + Copilot (via /models)
    pub manual_models: bool,       // Azure only
}

pub fn caps(kind: ProviderKind) -> ProviderCaps { /* match kind { … } */ }
```

Mirrors omni-pilot's `PROVIDERS` map
(`src/background/index.mjs:22-39`): Custom `{copilot:false, key:true,
autolist:true}`, Copilot `{copilot:true, key:false, autolist:true}`,
Azure `{copilot:false, key:true, manual:true}`.

### `Provider`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,               // stable, generated on create
    pub name: String,             // "Work Copilot"
    pub kind: ProviderKind,
    #[serde(default)] pub base_url: String,   // custom/azure
    #[serde(default)] pub api_key: String,    // custom/azure
    #[serde(default)] pub model: String,      // selected model / deployment
    #[serde(default)] pub models: Vec<String>,// azure manual list

    // Copilot-only (populated by the device flow; omni-pilot storage keys):
    #[serde(default)] pub copilot_github_token: String,  // long-lived GitHub OAuth token
    #[serde(default)] pub copilot_token: String,         // short-lived Copilot API token
    #[serde(default)] pub copilot_token_expiry: i64,     // ms epoch
    #[serde(default)] pub copilot_enterprise_url: String,// optional GHE domain
}
```

### `AppSettings` additions

```rust
#[serde(default)] pub providers: Vec<Provider>,
#[serde(default)] pub active_provider_id: String,
// existing ai_base_url / ai_model / ai_api_key retained (see Migration)
```

The `id` is generated without wall-clock or RNG dependence being a
correctness requirement — a counter (`provider-1`, `provider-2`, …
based on existing ids) or a hash of `name + kind` is sufficient and
keeps tests deterministic.

### `active_provider()` accessor

```rust
impl AppSettings {
    pub fn active_provider(&self) -> Option<&Provider> {
        self.providers.iter().find(|p| p.id == self.active_provider_id)
            .or_else(|| self.providers.first())
    }
}
```

All backend call sites resolve the live provider through this, replacing
direct reads of `ai_base_url` / `ai_model` / `resolve_ai_api_key()`.

## Migration

On `load_settings()` (settings.rs:630), after deserialization:

- If `providers` is non-empty, use as-is.
- Else, synthesize one provider from the legacy fields:
  ```rust
  Provider {
      id: "default".into(),
      name: "Default".into(),
      kind: ProviderKind::Custom,
      base_url: self.ai_base_url.clone(),
      api_key: self.ai_api_key.clone(),
      model: self.ai_model.clone(),
      ..Default::default()
  }
  ```
  and set `active_provider_id = "default"`.

The legacy `ai_*` fields are **kept** on the struct (not removed) so
older config files deserialize and the factory-default detection
(settings.rs:976-988, `is_factory_default`) keeps working. New writes
populate `providers`; the `ai_*` fields become a compatibility shim.
`writes` of settings continue through the existing POST /api/settings
guard in server.rs — that guard's factory-default comparison is extended
to also treat an empty `providers` list as unconfigured.

## Request Resolution

### `resolve_request(provider) -> ResolvedRequest`

New helper (in `ai/mod.rs` or a small `ai/provider.rs`):

```rust
pub struct ResolvedRequest {
    pub chat_url: String,            // full URL incl. /chat/completions
    pub headers: Vec<(String, String)>, // auth + any kind-specific headers
    pub model: String,
}
```

Resolution table (ported from omni-pilot `buildApiRequest`
index.mjs:1220-1301 and `createAuthHeaders` index.mjs:1069):

| kind | chat_url | headers |
|---|---|---|
| Custom | `{base}/v1/chat/completions` | `Authorization: Bearer {key}` (omit if empty) |
| Azure Foundry | `{base}/chat/completions` | `Authorization: Bearer {key}` |
| GitHub Copilot | `{copilot_base}/chat/completions` | `Bearer {copilot_token}` + editor headers |

`copilot_base` = `https://api.githubcopilot.com` (or
`https://copilot-api.{enterprise_domain}` when
`copilot_enterprise_url` is set — omni-pilot uses the fixed public base;
enterprise base derivation follows opencode's `base()` and is optional).

Note Custom appends `/v1/chat/completions` while Azure appends
`/chat/completions`. This is deliberate and asymmetric:

- **Custom preserves OmniLauncher's current behavior.** Today both
  `AiClient` (client.rs) and `list_models_backend` (server.rs:2291)
  hardcode a `/v1/...` suffix, so an existing user's
  `http://127.0.0.1:5000` base keeps working with **zero change**. The
  Custom kind keeps that `/v1` suffix; its model listing keeps hitting
  `{base}/v1/models`.
- **Azure follows omni-pilot**, which appends `/chat/completions` (no
  `/v1`) to a Foundry endpoint pasted *with* its own version/path
  segment (index.mjs:1291). Azure has no auto model listing, so there is
  no `/models` suffix to reconcile.

If this asymmetry proves confusing in practice, a later refinement can
let Custom endpoints opt out of the `/v1` prefix, but that is not
required here and would risk breaking existing configs.

### `AiClient` refactor (light)

`AiClient` currently stores `base_url` + `api_key` + `model` and builds
`{base}/v1/chat/completions` with `bearer_auth` internally
(client.rs:262-273, 306-309). Change it to be told the **resolved URL and
headers**:

- Add a constructor / setter path that accepts
  `chat_url: String` and `headers: Vec<(String,String)>` instead of
  `base_url` + `api_key`. `model` stays.
- In `chat_with_tools_once` and `chat_stream`, POST to `chat_url` and
  apply `headers` (replacing the `format!("{}/v1/chat/completions", …)`
  and `bearer_auth` lines). Retry/timeout logic is untouched.
- Keep the existing `new`/`with_timeout`/`with_retry` builders working
  for tests by having them construct a Custom-style resolved request
  internally, so `client_tests` and the ~15 construction sites in
  main.rs / server.rs / a2a compile with minimal edits — each site
  changes from "pass base_url+key" to "resolve active provider, pass
  url+headers".

### `ai/copilot.rs` (new module)

Direct port of omni-pilot `src/background/index.mjs` Copilot functions:

```rust
pub struct CopilotConfig; // constants below
// CLIENT_ID = "Iv1.b507a08c87ecfe98"
// DEVICE_CODE_URL = "https://github.com/login/device/code"
// ACCESS_TOKEN_URL = "https://github.com/login/oauth/access_token"
// COPILOT_API_KEY_URL = "https://api.github.com/copilot_internal/v2/token"
// COPILOT_API_BASE_URL = "https://api.githubcopilot.com"
// SCOPES = "read:user"
// USER_AGENT = "GitHubCopilotChat/0.26.7"
// EDITOR_VERSION = "vscode/1.83.1"
// EDITOR_PLUGIN_VERSION = "copilot-chat/0.26.7"
// API_VERSION = "2025-04-01"

pub struct DeviceFlow { pub device_code, user_code, verification_uri, interval, expires_in }

pub async fn start_device_flow() -> Result<DeviceFlow, String>;
pub async fn poll_token(device_code: &str) -> PollResult; // Pending | SlowDown | Success(github_token) | Failed
pub async fn ensure_access_token(provider: &mut Provider) -> Result<String, String>;
    // returns cached copilot_token if unexpired; else exchanges
    // copilot_github_token at COPILOT_API_KEY_URL, stores token+expiry
pub fn copilot_headers(token: &str) -> Vec<(String, String)>;
pub async fn fetch_models(token: &str) -> Result<Vec<String>, String>;
```

Headers (omni-pilot index.mjs:1934-1947):
`Authorization: Bearer {token}`, `copilot-integration-id: vscode-chat`,
`Editor-Version`, `Editor-Plugin-Version`, `User-Agent`,
`OpenAI-Intent: conversation-panel`, `X-Github-Api-Version`,
`X-Vscode-User-Agent-Library-Version: electron-fetch`.

The **two-step** nature is the key detail (index.mjs:2023-2059): the
device flow yields a long-lived GitHub OAuth token
(`copilot_github_token`); each request needs a short-lived Copilot API
token obtained by GETting `COPILOT_API_KEY_URL` with
`Authorization: token {github_token}`. `ensure_access_token` caches it
until `copilot_token_expiry`.

## Tauri Commands

Register in main.rs alongside the existing `list_models` (main.rs:639,
1329) and mirror in server.rs for the HTTP path:

- `list_providers() -> Vec<Provider>` (secrets masked for display where
  appropriate).
- `save_provider(provider: Provider)` — upsert by `id`.
- `delete_provider(id: String)`.
- `set_active_provider(id: String)` — rebuilds the live `AiClient`
  (the RwLock swap already done at main.rs:1482, server.rs:489).
- `copilot_start_device_flow(provider_id) -> DeviceFlow`.
- `copilot_poll(provider_id) -> PollStatus` — frontend polls on the
  interval; on success stores `copilot_github_token`.
- `copilot_logout(provider_id)` — clears Copilot tokens.
- Extend `list_models` to be **provider-aware**: given a provider id (or
  the active provider), dispatch on `caps(kind)`:
  - `auto_list_models` + Copilot → `copilot::fetch_models`.
  - `auto_list_models` + Custom → existing `list_models_backend`
    (server.rs:2285).
  - `manual_models` → return `provider.models`.

## Frontend

### SettingsWindow.tsx

The AI tab (currently the single "AI Provider" block at ~line 427)
becomes a **provider manager**:

- A list of configured providers with add / edit / delete and a
  "set active" affordance.
- A **kind** selector (Custom / GitHub Copilot / Azure Foundry) that
  toggles field visibility, porting omni-pilot's
  `updateProviderTypeUI` flags (options/index.mjs:828-848):
  - Custom: base URL, API key, model picker (auto-list).
  - Azure Foundry: base URL, API key, **manual models** textarea +
    model picker over that list.
  - GitHub Copilot: **Login** panel (no URL/key), model picker
    (auto-list once authorized).
- The existing model-picker dropdown (SettingsWindow.tsx:97-188,
  `fetchModels` → `list_models`) is reused per-provider; its source is
  auto vs manual per kind.
- **Copilot login panel**: a "Sign in to GitHub Copilot" button calls
  `copilot_start_device_flow`, displays `user_code` + a link to
  `verification_uri`, then polls `copilot_poll` on `interval` until
  success/expiry, then enables the model picker. A "Sign out" button
  calls `copilot_logout`.

### AiTopBar.tsx

Currently a thin wrapper over `SessionPicker` (AiTopBar.tsx, 29 lines).
Add a compact **active-provider + model** control:

- Reads `list_providers`; shows active provider name + current model.
- Selecting a different provider calls `set_active_provider`; selecting
  a model updates that provider's `model` via `save_provider`.
- Full management (add/login/delete) still lives in Settings; the top
  bar only switches among already-configured providers.

## Testing

- **Registry** (`caps`): each kind returns the expected flags.
- **Migration**: legacy `{ai_base_url, ai_model, ai_api_key}` with empty
  `providers` yields one Custom provider marked active; an existing
  `providers` list is untouched; factory-default detection still holds.
- **`resolve_request`**: Custom → `{base}/v1/chat/completions` + bearer;
  Azure → `{base}/chat/completions` + bearer; Copilot →
  `api.githubcopilot.com/chat/completions` + editor headers, no
  `Authorization: Bearer {api_key}` from the key field.
- **Copilot** (mocked fetch): device-flow start parses
  code/URL/interval; `poll_token` maps `authorization_pending` → Pending,
  `slow_down` → SlowDown(+5s), `access_token` → Success;
  `ensure_access_token` exchanges the GitHub token, caches until expiry,
  and refreshes after expiry; `fetch_models` maps `data[].id` → sorted
  list.
- **Provider-aware `list_models`**: Custom hits `/v1/models`, Azure
  returns the manual list, Copilot calls the Copilot models endpoint.
- **Frontend** (vitest/RTL, matching existing SettingsWindow.test.tsx):
  kind selector toggles field visibility; manual-models textarea drives
  the picker for Azure; Copilot login panel shows the user code and
  transitions on poll success.

## References

- omni-pilot provider registry & request routing:
  `~/repos/omni-pilot/src/background/index.mjs`
  — `PROVIDER_TYPES`/`PROVIDERS` (10-39), `COPILOT_CONFIG` (41-52),
  `buildApiRequest` (1220-1301), `createAuthHeaders` (1069-1080),
  `createCopilotHeaders` (1934-1947), device flow (1949-2063),
  `fetchCopilotModels`/`handleGetModels` (2065-2104).
- omni-pilot options UI field visibility:
  `~/repos/omni-pilot/src/options/index.mjs` — `PROVIDERS`/
  `updateProviderTypeUI` (9-30, 828-848).
- omni-pilot plans/specs:
  `~/repos/omni-pilot/docs/superpowers/plans/2026-06-24-modular-provider-picker.md`,
  `~/repos/omni-pilot/docs/superpowers/specs/2026-07-07-copilot-model-handling-design.md`.
- OmniLauncher current code: `src-tauri/src/settings.rs` (98-115,
  589-700, 976-988), `src-tauri/src/ai/client.rs`,
  `src-tauri/src/server.rs` (2285 `list_models_backend`),
  `src/components/SettingsWindow.tsx` (97-188, 427+),
  `src/components/AiTopBar.tsx`.
