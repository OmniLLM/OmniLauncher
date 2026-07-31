use std::{collections::HashMap, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use http::{HeaderName, HeaderValue};
use rmcp::{
    model::{CallToolRequestParams, Tool},
    service::RunningService,
    transport::{
        auth::{
            AuthClient, AuthError, AuthorizationManager, CredentialStore, OAuthClientConfig,
            StoredCredentials,
        },
        streamable_http_client::{
            StreamableHttpClient, StreamableHttpClientTransport,
            StreamableHttpClientTransportConfig,
        },
        TokioChildProcess,
    },
    Peer, RoleClient, ServiceExt,
};

use crate::{
    plugins::{Plugin, Query, QueryResult},
    settings::{AppSettings, McpOAuthConfig, McpServerConfig},
};

/// Persistent rmcp credential store. Access and refresh tokens live in an
/// owner-only file outside settings.json, so the normal settings API cannot
/// expose or accidentally overwrite rotating OAuth credentials.
#[derive(Clone)]
struct FileCredentialStore {
    path: PathBuf,
}

impl FileCredentialStore {
    fn for_server(name: &str) -> Self {
        Self {
            path: crate::path_config::config_dir()
                .join("mcp-oauth")
                // Hex is path-safe and collision-free (`a-b` and `a_b` must not
                // share rotating refresh-token generations).
                .join(format!("{}.json", hex_component(name))),
        }
    }
}

#[async_trait]
impl CredentialStore for FileCredentialStore {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&self.path)
            .map_err(|e| AuthError::InternalError(format!("read MCP credentials: {e}")))?;
        let credentials = serde_json::from_slice(&bytes)
            .map_err(|e| AuthError::InternalError(format!("decode MCP credentials: {e}")))?;
        Ok(Some(credentials))
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AuthError::InternalError(format!("create MCP credential dir: {e}")))?;
        }
        let bytes = serde_json::to_vec_pretty(&credentials)
            .map_err(|e| AuthError::InternalError(format!("encode MCP credentials: {e}")))?;
        // Refresh tokens may be single-use. Never truncate the last valid
        // generation in place: write+sync a sibling and atomically rename it.
        let temporary = self.path.with_extension(format!(
            "json.tmp.{}.{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        if !crate::settings::write_private_file(&temporary, &bytes) {
            return Err(AuthError::InternalError(format!(
                "write MCP credentials to {}",
                temporary.display()
            )));
        }
        if let Ok(file) = std::fs::File::open(&temporary) {
            let _ = file.sync_all();
        }
        if let Err(error) = std::fs::rename(&temporary, &self.path) {
            let _ = std::fs::remove_file(&temporary);
            return Err(AuthError::InternalError(format!(
                "commit MCP credentials to {}: {error}",
                self.path.display()
            )));
        }
        Ok(())
    }

    async fn clear(&self) -> Result<(), AuthError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AuthError::InternalError(format!(
                "remove MCP credentials: {e}"
            ))),
        }
    }
}

/// Owns the RMCP service task. Every discovered tool holds this shared session;
/// dropping the final tool closes the underlying Streamable HTTP connection.
struct McpSession {
    _service: RunningService<RoleClient, ()>,
    peer: Peer<RoleClient>,
}

struct McpToolPlugin {
    display_name: String,
    remote_name: String,
    schema: serde_json::Value,
    session: Arc<McpSession>,
}

#[async_trait]
impl Plugin for McpToolPlugin {
    fn name(&self) -> &str {
        &self.display_name
    }

    fn description(&self) -> &str {
        self.schema["function"]["description"]
            .as_str()
            .unwrap_or("Remote MCP tool")
    }

    fn keyword(&self) -> Option<&str> {
        None
    }

    fn cheap_prefix_match(&self, _raw: &str) -> bool {
        false
    }

    async fn query(&self, _q: &Query) -> Vec<QueryResult> {
        Vec::new()
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(self.schema.clone())
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let mut arguments = args.as_object().cloned().unwrap_or_default();
        // Callers that can only supply free-form text (e.g. A2A text parts)
        // hand us a synthetic `{"input": "..."}`. No MCP tool declares an
        // `input` parameter, so remap it onto the tool's own required string
        // parameter; otherwise the server rejects the call as missing args.
        if let Some(value) = remap_input_placeholder(&arguments, &self.schema) {
            if let Some(text) = arguments.remove("input") {
                arguments.insert(value, text);
            }
        }
        let request =
            CallToolRequestParams::new(self.remote_name.clone()).with_arguments(arguments);
        match self.session.peer.call_tool(request).await {
            Ok(result) => serde_json::to_string(&result)
                .unwrap_or_else(|e| format!("Error: failed to encode MCP result: {e}")),
            Err(e) => format!("Error: MCP tool '{}' failed: {e}", self.remote_name),
        }
    }
}

/// If `arguments` is exactly the synthetic `{"input": "..."}` placeholder,
/// return the name of the parameter it should be remapped to: the tool's sole
/// required string parameter, or (when nothing is marked required) the single
/// string property it declares. Returns `None` when the mapping is ambiguous or
/// the tool genuinely accepts `input`.
fn remap_input_placeholder(
    arguments: &serde_json::Map<String, serde_json::Value>,
    schema: &serde_json::Value,
) -> Option<String> {
    if arguments.len() != 1 || !arguments.get("input")?.is_string() {
        return None;
    }
    let params = &schema["function"]["parameters"];
    let properties = params.get("properties")?.as_object()?;
    if properties.contains_key("input") {
        return None;
    }

    let is_string = |name: &str| {
        properties
            .get(name)
            .map(|p| {
                let t = &p["type"];
                t == "string" || t.as_array().is_some_and(|a| a.iter().any(|v| v == "string"))
            })
            .unwrap_or(false)
    };

    let required: Vec<&str> = params
        .get("required")
        .and_then(|r| r.as_array())
        .map(|r| r.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    // Exactly one required parameter, and it takes a string.
    if let [only] = required.as_slice() {
        return is_string(only).then(|| (*only).to_string());
    }

    // Nothing required: fall back only if there is a single string property.
    if required.is_empty() {
        let mut strings = properties.keys().filter(|k| is_string(k));
        let first = strings.next()?;
        if strings.next().is_none() {
            return Some(first.clone());
        }
    }
    None
}

fn sanitize_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut previous_underscore = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            previous_underscore = false;
        } else if !previous_underscore {
            out.push('_');
            previous_underscore = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn hex_component(value: &str) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        let _ = write!(output, "{byte:02x}");
    }
    if output.is_empty() {
        "empty".to_string()
    } else {
        output
    }
}

pub(crate) fn tool_to_openai_schema(server_name: &str, tool: &Tool) -> serde_json::Value {
    let tool_name = format!(
        "mcp_{}_{}",
        sanitize_component(server_name),
        sanitize_component(tool.name.as_ref())
    );
    serde_json::json!({
        "type": "function",
        "function": {
            "name": tool_name,
            "description": tool.description.as_deref().unwrap_or("Remote MCP tool"),
            "parameters": tool.input_schema.as_ref(),
        }
    })
}

fn custom_headers(config: &McpServerConfig) -> Result<HashMap<HeaderName, HeaderValue>, String> {
    config
        .headers
        .iter()
        .map(|(name, value)| {
            let name = HeaderName::try_from(name.as_str())
                .map_err(|e| format!("invalid MCP header name '{name}': {e}"))?;
            let value = HeaderValue::try_from(value.as_str())
                .map_err(|e| format!("invalid MCP header value: {e}"))?;
            Ok((name, value))
        })
        .collect()
}

fn oauth_client_secret(oauth: &McpOAuthConfig) -> Option<String> {
    if !oauth.client_secret_env.trim().is_empty() {
        if let Ok(value) = std::env::var(oauth.client_secret_env.trim()) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }
    (!oauth.client_secret.trim().is_empty()).then(|| oauth.client_secret.clone())
}

/// Turn an initialized MCP session into launcher plugins. Shared by every
/// transport: only the handshake differs, tool discovery does not.
async fn plugins_from_service(
    server_name: &str,
    service: RunningService<RoleClient, ()>,
) -> Result<Vec<Box<dyn Plugin>>, String> {
    let peer = service.peer().clone();
    let tools = peer
        .list_all_tools()
        .await
        .map_err(|e| format!("MCP server '{server_name}' tool discovery failed: {e}"))?;
    let session = Arc::new(McpSession {
        _service: service,
        peer,
    });

    Ok(tools
        .into_iter()
        .map(|tool| {
            // Plugin names participate in an override index, so keep them
            // unique even when a server gives several tools the same title.
            let display_name = format!("MCP {server_name}/{}", tool.name);
            Box::new(McpToolPlugin {
                display_name,
                remote_name: tool.name.to_string(),
                schema: tool_to_openai_schema(server_name, &tool),
                session: session.clone(),
            }) as Box<dyn Plugin>
        })
        .collect())
}

async fn finish_connection<C>(
    server_name: &str,
    config: &McpServerConfig,
    client: C,
) -> Result<Vec<Box<dyn Plugin>>, String>
where
    C: StreamableHttpClient + Send + Sync + 'static,
    C::Error: std::error::Error + Send + Sync + 'static,
{
    let transport_config = StreamableHttpClientTransportConfig::with_uri(config.url.clone())
        .custom_headers(custom_headers(config)?)
        .reinit_on_expired_session(true);
    let transport = StreamableHttpClientTransport::with_client(client, transport_config);
    let service = ().serve(transport).await.map_err(|e| {
        format!(
            "MCP server '{server_name}' initialization failed at {}: {e}",
            config.url
        )
    })?;
    plugins_from_service(server_name, service).await
}

/// Spawn a local MCP server as a child process and speak JSON-RPC over its
/// stdin/stdout. The child is owned by the returned session, so it lives
/// exactly as long as the plugins that use it.
async fn connect_stdio(
    server_name: &str,
    config: &McpServerConfig,
) -> Result<Vec<Box<dyn Plugin>>, String> {
    if config.command.trim().is_empty() {
        return Err(format!(
            "MCP server '{server_name}' uses type 'stdio' but has no command"
        ));
    }
    // OAuth and HTTP headers are transport-specific; dropping them silently
    // would make a misconfigured server look like it simply has no tools.
    if config.oauth.is_some() {
        log::warn!("mcp: server '{server_name}' is stdio; ignoring its oauth configuration");
    }
    if !config.headers.is_empty() {
        log::warn!("mcp: server '{server_name}' is stdio; ignoring its http headers");
    }

    let mut command = tokio::process::Command::new(config.command.trim());
    command.args(&config.args);
    for (key, value) in &config.env {
        command.env(key, value);
    }

    let (transport, _stderr) = TokioChildProcess::builder(command)
        // The child's stderr is its own diagnostic channel (Node warnings and
        // similar); keep it out of the launcher's log.
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| {
            format!(
                "MCP server '{server_name}' failed to spawn '{}': {e}",
                config.command
            )
        })?;

    let service = ().serve(transport).await.map_err(|e| {
        format!(
            "MCP server '{server_name}' initialization failed running '{}': {e}",
            config.command
        )
    })?;
    plugins_from_service(server_name, service).await
}

async fn connect_one(
    server_name: &str,
    config: &McpServerConfig,
) -> Result<Vec<Box<dyn Plugin>>, String> {
    if config.transport_type.eq_ignore_ascii_case("stdio") {
        return connect_stdio(server_name, config).await;
    }
    if !config.transport_type.eq_ignore_ascii_case("http") {
        return Err(format!(
            "MCP server '{server_name}' uses unsupported type '{}'; only 'http' and 'stdio' are supported",
            config.transport_type
        ));
    }
    let parsed = reqwest13::Url::parse(&config.url)
        .map_err(|e| format!("MCP server '{server_name}' has invalid URL: {e}"))?;
    if parsed.scheme() != "https"
        && !(parsed.scheme() == "http"
            && matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "::1")))
    {
        return Err(format!(
            "MCP server '{server_name}' must use HTTPS (HTTP is allowed only for loopback testing)"
        ));
    }

    let http_client = reqwest13::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        // Never replay configured Authorization/X-API-Key headers to a redirect
        // target. MCP endpoints must be configured at their canonical URL.
        .redirect(reqwest13::redirect::Policy::none())
        .build()
        .map_err(|e| format!("build MCP HTTP client: {e}"))?;

    if let Some(oauth) = &config.oauth {
        let mut manager = AuthorizationManager::new(&config.url)
            .await
            .map_err(|e| format!("MCP OAuth setup failed for '{server_name}': {e}"))?;
        manager.set_credential_store(FileCredentialStore::for_server(server_name));
        let metadata = manager
            .discover_metadata()
            .await
            .map_err(|e| format!("MCP OAuth discovery failed for '{server_name}': {e}"))?;
        manager.set_metadata(metadata);
        let mut oauth_config =
            OAuthClientConfig::new(oauth.client_id.clone(), "http://127.0.0.1/oauth/callback")
                .with_scopes(oauth.scopes.clone());
        if let Some(secret) = oauth_client_secret(oauth) {
            oauth_config = oauth_config.with_client_secret(secret);
        }
        manager
            .configure_client(oauth_config)
            .map_err(|e| format!("MCP OAuth client configuration failed: {e}"))?;
        manager.get_access_token().await.map_err(|e| match e {
            AuthError::AuthorizationRequired => format!(
                "MCP server '{server_name}' requires OAuth authorization; run `ol mcp login {server_name}`"
            ),
            other => format!("MCP OAuth token unavailable for '{server_name}': {other}"),
        })?;
        let client = AuthClient::new(http_client, manager);
        finish_connection(server_name, config, client).await
    } else {
        finish_connection(server_name, config, http_client).await
    }
}

enum OAuthCallbackRequest {
    Ignore {
        status: &'static str,
        message: String,
    },
    Failure(String),
    Authorization {
        code: String,
        state: String,
        issuer: Option<String>,
    },
}

fn parse_oauth_callback_request(request: &str) -> OAuthCallbackRequest {
    let Some(first_line) = request.lines().next() else {
        return OAuthCallbackRequest::Ignore {
            status: "400 Bad Request",
            message: "Invalid OAuth callback request".to_string(),
        };
    };
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");
    if method != "GET" {
        return OAuthCallbackRequest::Ignore {
            status: "405 Method Not Allowed",
            message: "OAuth callback requires GET".to_string(),
        };
    }
    let callback = match reqwest13::Url::parse(&format!("http://127.0.0.1{target}")) {
        Ok(callback) => callback,
        Err(_) => {
            return OAuthCallbackRequest::Ignore {
                status: "400 Bad Request",
                message: "Invalid OAuth callback URL".to_string(),
            };
        }
    };
    if callback.path() != "/oauth/callback" {
        return OAuthCallbackRequest::Ignore {
            status: "404 Not Found",
            message: "Not Found".to_string(),
        };
    }
    let params: std::collections::HashMap<String, String> =
        callback.query_pairs().into_owned().collect();
    if let Some(error) = params.get("error") {
        return OAuthCallbackRequest::Failure(format!("MCP OAuth authorization failed: {error}"));
    }
    let Some(code) = params.get("code") else {
        return OAuthCallbackRequest::Failure("MCP OAuth callback omitted code".to_string());
    };
    let Some(state) = params.get("state") else {
        return OAuthCallbackRequest::Failure("MCP OAuth callback omitted state".to_string());
    };
    OAuthCallbackRequest::Authorization {
        code: code.clone(),
        state: state.clone(),
        issuer: params.get("iss").cloned(),
    }
}

async fn write_oauth_callback_response(
    stream: &mut tokio::net::TcpStream,
    status: &str,
    message: &str,
) {
    use tokio::io::AsyncWriteExt as _;
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{message}",
        message.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// A server needs a dynamically registered client when it has no `oauth` block
/// at all, or one whose `client_id` is blank. Both states mean "we have no
/// client credentials to present".
fn needs_registration(oauth: &Option<McpOAuthConfig>) -> bool {
    oauth
        .as_ref()
        .is_none_or(|oauth| oauth.client_id.trim().is_empty())
}

/// Scopes to request at registration. An explicit configuration always wins;
/// otherwise fall back to whatever the authorization server advertises, which
/// is what a client with no prior knowledge of the server can ask for.
fn resolve_scopes(configured: &[String], advertised: &[String]) -> Vec<String> {
    if configured.is_empty() {
        advertised.to_vec()
    } else {
        configured.to_vec()
    }
}

/// Store a dynamically registered client back into settings so subsequent
/// connects (which key off `config.oauth.is_some()`) can authenticate without
/// another registration. Settings are re-read here rather than mutating the
/// caller's snapshot, so a concurrent writer's unrelated changes survive.
fn persist_registered_client(server_name: &str, oauth: &McpOAuthConfig) -> Result<(), String> {
    let mut settings = crate::load_settings();
    let config = settings
        .mcp_servers
        .get_mut(server_name)
        .ok_or_else(|| format!("MCP server '{server_name}' is not configured"))?;
    config.oauth = Some(oauth.clone());
    if crate::save_settings(&settings) {
        Ok(())
    } else {
        Err(format!(
            "failed to save registered OAuth client for '{server_name}'"
        ))
    }
}

/// Complete an OAuth authorization-code + PKCE login for one configured MCP
/// server. The browser callback is loopback-only; credentials are persisted by
/// `FileCredentialStore` and subsequently refreshed by rmcp without another UI
/// login while the authorization remains valid.
pub async fn login(settings: &AppSettings, server_name: &str) -> Result<String, String> {
    use tokio::io::AsyncReadExt;

    let config = settings
        .mcp_servers
        .get(server_name)
        .ok_or_else(|| format!("MCP server '{server_name}' is not configured"))?;
    if config.transport_type.eq_ignore_ascii_case("stdio") {
        return Err(format!(
            "MCP server '{server_name}' is a stdio server; OAuth login applies only to http servers"
        ));
    }
    // A missing `oauth` block is not an error: servers whose authorization
    // server supports RFC 7591 dynamic client registration get a `client_id`
    // minted below, matching what other MCP clients do transparently.
    let oauth = config.oauth.clone();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| format!("bind MCP OAuth callback: {e}"))?;
    let callback_port = listener
        .local_addr()
        .map_err(|e| format!("read MCP OAuth callback address: {e}"))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{callback_port}/oauth/callback");

    let mut manager = AuthorizationManager::new(&config.url)
        .await
        .map_err(|e| format!("MCP OAuth setup failed: {e}"))?;
    manager.set_credential_store(FileCredentialStore::for_server(server_name));
    let metadata = manager
        .discover_metadata()
        .await
        .map_err(|e| format!("MCP OAuth metadata discovery failed: {e}"))?;
    let registration_endpoint = metadata.registration_endpoint.clone();
    let metadata_scopes = metadata.scopes_supported.clone().unwrap_or_default();
    manager.set_metadata(metadata);

    let oauth = if needs_registration(&oauth) {
        if registration_endpoint.is_none() {
            return Err(format!(
                "MCP server '{server_name}' has no oauth configuration and its \
                 authorization server does not advertise dynamic client \
                 registration; set one explicitly with `ol mcp update \
                 {server_name} --oauth-client-id <ID>`"
            ));
        }
        let scopes = resolve_scopes(
            oauth.as_ref().map(|o| o.scopes.as_slice()).unwrap_or(&[]),
            &metadata_scopes,
        );
        let scope_refs: Vec<&str> = scopes.iter().map(String::as_str).collect();
        // `register_client` configures the client internally, so the manual
        // `configure_client` below is deliberately skipped on this path.
        let registered = manager
            .register_client("OmniLauncher", &redirect_uri, &scope_refs)
            .await
            .map_err(|e| format!("MCP dynamic client registration failed: {e}"))?;
        log::info!(
            "mcp: registered OAuth client for '{server_name}' (client_id {})",
            registered.client_id
        );
        let effective = McpOAuthConfig {
            client_id: registered.client_id,
            client_secret: registered.client_secret.clone().unwrap_or_default(),
            client_secret_env: String::new(),
            scopes,
        };
        persist_registered_client(server_name, &effective)?;
        effective
    } else {
        let oauth = oauth.expect("needs_registration returns true for None");
        let mut client_config = OAuthClientConfig::new(oauth.client_id.clone(), &redirect_uri)
            .with_scopes(oauth.scopes.clone());
        if let Some(secret) = oauth_client_secret(&oauth) {
            client_config = client_config.with_client_secret(secret);
        }
        manager
            .configure_client(client_config)
            .map_err(|e| format!("MCP OAuth client configuration failed: {e}"))?;
        oauth
    };
    let oauth = &oauth;
    let scope_refs: Vec<&str> = oauth.scopes.iter().map(String::as_str).collect();
    let authorization_url = manager
        .get_authorization_url(&scope_refs)
        .await
        .map_err(|e| format!("build MCP OAuth authorization URL: {e}"))?;

    crate::plugins::url_opener::open_url_in_browser(&authorization_url)
        .map_err(|e| format!("open MCP OAuth browser: {e}"))?;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(300);
    let (mut callback_stream, code, state, issuer) = loop {
        let (mut stream, _) = tokio::time::timeout_at(deadline, listener.accept())
            .await
            .map_err(|_| "MCP OAuth callback timed out after 5 minutes".to_string())?
            .map_err(|e| format!("accept MCP OAuth callback: {e}"))?;
        let mut buffer = vec![0u8; 16 * 1024];
        let count = match tokio::time::timeout_at(deadline, stream.read(&mut buffer)).await {
            Ok(Ok(count)) => count,
            Ok(Err(error)) => {
                write_oauth_callback_response(
                    &mut stream,
                    "400 Bad Request",
                    "Unable to read OAuth callback request",
                )
                .await;
                log::debug!("mcp: failed to read loopback OAuth request: {error}");
                continue;
            }
            Err(_) => {
                write_oauth_callback_response(
                    &mut stream,
                    "408 Request Timeout",
                    "OAuth callback timed out",
                )
                .await;
                return Err("MCP OAuth callback timed out after 5 minutes".to_string());
            }
        };
        let request = String::from_utf8_lossy(&buffer[..count]);
        match parse_oauth_callback_request(&request) {
            OAuthCallbackRequest::Ignore { status, message } => {
                write_oauth_callback_response(&mut stream, status, &message).await;
            }
            OAuthCallbackRequest::Failure(error) => {
                write_oauth_callback_response(&mut stream, "400 Bad Request", &error).await;
                return Err(error);
            }
            OAuthCallbackRequest::Authorization {
                code,
                state,
                issuer,
            } => break (stream, code, state, issuer),
        }
    };

    let exchange = manager
        .exchange_code_for_token_with_issuer(&code, &state, issuer.as_deref())
        .await;
    let (status, message) = match &exchange {
        Ok(_) => (
            "200 OK",
            "MCP authorization complete. You can close this window.",
        ),
        Err(_) => (
            "400 Bad Request",
            "MCP authorization failed. Return to the terminal for details.",
        ),
    };
    write_oauth_callback_response(&mut callback_stream, status, message).await;
    exchange.map_err(|e| {
        let secret_hint = if oauth_client_secret(oauth).is_none() {
            " The authorization server may require a confidential client secret; configure oauth.clientSecretEnv."
        } else {
            ""
        };
        format!("MCP OAuth token exchange failed: {e}.{secret_hint}")
    })?;

    Ok(format!("authorized MCP server '{server_name}'"))
}

pub async fn logout(settings: &AppSettings, server_name: &str) -> Result<String, String> {
    if !settings.mcp_servers.contains_key(server_name) {
        return Err(format!("MCP server '{server_name}' is not configured"));
    }
    FileCredentialStore::for_server(server_name)
        .clear()
        .await
        .map_err(|e| format!("clear MCP OAuth credentials: {e}"))?;
    Ok(format!("cleared MCP OAuth credentials for '{server_name}'"))
}

/// Connect every configured MCP server, discover its tools, and return plugins
/// ready for registration. One broken server does not disable the others.
pub async fn connect_configured(settings: &AppSettings) -> Vec<Box<dyn Plugin>> {
    let mut plugins = Vec::new();
    for (name, config) in &settings.mcp_servers {
        let connection = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            connect_one(name, config),
        )
        .await;
        match connection {
            Ok(Ok(mut discovered)) => {
                log::info!(
                    "mcp: connected server '{}' and discovered {} tools",
                    name,
                    discovered.len()
                );
                plugins.append(&mut discovered);
            }
            Ok(Err(error)) => log::warn!("mcp: {error}"),
            Err(_) => log::warn!("mcp: server '{name}' connection timed out after 60s"),
        }
    }
    plugins
}

#[cfg(test)]
mod tests {
    use rmcp::{model::Tool, transport::CredentialStore};
    use serde_json::{Map, Value};

    use crate::settings::McpOAuthConfig;

    fn oauth_with(client_id: &str) -> Option<McpOAuthConfig> {
        Some(McpOAuthConfig {
            client_id: client_id.to_string(),
            client_secret: String::new(),
            client_secret_env: String::new(),
            scopes: Vec::new(),
        })
    }

    /// The whole point of the feature: a server configured with no `oauth`
    /// block must be registrable rather than rejected outright.
    #[test]
    fn missing_oauth_block_needs_registration() {
        assert!(super::needs_registration(&None));
    }

    /// A present-but-empty `client_id` carries no more credentials than an
    /// absent block, so it must take the same registration path instead of
    /// being sent to the authorization server as a blank client.
    #[test]
    fn blank_client_id_needs_registration() {
        assert!(super::needs_registration(&oauth_with("")));
        assert!(super::needs_registration(&oauth_with("   ")));
    }

    #[test]
    fn configured_client_id_skips_registration() {
        assert!(!super::needs_registration(&oauth_with("abc123")));
    }

    #[test]
    fn configured_scopes_take_precedence_over_advertised() {
        let configured = vec!["custom".to_string()];
        let advertised = vec!["openid".to_string(), "email".to_string()];
        assert_eq!(
            super::resolve_scopes(&configured, &advertised),
            vec!["custom".to_string()]
        );
    }

    /// With nothing configured, a client that has never seen the server can
    /// only ask for what the metadata advertises.
    #[test]
    fn empty_configured_scopes_fall_back_to_advertised() {
        let advertised = vec!["openid".to_string(), "groups".to_string()];
        assert_eq!(super::resolve_scopes(&[], &advertised), advertised);
    }

    #[test]
    fn absent_scopes_everywhere_yields_empty() {
        assert!(super::resolve_scopes(&[], &[]).is_empty());
    }
    use std::sync::Arc;

    #[test]
    fn oauth_callback_rejects_stray_method_and_path_without_consuming_login() {
        assert!(matches!(
            super::parse_oauth_callback_request(
                "POST /oauth/callback?code=c&state=s HTTP/1.1\r\n\r\n"
            ),
            super::OAuthCallbackRequest::Ignore {
                status: "405 Method Not Allowed",
                ..
            }
        ));
        assert!(matches!(
            super::parse_oauth_callback_request("GET /favicon.ico HTTP/1.1\r\n\r\n"),
            super::OAuthCallbackRequest::Ignore {
                status: "404 Not Found",
                ..
            }
        ));
    }

    #[test]
    fn oauth_callback_reports_provider_errors_and_accepts_valid_code_state() {
        assert!(matches!(
            super::parse_oauth_callback_request(
                "GET /oauth/callback?error=access_denied HTTP/1.1\r\n\r\n"
            ),
            super::OAuthCallbackRequest::Failure(message)
                if message.contains("access_denied")
        ));
        match super::parse_oauth_callback_request(
            "GET /oauth/callback?code=abc%20123&state=csrf&iss=https%3A%2F%2Fissuer.example HTTP/1.1\r\n\r\n",
        ) {
            super::OAuthCallbackRequest::Authorization {
                code,
                state,
                issuer,
            } => {
                assert_eq!(code, "abc 123");
                assert_eq!(state, "csrf");
                assert_eq!(issuer.as_deref(), Some("https://issuer.example"));
            }
            _ => panic!("valid callback was rejected"),
        }
    }

    #[tokio::test]
    async fn oauth_callback_writer_always_returns_an_http_response() {
        use tokio::io::AsyncReadExt as _;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let client = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).await.unwrap();
            response
        });
        let (mut server, _) = listener.accept().await.unwrap();
        super::write_oauth_callback_response(
            &mut server,
            "400 Bad Request",
            "MCP OAuth callback omitted state",
        )
        .await;
        let response = client.await.unwrap();
        assert!(response.starts_with("HTTP/1.1 400 Bad Request\r\n"));
        assert!(response.ends_with("MCP OAuth callback omitted state"));
    }

    #[tokio::test]
    async fn oauth_credential_store_overwrites_atomically_and_keeps_private_mode() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("credential.json");
        let store = super::FileCredentialStore { path: path.clone() };
        store
            .save(rmcp::transport::StoredCredentials::new(
                "client-one".into(),
                None,
                vec![],
                Some(1),
            ))
            .await
            .unwrap();
        store
            .save(rmcp::transport::StoredCredentials::new(
                "client-two".into(),
                None,
                vec![],
                Some(2),
            ))
            .await
            .unwrap();
        let loaded = store.load().await.unwrap().unwrap();
        assert_eq!(loaded.client_id, "client-two");
        let leftovers: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(leftovers.is_empty());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[tokio::test]
    async fn http_mcp_discovers_and_executes_remote_tool_end_to_end() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut bytes = Vec::new();
                    let mut chunk = [0u8; 4096];
                    let header_end;
                    loop {
                        let Ok(n) = stream.read(&mut chunk).await else {
                            return;
                        };
                        if n == 0 {
                            return;
                        }
                        bytes.extend_from_slice(&chunk[..n]);
                        if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                            header_end = pos + 4;
                            break;
                        }
                    }
                    let headers = String::from_utf8_lossy(&bytes[..header_end]);
                    let content_length = headers
                        .lines()
                        .filter_map(|line| line.split_once(':'))
                        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    while bytes.len() < header_end + content_length {
                        let Ok(n) = stream.read(&mut chunk).await else {
                            return;
                        };
                        if n == 0 {
                            break;
                        }
                        bytes.extend_from_slice(&chunk[..n]);
                    }
                    let body: serde_json::Value =
                        serde_json::from_slice(&bytes[header_end..header_end + content_length])
                            .unwrap();
                    let id = body.get("id").cloned();
                    let method = body["method"].as_str().unwrap_or("");
                    if id.is_none() {
                        let response = "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(response.as_bytes()).await;
                        return;
                    }
                    let result = match method {
                        "initialize" => serde_json::json!({
                            "protocolVersion": body["params"]["protocolVersion"],
                            "capabilities": {"tools": {}},
                            "serverInfo": {"name": "mock-mcp", "version": "1.0"}
                        }),
                        "tools/list" => serde_json::json!({
                            "tools": [{
                                "name": "echo-tool",
                                "description": "Echo an input value",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {"value": {"type": "string"}},
                                    "required": ["value"]
                                }
                            }]
                        }),
                        "tools/call" => serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": format!("echo:{}", body["params"]["arguments"]["value"].as_str().unwrap_or(""))
                            }],
                            "isError": false
                        }),
                        _ => serde_json::json!({}),
                    };
                    let response_body = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id.unwrap(),
                        "result": result
                    })
                    .to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });

        let config = crate::settings::McpServerConfig {
            transport_type: "http".into(),
            url: format!("http://127.0.0.1:{port}/mcp"),
            ..Default::default()
        };
        let plugins = super::connect_one("mock", &config).await.unwrap();
        assert_eq!(plugins.len(), 1);
        let schema = plugins[0].tool_schema().unwrap();
        assert_eq!(schema["function"]["name"], "mcp_mock_echo_tool");
        let output = plugins[0]
            .execute_tool(serde_json::json!({"value": "hello"}))
            .await;
        assert!(output.contains("echo:hello"), "unexpected output: {output}");
    }

    /// A newline-delimited JSON-RPC MCP server, small enough to inline. stdio
    /// transports frame one JSON object per line.
    const STDIO_MOCK_SERVER: &str = r#"
import json, os, sys
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    message = json.loads(line)
    if "id" not in message:
        continue
    method = message.get("method", "")
    if method == "initialize":
        result = {
            "protocolVersion": message["params"]["protocolVersion"],
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "stdio-mock", "version": "1.0"},
        }
    elif method == "tools/list":
        result = {"tools": [{
            "name": "echo-tool",
            "description": "Echo an input value",
            "inputSchema": {
                "type": "object",
                "properties": {"value": {"type": "string"}},
                "required": ["value"],
            },
        }]}
    elif method == "tools/call":
        value = message["params"]["arguments"].get("value", "")
        result = {
            "content": [{"type": "text", "text": "echo:%s:%s" % (value, os.environ.get("MOCK_SUFFIX", ""))}],
            "isError": False,
        }
    else:
        result = {}
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": result}) + "\n")
    sys.stdout.flush()
"#;

    #[tokio::test]
    async fn stdio_server_discovers_tools_and_receives_configured_env() {
        if std::process::Command::new("python3")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_err()
        {
            eprintln!("skipping: python3 is unavailable");
            return;
        }

        let config = crate::settings::McpServerConfig {
            transport_type: "stdio".into(),
            command: "python3".into(),
            args: vec!["-c".into(), STDIO_MOCK_SERVER.into()],
            env: [("MOCK_SUFFIX".to_string(), "from-env".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };

        let plugins = super::connect_one("localmock", &config).await.unwrap();
        assert_eq!(plugins.len(), 1);
        let schema = plugins[0].tool_schema().unwrap();
        assert_eq!(schema["function"]["name"], "mcp_localmock_echo_tool");
        let output = plugins[0]
            .execute_tool(serde_json::json!({"value": "hello"}))
            .await;
        assert!(
            output.contains("echo:hello:from-env"),
            "unexpected output: {output}"
        );
    }

    #[tokio::test]
    async fn stdio_server_without_command_is_rejected() {
        let config = crate::settings::McpServerConfig {
            transport_type: "stdio".into(),
            ..Default::default()
        };
        let Err(error) = super::connect_one("broken", &config).await else {
            panic!("stdio server without a command must not connect");
        };
        assert!(error.contains("no command"), "unexpected error: {error}");
    }

    #[tokio::test]
    async fn unknown_transport_type_is_rejected() {
        let config = crate::settings::McpServerConfig {
            transport_type: "carrier-pigeon".into(),
            url: "https://example.com/mcp".into(),
            ..Default::default()
        };
        let Err(error) = super::connect_one("odd", &config).await else {
            panic!("unknown transport types must not connect");
        };
        assert!(
            error.contains("'http' and 'stdio'"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn mcp_tool_schema_is_namespaced_and_preserves_json_schema() {
        let mut input = Map::new();
        input.insert("type".into(), Value::String("object".into()));
        input.insert(
            "properties".into(),
            serde_json::json!({"query": {"type": "string"}}),
        );
        let tool = Tool::new("search-messages", "Search Slack messages", Arc::new(input));

        let schema = super::tool_to_openai_schema("slackBlizzard", &tool);

        assert_eq!(
            schema["function"]["name"],
            "mcp_slackBlizzard_search_messages"
        );
        assert_eq!(schema["function"]["description"], "Search Slack messages");
        assert_eq!(
            schema["function"]["parameters"]["properties"]["query"]["type"],
            "string"
        );
    }
}

#[cfg(test)]
mod remap_input_tests {
    use super::remap_input_placeholder;
    use serde_json::json;

    fn schema(params: serde_json::Value) -> serde_json::Value {
        json!({"type":"function","function":{"name":"t","parameters":params}})
    }
    fn args(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        v.as_object().unwrap().clone()
    }

    /// Regression: free-form text callers send `{"input": "..."}`, but no MCP
    /// tool declares `input`. It must land on the tool's required parameter
    /// (e.g. workiq `ask` -> `question`) or the server rejects the call.
    #[test]
    fn maps_input_to_sole_required_string() {
        let s = schema(json!({
            "properties": {"question": {"type":"string"}, "agentId": {"type":["string","null"]}},
            "required": ["question"]
        }));
        assert_eq!(
            remap_input_placeholder(&args(json!({"input":"show me upcoming meetings"})), &s),
            Some("question".to_string())
        );
    }

    #[test]
    fn maps_to_single_string_property_when_nothing_required() {
        let s = schema(json!({"properties": {"query": {"type":"string"}}}));
        assert_eq!(
            remap_input_placeholder(&args(json!({"input":"hi"})), &s),
            Some("query".to_string())
        );
    }

    #[test]
    fn declines_when_multiple_required_params() {
        let s = schema(json!({
            "properties": {"a": {"type":"string"}, "b": {"type":"string"}},
            "required": ["a","b"]
        }));
        assert_eq!(remap_input_placeholder(&args(json!({"input":"hi"})), &s), None);
    }

    #[test]
    fn declines_when_required_param_is_not_a_string() {
        let s = schema(json!({
            "properties": {"entityUrls": {"type":"array"}},
            "required": ["entityUrls"]
        }));
        assert_eq!(remap_input_placeholder(&args(json!({"input":"hi"})), &s), None);
    }

    #[test]
    fn declines_when_tool_really_accepts_input() {
        let s = schema(json!({
            "properties": {"input": {"type":"string"}},
            "required": ["input"]
        }));
        assert_eq!(remap_input_placeholder(&args(json!({"input":"hi"})), &s), None);
    }

    #[test]
    fn leaves_real_named_args_untouched() {
        let s = schema(json!({
            "properties": {"question": {"type":"string"}},
            "required": ["question"]
        }));
        assert_eq!(
            remap_input_placeholder(&args(json!({"question":"hi"})), &s),
            None
        );
    }
}
