//! A2A protocol wire types, conforming to **A2A Protocol v1.0**.
//!
//! Two spec rules drive the unusual serde code in this module:
//!
//! * **§A.2.1** — the `kind` discriminator was removed in v1.0. Polymorphic
//!   objects use the *JSON member name itself* as the discriminator, matching
//!   Protocol Buffers `oneof` semantics. A text part is `{"text": "..."}`, not
//!   `{"kind": "text", "text": "..."}`.
//! * **§5.5** — enums serialize per ProtoJSON, i.e. their protobuf names in
//!   SCREAMING_SNAKE_CASE (`TASK_STATE_COMPLETED`, `ROLE_USER`).
//!
//! Per §A.2 ("Servers MAY accept both legacy and current request message forms
//! during the overlap period. Emit only current form in responses"), everything
//! here **serializes strictly as v1.0** but **deserializes tolerantly**,
//! accepting the v0.3.x `kind` form, this crate's older `type` form, and
//! lowercase enum names.

use serde::de::{Error as DeError, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::fmt;

/// The A2A protocol version this implementation speaks.
pub const A2A_PROTOCOL_VERSION: &str = "1.0";

// ── Agent Card ──────────────────────────────────────────────────────────────

/// Top-level Agent Card returned by `GET /.well-known/agent-card.json` (and
/// its legacy alias `GET /.well-known/agent.json`, retained for older
/// clients). Shape follows the v1.0 proto `AgentCard`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    /// Ordered list of supported interfaces; the first entry is preferred.
    /// Replaces the pre-1.0 top-level `url` field.
    pub supported_interfaces: Vec<AgentInterface>,
    /// The version of *this agent* (not the protocol).
    pub version: String,
    pub capabilities: AgentCapabilities,
    /// Named security schemes, keyed by scheme name.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub security_schemes: std::collections::BTreeMap<String, SecurityScheme>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security_requirements: Vec<SecurityRequirement>,
    /// Default input modes accepted by this agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_input_modes: Vec<String>,
    /// Default output modes produced by this agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_output_modes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<AgentSkill>,
}

/// One transport endpoint advertised by the agent (proto `AgentInterface`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInterface {
    pub url: String,
    /// One of `JSONRPC`, `GRPC`, `HTTP+JSON`.
    pub protocol_binding: String,
    /// A2A protocol version exposed at this URL, e.g. "1.0".
    pub protocol_version: String,
}

/// Optional capabilities (proto `AgentCapabilities`).
///
/// Per **§A.2.2**, `extendedAgentCard` lives here — it was relocated from the
/// pre-1.0 top-level `AgentCard.supportsExtendedAgentCard` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    pub streaming: bool,
    pub push_notifications: bool,
    pub extended_agent_card: bool,
}

/// A `SecurityScheme` is a proto `oneof`; per §A.2.1 the member name is the
/// discriminator. We only ever emit the HTTP-auth variant (bearer tokens).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityScheme {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_auth_security_scheme: Option<HttpAuthSecurityScheme>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpAuthSecurityScheme {
    /// HTTP auth scheme name per RFC 7235, e.g. "Bearer".
    pub scheme: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A security requirement: a map of scheme name → required scope list.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityRequirement {
    pub schemes: std::collections::BTreeMap<String, StringList>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StringList {
    #[serde(default)]
    pub list: Vec<String>,
}

/// A single capability/skill advertised in the Agent Card.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    /// Required by the v1.0 proto.
    pub description: String,
    /// Required by the v1.0 proto (may be empty).
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_modes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_modes: Vec<String>,
}

// ── Role ────────────────────────────────────────────────────────────────────

/// Message sender (proto `Role`). Serializes per ProtoJSON (§5.5) as
/// `ROLE_USER` / `ROLE_AGENT`; deserializes those plus the pre-1.0 lowercase
/// spellings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A2aRole {
    User,
    Agent,
}

impl A2aRole {
    pub fn as_protojson(self) -> &'static str {
        match self {
            Self::User => "ROLE_USER",
            Self::Agent => "ROLE_AGENT",
        }
    }

    /// True when this role denotes an agent/assistant turn.
    pub fn is_agent(self) -> bool {
        matches!(self, Self::Agent)
    }
}

impl Serialize for A2aRole {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_protojson())
    }
}

impl<'de> Deserialize<'de> for A2aRole {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        match raw.as_str() {
            // v1.0 ProtoJSON.
            "ROLE_USER" => Ok(Self::User),
            "ROLE_AGENT" => Ok(Self::Agent),
            // Legacy pre-1.0 spellings, accepted per §A.2.
            "user" => Ok(Self::User),
            "agent" | "assistant" => Ok(Self::Agent),
            // Unknown roles are treated as user turns rather than hard errors:
            // an unrecognized sender is far more likely to be a client than
            // this agent talking to itself.
            _ => Ok(Self::User),
        }
    }
}

// ── Messages & Parts ────────────────────────────────────────────────────────

/// A message within an A2A conversation (proto `Message`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct A2aMessage {
    /// Unique message identifier, created by the message author. Required by
    /// the v1.0 proto; defaulted on input so pre-1.0 clients still parse.
    #[serde(default = "crate::a2a::tasks::generate_task_id")]
    pub message_id: String,
    pub role: A2aRole,
    pub parts: Vec<A2aPart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl A2aMessage {
    /// Build an agent-authored message carrying a single text part.
    pub fn agent_text(text: impl Into<String>) -> Self {
        Self::new(A2aRole::Agent, vec![A2aPart::text(text)])
    }

    /// Build a message with a freshly generated `messageId`.
    pub fn new(role: A2aRole, parts: Vec<A2aPart>) -> Self {
        Self {
            message_id: crate::a2a::tasks::generate_task_id(),
            role,
            parts,
            context_id: None,
            task_id: None,
            metadata: None,
        }
    }

    /// Concatenate all text parts of this message, newline-separated.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for part in &self.parts {
            if let A2aPartContent::Text(text) = &part.content {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(text);
            }
        }
        out.trim().to_string()
    }
}

/// The `oneof content` of a [`A2aPart`] (proto `Part`).
///
/// Per **§A.2.1** the variant is identified purely by which JSON member is
/// present — there is no `kind` (v0.3.x) or `type` (this crate, pre-1.0) tag.
#[derive(Debug, Clone, PartialEq)]
pub enum A2aPartContent {
    /// `{"text": "..."}`
    Text(String),
    /// `{"raw": "<base64>"}` — inline file bytes.
    Raw(String),
    /// `{"url": "https://..."}` — file by reference.
    Url(String),
    /// `{"data": {...}}` — arbitrary structured JSON.
    Data(Value),
}

/// A single part of a message or artifact (proto `Part`).
#[derive(Debug, Clone, PartialEq)]
pub struct A2aPart {
    pub content: A2aPartContent,
    /// Optional filename, e.g. "document.pdf". Applies to `raw`/`url` parts.
    pub filename: Option<String>,
    /// Media (MIME) type of the content. Valid for every part variant.
    pub media_type: Option<String>,
    pub metadata: Option<Value>,
}

impl A2aPart {
    /// A plain text part: `{"text": "..."}`.
    pub fn text(text: impl Into<String>) -> Self {
        Self::from_content(A2aPartContent::Text(text.into()))
    }

    /// A structured data part. The spec's example carries an explicit
    /// `mediaType`, so we set `application/json`.
    pub fn data(data: Value) -> Self {
        Self {
            media_type: Some("application/json".to_string()),
            ..Self::from_content(A2aPartContent::Data(data))
        }
    }

    /// An inline file part: base64 `raw` bytes plus filename/media type.
    pub fn raw_file(
        base64: impl Into<String>,
        filename: Option<String>,
        media_type: Option<String>,
    ) -> Self {
        Self {
            filename,
            media_type,
            ..Self::from_content(A2aPartContent::Raw(base64.into()))
        }
    }

    /// A file-by-reference part.
    pub fn url_file(
        url: impl Into<String>,
        filename: Option<String>,
        media_type: Option<String>,
    ) -> Self {
        Self {
            filename,
            media_type,
            ..Self::from_content(A2aPartContent::Url(url.into()))
        }
    }

    fn from_content(content: A2aPartContent) -> Self {
        Self {
            content,
            filename: None,
            media_type: None,
            metadata: None,
        }
    }

    /// Borrow the text of a text part, if this is one.
    pub fn as_text(&self) -> Option<&str> {
        match &self.content {
            A2aPartContent::Text(text) => Some(text.as_str()),
            _ => None,
        }
    }

    /// Borrow the payload of a data part, if this is one.
    pub fn as_data(&self) -> Option<&Value> {
        match &self.content {
            A2aPartContent::Data(data) => Some(data),
            _ => None,
        }
    }
}

impl Serialize for A2aPart {
    /// Emits the **v1.0 form only** — exactly one content member, named after
    /// the variant, with no `kind`/`type` discriminator (§A.2.1).
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        let extra = self.filename.is_some() as usize
            + self.media_type.is_some() as usize
            + self.metadata.is_some() as usize;
        let mut map = serializer.serialize_map(Some(1 + extra))?;

        match &self.content {
            A2aPartContent::Text(text) => map.serialize_entry("text", text)?,
            A2aPartContent::Raw(raw) => map.serialize_entry("raw", raw)?,
            A2aPartContent::Url(url) => map.serialize_entry("url", url)?,
            A2aPartContent::Data(data) => map.serialize_entry("data", data)?,
        }
        if let Some(filename) = &self.filename {
            map.serialize_entry("filename", filename)?;
        }
        if let Some(media_type) = &self.media_type {
            map.serialize_entry("mediaType", media_type)?;
        }
        if let Some(metadata) = &self.metadata {
            map.serialize_entry("metadata", metadata)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for A2aPart {
    /// Accepts, in priority order:
    ///
    /// 1. **v1.0** — a bare `text` / `raw` / `url` / `data` member;
    /// 2. **v0.3.x legacy** — a `kind` discriminator with the nested `file`
    ///    object (`name` / `mimeType` / `fileWithBytes` / `fileWithUri`);
    /// 3. **this crate's pre-1.0 form** — a `type` discriminator.
    ///
    /// Only form 1 is ever emitted; the rest exist for the §A.2 overlap period.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PartVisitor;

        impl<'de> Visitor<'de> for PartVisitor {
            type Value = A2aPart;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an A2A Part object with a text, raw, url, or data member")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut access: M) -> Result<A2aPart, M::Error> {
                let mut obj = serde_json::Map::new();
                while let Some((key, value)) = access.next_entry::<String, Value>()? {
                    obj.insert(key, value);
                }
                part_from_map(obj).map_err(M::Error::custom)
            }
        }

        deserializer.deserialize_map(PartVisitor)
    }
}

/// Shared decoding logic for [`A2aPart`], expressed over a plain JSON map so
/// it can be unit-tested directly and reused by any future binding.
fn part_from_map(mut obj: serde_json::Map<String, Value>) -> Result<A2aPart, String> {
    let take_string = |obj: &mut serde_json::Map<String, Value>, key: &str| -> Option<String> {
        obj.remove(key).and_then(|v| match v {
            Value::String(s) => Some(s),
            _ => None,
        })
    };

    // Legacy v0.3.x FilePart: unwrap the nested `file` object into the flat
    // v1.0 representation before the generic member probe below.
    if obj.get("kind").and_then(Value::as_str) == Some("file") {
        let file = obj
            .remove("file")
            .ok_or("file part missing `file` member")?;
        let mut file = match file {
            Value::Object(map) => map,
            _ => return Err("`file` member must be an object".to_string()),
        };
        let filename = take_string(&mut file, "name");
        let media_type = take_string(&mut file, "mimeType");
        if let Some(bytes) = take_string(&mut file, "fileWithBytes") {
            return Ok(A2aPart::raw_file(bytes, filename, media_type));
        }
        if let Some(uri) =
            take_string(&mut file, "fileWithUri").or_else(|| take_string(&mut file, "uri"))
        {
            return Ok(A2aPart::url_file(uri, filename, media_type));
        }
        return Err("file part has neither fileWithBytes nor fileWithUri".to_string());
    }

    // Discriminators are ignored from here on: in v1.0 the member name *is*
    // the discriminator, and for the legacy `text`/`data` forms the tag is
    // redundant with the member that carries the payload.
    obj.remove("kind");
    obj.remove("type");

    let filename = take_string(&mut obj, "filename");
    let media_type =
        take_string(&mut obj, "mediaType").or_else(|| take_string(&mut obj, "mimeType"));
    let metadata = obj.remove("metadata");

    let content = if let Some(Value::String(text)) = obj.remove("text") {
        A2aPartContent::Text(text)
    } else if let Some(Value::String(raw)) = obj.remove("raw") {
        A2aPartContent::Raw(raw)
    } else if let Some(Value::String(url)) = obj.remove("url") {
        A2aPartContent::Url(url)
    } else if let Some(data) = obj.remove("data") {
        A2aPartContent::Data(data)
    } else {
        return Err("part has no text, raw, url, or data member".to_string());
    };

    Ok(A2aPart {
        content,
        filename,
        media_type,
        metadata,
    })
}

/// An artifact produced by a completed task (proto `Artifact`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct A2aArtifact {
    pub artifact_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parts: Vec<A2aPart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

// ── Task ────────────────────────────────────────────────────────────────────

/// Task lifecycle states (proto `TaskState`).
///
/// Serializes per ProtoJSON (§5.5) as `TASK_STATE_*`; deserializes those plus
/// the pre-1.0 lowercase spellings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A2aTaskState {
    Submitted,
    Working,
    Completed,
    Failed,
    Canceled,
    Rejected,
    /// Interrupted state: the agent needs more user input to proceed.
    InputRequired,
    /// Interrupted state: the agent needs authentication to proceed.
    AuthRequired,
}

impl A2aTaskState {
    pub fn as_protojson(self) -> &'static str {
        match self {
            Self::Submitted => "TASK_STATE_SUBMITTED",
            Self::Working => "TASK_STATE_WORKING",
            Self::Completed => "TASK_STATE_COMPLETED",
            Self::Failed => "TASK_STATE_FAILED",
            Self::Canceled => "TASK_STATE_CANCELED",
            Self::Rejected => "TASK_STATE_REJECTED",
            Self::InputRequired => "TASK_STATE_INPUT_REQUIRED",
            Self::AuthRequired => "TASK_STATE_AUTH_REQUIRED",
        }
    }

    /// Returns `true` for terminal states that will never transition further.
    ///
    /// `InputRequired` and `AuthRequired` are *interrupted*, not terminal —
    /// the spec distinguishes the two and such tasks can still progress.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Canceled | Self::Rejected
        )
    }
}

impl Serialize for A2aTaskState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_protojson())
    }
}

impl<'de> Deserialize<'de> for A2aTaskState {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        match raw.as_str() {
            // v1.0 ProtoJSON.
            "TASK_STATE_SUBMITTED" => Ok(Self::Submitted),
            "TASK_STATE_WORKING" => Ok(Self::Working),
            "TASK_STATE_COMPLETED" => Ok(Self::Completed),
            "TASK_STATE_FAILED" => Ok(Self::Failed),
            "TASK_STATE_CANCELED" => Ok(Self::Canceled),
            "TASK_STATE_REJECTED" => Ok(Self::Rejected),
            "TASK_STATE_INPUT_REQUIRED" => Ok(Self::InputRequired),
            "TASK_STATE_AUTH_REQUIRED" => Ok(Self::AuthRequired),
            // Legacy pre-1.0 spellings, accepted per §A.2.
            "submitted" => Ok(Self::Submitted),
            "working" => Ok(Self::Working),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "canceled" => Ok(Self::Canceled),
            "rejected" => Ok(Self::Rejected),
            "input-required" | "input_required" => Ok(Self::InputRequired),
            "auth-required" | "auth_required" => Ok(Self::AuthRequired),
            other => Err(D::Error::custom(format!("unknown task state: {other}"))),
        }
    }
}

/// The status block embedded in a task response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aTaskStatus {
    pub state: A2aTaskState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<A2aMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// An A2A task as returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct A2aTask {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    pub status: A2aTaskStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<A2aArtifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<A2aMessage>,
}

/// Result of `SendMessage` (proto `SendMessageResponse`).
///
/// A `oneof payload` of `task` or `message`; per §A.2.1 the member name is the
/// discriminator. We always create a task, so only `task` is ever populated.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<A2aTask>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<A2aMessage>,
}

impl SendMessageResponse {
    pub fn task(task: A2aTask) -> Self {
        Self {
            task: Some(task),
            message: None,
        }
    }
}

/// Result of `ListTasks` (proto `ListTasksResponse`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTasksResponse {
    pub tasks: Vec<A2aTask>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

// ── Request / Response envelopes ────────────────────────────────────────────

// ── JSON-RPC 2.0 envelope ───────────────────────────────────────────────────

/// A JSON-RPC 2.0 request envelope.
///
/// `id` and `params` default to `Value::Null` when the field is absent, so
/// notifications and parameter-less requests both parse cleanly.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: serde_json::Value,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// A JSON-RPC 2.0 response envelope. Exactly one of `result` and `error` is
/// set on a well-formed response.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse<T: Serialize> {
    pub jsonrpc: &'static str,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcErrorObj>,
}

/// The JSON-RPC error object inside a `JsonRpcResponse`.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcErrorObj {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Params for `SendMessage` (proto `SendMessageRequest`).
///
/// The v1.0 shape is `{message, configuration?, metadata?, tenant?}`, with
/// `contextId` carried *inside* the message. The pre-1.0 top-level `contextId`
/// and `skillId` params are still accepted per §A.2 and used as fallbacks.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageParams {
    pub message: A2aMessage,
    #[serde(default)]
    pub configuration: Option<SendMessageConfiguration>,
    #[serde(default)]
    pub metadata: Option<Value>,
    /// Legacy pre-1.0 param. In v1.0 this belongs on `message.contextId`.
    #[serde(default)]
    pub context_id: Option<String>,
    /// Non-spec extension: names a specific skill to invoke directly.
    #[serde(default)]
    pub skill_id: Option<String>,
}

impl SendMessageParams {
    /// Resolve the effective context id, preferring the v1.0 location
    /// (`message.contextId`) over the legacy top-level param.
    pub fn effective_context_id(&self) -> Option<String> {
        self.message
            .context_id
            .clone()
            .or_else(|| self.context_id.clone())
    }
}

/// Subset of proto `SendMessageConfiguration` that we can honour today.
/// Unknown members are ignored rather than rejected, so clients sending the
/// full configuration (push notification config, blocking mode, …) still work.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageConfiguration {
    #[serde(default)]
    pub history_length: Option<i32>,
    #[serde(default)]
    pub return_immediately: Option<bool>,
}

/// Params for `GetTask` / `CancelTask`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskIdParams {
    pub id: String,
    #[serde(default)]
    pub history_length: Option<i32>,
}

/// Internal request shape used by the adapter. Not a wire type.
#[derive(Debug, Clone, Deserialize)]
pub struct MessageSendRequest {
    pub messages: Vec<A2aMessage>,
    /// If present, names a specific tool/skill to invoke directly (bypassing
    /// conversational routing). The tool arguments are expected in the first
    /// message's data part.
    #[serde(default)]
    pub tool: Option<String>,
}

// ── Errors ──────────────────────────────────────────────────────────────────

/// Standard A2A-compatible error payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl A2aError {
    // ── Standard error constructors ─────────────────────────────────────

    pub fn parse_error(detail: impl Into<String>) -> Self {
        Self {
            code: -32700,
            message: detail.into(),
            data: None,
        }
    }

    pub fn invalid_request(detail: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: detail.into(),
            data: None,
        }
    }

    pub fn method_not_found(detail: impl Into<String>) -> Self {
        Self {
            code: -32601,
            message: detail.into(),
            data: None,
        }
    }

    pub fn invalid_params(detail: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: detail.into(),
            data: None,
        }
    }

    pub fn internal_error(detail: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: detail.into(),
            data: None,
        }
    }

    // ── A2A-specific errors ─────────────────────────────────────────────

    pub fn task_not_found(task_id: &str) -> Self {
        Self {
            code: -32001,
            message: format!("Task not found: {task_id}"),
            data: None,
        }
    }

    pub fn unsupported_operation(detail: impl Into<String>) -> Self {
        Self {
            code: -32004,
            message: detail.into(),
            data: None,
        }
    }

    pub fn push_notification_not_supported() -> Self {
        Self {
            code: -32005,
            message: "Push notifications are not supported".to_string(),
            data: None,
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── §A.2.1: kind discriminator removed ──────────────────────────────

    #[test]
    fn text_part_serializes_as_bare_member() {
        let json = serde_json::to_string(&A2aPart::text("Hello, world!")).unwrap();
        assert_eq!(json, r#"{"text":"Hello, world!"}"#);
    }

    #[test]
    fn part_never_emits_kind_or_type_discriminator() {
        let parts = vec![
            A2aPart::text("hi"),
            A2aPart::data(json!({"answer": 42})),
            A2aPart::raw_file(
                "iVBORw0KGgo...",
                Some("d.png".into()),
                Some("image/png".into()),
            ),
            A2aPart::url_file("https://x/y.png", None, Some("image/png".into())),
        ];
        for part in parts {
            let json = serde_json::to_string(&part).unwrap();
            assert!(!json.contains("\"kind\""), "emitted kind: {json}");
            assert!(!json.contains("\"type\""), "emitted type: {json}");
        }
    }

    #[test]
    fn file_part_serializes_flat_with_media_type() {
        let part = A2aPart::raw_file(
            "iVBORw0KGgo...",
            Some("diagram.png".into()),
            Some("image/png".into()),
        );
        let value: Value = serde_json::to_value(&part).unwrap();
        assert_eq!(value["raw"], "iVBORw0KGgo...");
        assert_eq!(value["filename"], "diagram.png");
        assert_eq!(value["mediaType"], "image/png");
        assert!(value.get("file").is_none());
    }

    #[test]
    fn data_part_carries_json_media_type() {
        let value: Value = serde_json::to_value(A2aPart::data(json!({"k": "v"}))).unwrap();
        assert_eq!(value["data"]["k"], "v");
        assert_eq!(value["mediaType"], "application/json");
    }

    #[test]
    fn part_accepts_v1_bare_member_form() {
        let part: A2aPart = serde_json::from_str(r#"{"text":"hi"}"#).unwrap();
        assert_eq!(part.as_text(), Some("hi"));

        let part: A2aPart =
            serde_json::from_str(r#"{"url":"https://x/y","mediaType":"image/png"}"#).unwrap();
        assert_eq!(part.content, A2aPartContent::Url("https://x/y".into()));
        assert_eq!(part.media_type.as_deref(), Some("image/png"));
    }

    #[test]
    fn part_accepts_legacy_kind_text_form() {
        let part: A2aPart = serde_json::from_str(r#"{"kind":"text","text":"hello"}"#).unwrap();
        assert_eq!(part.as_text(), Some("hello"));
    }

    #[test]
    fn part_accepts_legacy_kind_data_form() {
        let part: A2aPart = serde_json::from_str(r#"{"kind":"data","data":{"a":1}}"#).unwrap();
        assert_eq!(part.as_data().unwrap()["a"], 1);
    }

    #[test]
    fn part_accepts_legacy_kind_file_with_bytes() {
        let json = r#"{"kind":"file","file":{"name":"diagram.png",
            "mimeType":"image/png","fileWithBytes":"iVBORw0KGgo..."}}"#;
        let part: A2aPart = serde_json::from_str(json).unwrap();
        assert_eq!(part.content, A2aPartContent::Raw("iVBORw0KGgo...".into()));
        assert_eq!(part.filename.as_deref(), Some("diagram.png"));
        assert_eq!(part.media_type.as_deref(), Some("image/png"));
    }

    #[test]
    fn part_accepts_legacy_kind_file_with_uri() {
        let json = r#"{"kind":"file","file":{"name":"a.pdf","fileWithUri":"https://x/a.pdf"}}"#;
        let part: A2aPart = serde_json::from_str(json).unwrap();
        assert_eq!(part.content, A2aPartContent::Url("https://x/a.pdf".into()));
    }

    #[test]
    fn part_accepts_this_crates_legacy_type_form() {
        let part: A2aPart = serde_json::from_str(r#"{"type":"text","text":"hi"}"#).unwrap();
        assert_eq!(part.as_text(), Some("hi"));

        let part: A2aPart = serde_json::from_str(r#"{"type":"data","data":{"q":"x"}}"#).unwrap();
        assert_eq!(part.as_data().unwrap()["q"], "x");
    }

    #[test]
    fn legacy_part_round_trips_into_v1_form() {
        let part: A2aPart = serde_json::from_str(r#"{"kind":"text","text":"hi"}"#).unwrap();
        assert_eq!(serde_json::to_string(&part).unwrap(), r#"{"text":"hi"}"#);
    }

    #[test]
    fn part_without_content_member_is_rejected() {
        assert!(serde_json::from_str::<A2aPart>(r#"{"mediaType":"text/plain"}"#).is_err());
    }

    // ── §5.5: ProtoJSON enum encoding ───────────────────────────────────

    #[test]
    fn task_state_serializes_as_protojson() {
        let cases = [
            (A2aTaskState::Submitted, "\"TASK_STATE_SUBMITTED\""),
            (A2aTaskState::Working, "\"TASK_STATE_WORKING\""),
            (A2aTaskState::Completed, "\"TASK_STATE_COMPLETED\""),
            (A2aTaskState::Failed, "\"TASK_STATE_FAILED\""),
            (A2aTaskState::Canceled, "\"TASK_STATE_CANCELED\""),
            (A2aTaskState::Rejected, "\"TASK_STATE_REJECTED\""),
            (A2aTaskState::InputRequired, "\"TASK_STATE_INPUT_REQUIRED\""),
            (A2aTaskState::AuthRequired, "\"TASK_STATE_AUTH_REQUIRED\""),
        ];
        for (state, expected) in cases {
            assert_eq!(serde_json::to_string(&state).unwrap(), expected);
        }
    }

    #[test]
    fn task_state_roundtrips() {
        for state in [
            A2aTaskState::Submitted,
            A2aTaskState::Working,
            A2aTaskState::Completed,
            A2aTaskState::Failed,
            A2aTaskState::Canceled,
            A2aTaskState::Rejected,
            A2aTaskState::InputRequired,
            A2aTaskState::AuthRequired,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let back: A2aTaskState = serde_json::from_str(&json).unwrap();
            assert_eq!(back, state);
        }
    }

    #[test]
    fn task_state_accepts_legacy_lowercase_names() {
        let back: A2aTaskState = serde_json::from_str("\"completed\"").unwrap();
        assert_eq!(back, A2aTaskState::Completed);
        let back: A2aTaskState = serde_json::from_str("\"input-required\"").unwrap();
        assert_eq!(back, A2aTaskState::InputRequired);
    }

    #[test]
    fn unknown_task_state_is_rejected() {
        assert!(serde_json::from_str::<A2aTaskState>("\"TASK_STATE_RUNNING\"").is_err());
    }

    #[test]
    fn interrupted_states_are_not_terminal() {
        assert!(!A2aTaskState::Submitted.is_terminal());
        assert!(!A2aTaskState::Working.is_terminal());
        assert!(!A2aTaskState::InputRequired.is_terminal());
        assert!(!A2aTaskState::AuthRequired.is_terminal());
        assert!(A2aTaskState::Completed.is_terminal());
        assert!(A2aTaskState::Failed.is_terminal());
        assert!(A2aTaskState::Canceled.is_terminal());
        assert!(A2aTaskState::Rejected.is_terminal());
    }

    #[test]
    fn role_serializes_as_protojson() {
        assert_eq!(
            serde_json::to_string(&A2aRole::User).unwrap(),
            "\"ROLE_USER\""
        );
        assert_eq!(
            serde_json::to_string(&A2aRole::Agent).unwrap(),
            "\"ROLE_AGENT\""
        );
    }

    #[test]
    fn role_accepts_legacy_lowercase_names() {
        let user: A2aRole = serde_json::from_str("\"user\"").unwrap();
        assert_eq!(user, A2aRole::User);
        let agent: A2aRole = serde_json::from_str("\"agent\"").unwrap();
        assert_eq!(agent, A2aRole::Agent);
        let assistant: A2aRole = serde_json::from_str("\"assistant\"").unwrap();
        assert_eq!(assistant, A2aRole::Agent);
    }

    // ── §4.4.1 / §A.2.2: Agent Card ─────────────────────────────────────

    fn sample_card() -> AgentCard {
        AgentCard {
            name: "OmniLauncher".to_string(),
            description: "Desktop agent".to_string(),
            supported_interfaces: vec![AgentInterface {
                url: "http://127.0.0.1:1423".to_string(),
                protocol_binding: "JSONRPC".to_string(),
                protocol_version: A2A_PROTOCOL_VERSION.to_string(),
            }],
            version: "0.1.0".to_string(),
            capabilities: AgentCapabilities {
                streaming: false,
                push_notifications: false,
                extended_agent_card: false,
            },
            security_schemes: [(
                "bearer".to_string(),
                SecurityScheme {
                    http_auth_security_scheme: Some(HttpAuthSecurityScheme {
                        scheme: "Bearer".to_string(),
                        description: None,
                    }),
                },
            )]
            .into_iter()
            .collect(),
            security_requirements: vec![SecurityRequirement {
                schemes: [("bearer".to_string(), StringList::default())]
                    .into_iter()
                    .collect(),
            }],
            default_input_modes: vec!["text/plain".to_string()],
            default_output_modes: vec!["text/plain".to_string()],
            skills: vec![AgentSkill {
                id: "calculator".to_string(),
                name: "Calculator".to_string(),
                description: "Evaluate math expressions".to_string(),
                tags: vec!["math".to_string()],
                examples: vec![],
                input_modes: vec![],
                output_modes: vec![],
            }],
        }
    }

    #[test]
    fn agent_card_roundtrip() {
        let json = serde_json::to_string_pretty(&sample_card()).unwrap();
        let back: AgentCard = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "OmniLauncher");
        assert!(!back.capabilities.streaming);
        assert_eq!(back.skills.len(), 1);
        assert_eq!(back.skills[0].id, "calculator");
    }

    #[test]
    fn agent_card_declares_extended_agent_card_under_capabilities() {
        let value: Value = serde_json::to_value(sample_card()).unwrap();
        // §A.2.2: the flag lives in `capabilities`, never at the top level.
        assert_eq!(value["capabilities"]["extendedAgentCard"], false);
        assert!(value.get("supportsExtendedAgentCard").is_none());
    }

    #[test]
    fn agent_card_declares_supported_interfaces_with_protocol_version() {
        let value: Value = serde_json::to_value(sample_card()).unwrap();
        let iface = &value["supportedInterfaces"][0];
        assert_eq!(iface["protocolBinding"], "JSONRPC");
        assert_eq!(iface["protocolVersion"], "1.0");
        // Pre-1.0 top-level `url` was replaced by `supportedInterfaces`.
        assert!(value.get("url").is_none());
    }

    #[test]
    fn agent_card_uses_spec_security_schemes() {
        let value: Value = serde_json::to_value(sample_card()).unwrap();
        assert_eq!(
            value["securitySchemes"]["bearer"]["httpAuthSecurityScheme"]["scheme"],
            "Bearer"
        );
        // The pre-1.0 non-spec `authentication` block is gone.
        assert!(value.get("authentication").is_none());
    }

    // ── Messages, artifacts, tasks ──────────────────────────────────────

    #[test]
    fn message_serializes_role_and_message_id() {
        let value: Value = serde_json::to_value(A2aMessage::agent_text("done")).unwrap();
        assert_eq!(value["role"], "ROLE_AGENT");
        assert_eq!(value["parts"][0]["text"], "done");
        assert!(value["messageId"].as_str().is_some_and(|s| !s.is_empty()));
    }

    #[test]
    fn message_without_message_id_gets_one_generated() {
        let json = r#"{"role":"user","parts":[{"text":"hi"}]}"#;
        let msg: A2aMessage = serde_json::from_str(json).unwrap();
        assert!(!msg.message_id.is_empty());
        assert_eq!(msg.role, A2aRole::User);
    }

    #[test]
    fn message_text_joins_text_parts_only() {
        let msg = A2aMessage::new(
            A2aRole::User,
            vec![
                A2aPart::text("one"),
                A2aPart::data(json!({"ignored": true})),
                A2aPart::text("two"),
            ],
        );
        assert_eq!(msg.text(), "one\ntwo");
    }

    #[test]
    fn a2a_task_roundtrip() {
        let task = A2aTask {
            id: "task-001".to_string(),
            context_id: Some("ctx-1".to_string()),
            status: A2aTaskStatus {
                state: A2aTaskState::Completed,
                message: Some(A2aMessage::agent_text("Done!")),
                timestamp: Some("2026-06-25T12:00:00Z".to_string()),
            },
            artifacts: vec![A2aArtifact {
                artifact_id: "art-1".to_string(),
                name: Some("result".to_string()),
                description: None,
                parts: vec![A2aPart::data(json!({"answer": 42}))],
                metadata: None,
            }],
            history: vec![],
        };

        let json = serde_json::to_string_pretty(&task).unwrap();
        let back: A2aTask = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "task-001");
        assert_eq!(back.context_id.as_deref(), Some("ctx-1"));
        assert_eq!(back.status.state, A2aTaskState::Completed);
        assert_eq!(back.artifacts.len(), 1);
        assert_eq!(back.artifacts[0].artifact_id, "art-1");
        assert_eq!(back.artifacts[0].parts[0].as_data().unwrap()["answer"], 42);
    }

    #[test]
    fn a2a_task_serializes_context_id_when_present() {
        let task = A2aTask {
            id: "t-1".to_string(),
            context_id: Some("ctx-42".to_string()),
            status: A2aTaskStatus {
                state: A2aTaskState::Completed,
                message: None,
                timestamp: None,
            },
            artifacts: vec![],
            history: vec![],
        };
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("\"contextId\":\"ctx-42\""));
    }

    #[test]
    fn a2a_task_omits_context_id_when_none() {
        let task = A2aTask {
            id: "t-1".to_string(),
            context_id: None,
            status: A2aTaskStatus {
                state: A2aTaskState::Completed,
                message: None,
                timestamp: None,
            },
            artifacts: vec![],
            history: vec![],
        };
        assert!(!serde_json::to_string(&task).unwrap().contains("contextId"));
    }

    #[test]
    fn a2a_artifact_serializes_artifact_id() {
        let artifact = A2aArtifact {
            artifact_id: "art-abc".to_string(),
            name: Some("results".to_string()),
            description: None,
            parts: vec![A2aPart::text("hi")],
            metadata: None,
        };
        let json = serde_json::to_string(&artifact).unwrap();
        assert!(json.contains("\"artifactId\":\"art-abc\""));
        // The pre-1.0 non-spec `index` field is gone.
        assert!(!json.contains("index"));
    }

    #[test]
    fn send_message_response_wraps_task_under_member_name() {
        let task = A2aTask {
            id: "t-9".to_string(),
            context_id: None,
            status: A2aTaskStatus {
                state: A2aTaskState::Working,
                message: None,
                timestamp: None,
            },
            artifacts: vec![],
            history: vec![],
        };
        let value: Value = serde_json::to_value(SendMessageResponse::task(task)).unwrap();
        assert_eq!(value["task"]["id"], "t-9");
        assert!(value.get("message").is_none());
    }

    // ── Params ──────────────────────────────────────────────────────────

    #[test]
    fn send_message_params_reads_context_id_from_message() {
        let json = r#"{
            "message": {"role":"ROLE_USER","messageId":"m1","contextId":"ctx-v1",
                        "parts":[{"text":"hi"}]}
        }"#;
        let params: SendMessageParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.effective_context_id().as_deref(), Some("ctx-v1"));
    }

    #[test]
    fn send_message_params_falls_back_to_legacy_top_level_context_id() {
        let json = r#"{
            "message": {"role":"user","parts":[{"type":"text","text":"hi"}]},
            "contextId": "ctx-legacy",
            "skillId": "skill:alibaba"
        }"#;
        let params: SendMessageParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.effective_context_id().as_deref(), Some("ctx-legacy"));
        assert_eq!(params.skill_id.as_deref(), Some("skill:alibaba"));
        assert_eq!(params.message.role, A2aRole::User);
    }

    #[test]
    fn send_message_params_accepts_configuration_block() {
        let json = r#"{
            "message": {"role":"ROLE_USER","parts":[{"text":"hi"}]},
            "configuration": {"historyLength": 5, "returnImmediately": true}
        }"#;
        let params: SendMessageParams = serde_json::from_str(json).unwrap();
        let config = params.configuration.unwrap();
        assert_eq!(config.history_length, Some(5));
        assert_eq!(config.return_immediately, Some(true));
    }

    #[test]
    fn task_id_params_deserializes_id_field() {
        let params: TaskIdParams = serde_json::from_str(r#"{"id":"task-xyz"}"#).unwrap();
        assert_eq!(params.id, "task-xyz");
    }

    #[test]
    fn message_send_request_with_tool() {
        let json = r#"{
            "messages": [{"role":"ROLE_USER","parts":[{"text":"hello"}]}],
            "tool": "calculator"
        }"#;
        let req: MessageSendRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.tool, Some("calculator".to_string()));
        assert_eq!(req.messages.len(), 1);
    }

    #[test]
    fn message_send_request_without_tool() {
        let json = r#"{"messages":[{"role":"ROLE_USER","parts":[{"text":"when?"}]}]}"#;
        let req: MessageSendRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.tool, None);
    }

    // ── Errors / JSON-RPC envelope ──────────────────────────────────────

    #[test]
    fn a2a_error_serialization() {
        let err = A2aError::unsupported_operation("Streaming is not supported in this version");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("-32004"));
        assert!(json.contains("Streaming is not supported"));

        let json2 = serde_json::to_string(&A2aError::task_not_found("abc-123")).unwrap();
        assert!(json2.contains("-32001"));
        assert!(json2.contains("abc-123"));
    }

    #[test]
    fn jsonrpc_request_deserializes_full_envelope() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 42,
            "method": "SendMessage",
            "params": {"skillId":"skill:alibaba"}
        }"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "SendMessage");
        assert!(req.id.is_number());
        assert_eq!(req.params["skillId"], "skill:alibaba");
    }

    #[test]
    fn jsonrpc_request_defaults_missing_id_and_params_to_null() {
        let req: JsonRpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","method":"GetTask"}"#).unwrap();
        assert!(req.id.is_null());
        assert!(req.params.is_null());
    }

    #[test]
    fn jsonrpc_response_success_serializes_without_error_field() {
        let resp = JsonRpcResponse::<Value> {
            jsonrpc: "2.0",
            id: json!(1),
            result: Some(json!({"ok": true})),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\":{\"ok\":true}"));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn jsonrpc_response_error_serializes_without_result_field() {
        let resp = JsonRpcResponse::<Value> {
            jsonrpc: "2.0",
            id: json!(1),
            result: None,
            error: Some(JsonRpcErrorObj {
                code: -32601,
                message: "Method not found".to_string(),
                data: None,
            }),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
        assert!(json.contains("-32601"));
        assert!(!json.contains("\"result\""));
    }
}
