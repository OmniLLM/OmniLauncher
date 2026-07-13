//! GitHub Copilot OAuth (device-code flow) and token exchange.
//!
//! Ported from the sibling Go project `omnillm`
//! (`internal/services/github/github.go`, `internal/providers/copilot`).
//!
//! Flow:
//!   1. `get_device_code()`     — POST github.com/login/device/code
//!   2. user authorizes in browser using the returned user_code
//!   3. `poll_access_token()`   — POST github.com/login/oauth/access_token until
//!      a long-lived GitHub access token is issued
//!   4. `get_copilot_token()`   — GET api.github.com/copilot_internal/v2/token to
//!      exchange the GitHub token for a short-lived Copilot API token
//!
//! The Copilot token is refreshed on demand when it is missing or within 5
//! minutes of expiry via `refresh_copilot_token_if_needed`.

use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::settings::Provider;

// GitHub Copilot Chat OAuth app + editor identity headers. Kept in sync with the
// header values hard-coded in `crate::ai::provider`.
const CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const APP_SCOPES: &str = "read:user";
const API_VERSION: &str = "2025-04-01";
const USER_AGENT: &str = "GitHubCopilotChat/0.26.7";
const EDITOR_VERSION: &str = "vscode/1.83.1";
const PLUGIN_VERSION: &str = "copilot-chat/0.26.7";

const GITHUB_BASE_URL: &str = "https://github.com";
const GITHUB_API_BASE_URL: &str = "https://api.github.com";

/// Refresh the Copilot token when within this many seconds of expiry.
pub const REFRESH_SKEW_SECS: i64 = 300;

/// Shared HTTP client with a 15s timeout, matching `plugins::web_fetch`.
static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_default()
});

/// Response from GitHub's device-code endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub expires_in: u64,
    #[serde(default)]
    pub interval: u64,
}

/// Response from GitHub's OAuth token endpoint (device-code grant).
///
/// When the GitHub App has "Expiration of user authorization tokens" enabled,
/// GitHub returns an expiring `access_token` (valid `expires_in` seconds)
/// together with a long-lived `refresh_token`. Exchanging the refresh token
/// (`grant_type=refresh_token`) mints a new access token without a fresh
/// device-code login, giving unattended, indefinite operation. When expiration
/// is disabled these fields are empty/zero and the access token never expires.
#[derive(Debug, Clone, Deserialize)]
pub struct AccessTokenResponse {
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub expires_in: i64,
    #[serde(default)]
    pub refresh_token_expires_in: i64,
    #[serde(default)]
    pub error: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CopilotRefreshOutcome {
    pub outer_rotated: bool,
    pub inner_refreshed: bool,
}

impl CopilotRefreshOutcome {
    pub fn changed(self) -> bool {
        self.outer_rotated || self.inner_refreshed
    }
}

/// Response from the Copilot internal token API.
#[derive(Debug, Clone, Deserialize)]
pub struct CopilotTokenResponse {
    pub token: String,
    #[serde(default)]
    pub expires_at: i64,
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Compute the Copilot API base URL for a provider, honouring an optional
/// GitHub Enterprise host. Shared with `crate::ai::provider`.
pub fn copilot_base_url(enterprise_url: &str) -> String {
    let enterprise = enterprise_url.trim();
    if enterprise.is_empty() {
        return "https://api.githubcopilot.com".to_string();
    }
    let host = enterprise
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    format!("https://copilot-api.{host}")
}

/// Initiate the GitHub OAuth device-code flow.
pub async fn get_device_code() -> Result<DeviceCode, String> {
    let resp = CLIENT
        .post(format!("{GITHUB_BASE_URL}/login/device/code"))
        .header("Accept", "application/json")
        .json(&serde_json::json!({ "client_id": CLIENT_ID, "scope": APP_SCOPES }))
        .send()
        .await
        .map_err(|e| format!("device code request failed: {e}"))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("failed to read device code response: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "device code request failed with status {status}: {body}"
        ));
    }
    serde_json::from_str::<DeviceCode>(&body)
        .map_err(|e| format!("failed to decode device code response: {e}"))
}

/// Poll for the access token after the user authorizes the device.
///
/// Returns the full token response so callers can persist a `refresh_token`
/// (and its expiry) when the GitHub App issues expiring user tokens.
pub async fn poll_access_token(device: &DeviceCode) -> Result<AccessTokenResponse, String> {
    // GitHub asks callers to wait `interval` seconds between polls; add a small
    // cushion to avoid `slow_down`.
    let mut interval = Duration::from_secs(device.interval.max(1) + 1);
    let expires_in = if device.expires_in == 0 {
        900
    } else {
        device.expires_in
    };
    let deadline = SystemTime::now() + Duration::from_secs(expires_in);

    loop {
        if SystemTime::now() >= deadline {
            return Err("device code expired before authorization".to_string());
        }

        let result = CLIENT
            .post(format!("{GITHUB_BASE_URL}/login/oauth/access_token"))
            .header("Accept", "application/json")
            .json(&serde_json::json!({
                "client_id": CLIENT_ID,
                "device_code": device.device_code,
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
            }))
            .send()
            .await;

        match result {
            Ok(resp) => match resp.json::<AccessTokenResponse>().await {
                Ok(token) if !token.access_token.is_empty() => return Ok(token),
                Ok(token) => match token.error.as_str() {
                    "authorization_pending" | "" => {}
                    "slow_down" => interval += Duration::from_secs(5),
                    "expired_token" => {
                        return Err("device code expired, please try again".to_string())
                    }
                    "access_denied" => return Err("GitHub authorization was denied".to_string()),
                    error => return Err(format!("GitHub device authorization failed: {error}")),
                },
                Err(e) => {
                    log::warn!("copilot auth: failed to decode token response, retrying: {e}");
                }
            },
            Err(e) => {
                log::warn!("copilot auth: token poll request failed, retrying: {e}");
            }
        }

        tokio::time::sleep(interval).await;
    }
}

/// Exchange a long-lived GitHub OAuth refresh token for a fresh access token
/// (and a rotated refresh token) via the device-flow refresh grant. This renews
/// the *outer* GitHub token without a new device-code login — only possible when
/// the GitHub App has user-token expiration enabled (otherwise no refresh token
/// is ever issued).
///
/// GitHub rotates the refresh token on every successful exchange, so callers
/// MUST persist the returned `refresh_token`.
pub async fn refresh_access_token(refresh_token: &str) -> Result<AccessTokenResponse, String> {
    if refresh_token.trim().is_empty() {
        return Err("no refresh token available".to_string());
    }

    let resp = CLIENT
        .post(format!("{GITHUB_BASE_URL}/login/oauth/access_token"))
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "client_id": CLIENT_ID,
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
        }))
        .send()
        .await
        .map_err(|e| format!("access token refresh request failed: {e}"))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("failed to read access token refresh response: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "access token refresh failed with status {status}: {body}"
        ));
    }
    let parsed = serde_json::from_str::<AccessTokenResponse>(&body)
        .map_err(|e| format!("failed to decode access token refresh response: {e}"))?;
    if parsed.access_token.is_empty() {
        return Err(format!(
            "access token refresh returned no token (error={:?}): {body}",
            parsed.error
        ));
    }
    Ok(parsed)
}

/// Exchange a GitHub access token for a short-lived Copilot API token.
pub async fn get_copilot_token(
    github_token: &str,
    _enterprise_url: &str,
) -> Result<CopilotTokenResponse, String> {
    let resp = CLIENT
        .get(format!("{GITHUB_API_BASE_URL}/copilot_internal/v2/token"))
        .header("Authorization", format!("token {github_token}"))
        .header("Accept", "application/json")
        .header("Editor-Version", EDITOR_VERSION)
        .header("Editor-Plugin-Version", PLUGIN_VERSION)
        .header("User-Agent", USER_AGENT)
        .header("X-Github-Api-Version", API_VERSION)
        .send()
        .await
        .map_err(|e| format!("copilot token request failed: {e}"))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("failed to read copilot token response: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "copilot token request failed with status {status}: {body}"
        ));
    }
    serde_json::from_str::<CopilotTokenResponse>(&body)
        .map_err(|e| format!("failed to decode copilot token response: {e}"))
}

/// Fetch basic GitHub user info for the authenticated token.
pub async fn get_user(github_token: &str) -> Result<serde_json::Value, String> {
    let resp = CLIENT
        .get(format!("{GITHUB_API_BASE_URL}/user"))
        .header("Authorization", format!("token {github_token}"))
        .header("Accept", "application/json")
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| format!("user info request failed: {e}"))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("failed to read user response: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "user info request failed with status {status}: {body}"
        ));
    }
    serde_json::from_str::<serde_json::Value>(&body)
        .map_err(|e| format!("failed to decode user response: {e}"))
}

/// Build a human-friendly display name for a Copilot provider from a `/user`
/// response. Priority: "name · email" → "name · login" → "email" → "login".
pub fn copilot_provider_name(user: &serde_json::Value) -> String {
    let login = user.get("login").and_then(|v| v.as_str()).unwrap_or("");
    let email = user
        .get("email")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| user.get("notification_email").and_then(|v| v.as_str()))
        .unwrap_or("");
    let real_name = user.get("name").and_then(|v| v.as_str()).unwrap_or("");

    match (real_name, email, login) {
        (n, e, _) if !n.is_empty() && !e.is_empty() => format!("GitHub Copilot ({n} · {e})"),
        (n, _, l) if !n.is_empty() && !l.is_empty() => format!("GitHub Copilot ({n} · {l})"),
        (_, e, _) if !e.is_empty() => format!("GitHub Copilot ({e})"),
        (_, _, l) if !l.is_empty() => format!("GitHub Copilot ({l})"),
        _ => "GitHub Copilot".to_string(),
    }
}

fn absolute_expiry(now: i64, expires_in: i64) -> i64 {
    if expires_in > 0 {
        now + expires_in
    } else {
        0
    }
}

fn apply_access_token_response(provider: &mut Provider, fresh: AccessTokenResponse, now: i64) {
    provider.copilot_github_token = fresh.access_token;
    if !fresh.refresh_token.is_empty() {
        provider.copilot_github_refresh_token = fresh.refresh_token;
    }
    provider.copilot_github_token_expiry = absolute_expiry(now, fresh.expires_in);
    provider.copilot_github_refresh_token_expiry =
        absolute_expiry(now, fresh.refresh_token_expires_in);
}

pub(crate) fn is_bad_credentials(error: &str) -> bool {
    error.contains("401 Unauthorized") && error.contains("Bad credentials")
}

async fn rotate_github_token(provider: &mut Provider, force: bool) -> Result<bool, String> {
    if provider.copilot_github_refresh_token.trim().is_empty() {
        return Ok(false);
    }
    if !force
        && (provider.copilot_github_token_expiry == 0
            || now_unix() <= provider.copilot_github_token_expiry - REFRESH_SKEW_SECS)
    {
        return Ok(false);
    }
    if provider.copilot_github_refresh_token_expiry > 0
        && now_unix() >= provider.copilot_github_refresh_token_expiry
    {
        return Err("GitHub Copilot authorization expired; run `ol providers login`".to_string());
    }

    let fresh = refresh_access_token(&provider.copilot_github_refresh_token).await?;
    apply_access_token_response(provider, fresh, now_unix());
    Ok(true)
}

/// Rotate the long-lived GitHub OAuth token using its refresh token when the
/// token is near expiry.
pub async fn rotate_github_token_if_needed(provider: &mut Provider) -> Result<bool, String> {
    rotate_github_token(provider, false).await
}

/// Refresh a provider's outer GitHub credential and/or inner Copilot token.
pub async fn refresh_copilot_token_if_needed(
    provider: &mut Provider,
) -> Result<CopilotRefreshOutcome, String> {
    let mut outer_rotated = rotate_github_token_if_needed(provider).await?;

    if provider.copilot_github_token.trim().is_empty() {
        return Ok(CopilotRefreshOutcome {
            outer_rotated,
            inner_refreshed: false,
        });
    }

    let needs_refresh = provider.copilot_token.trim().is_empty()
        || provider.copilot_token_expiry == 0
        || now_unix() > provider.copilot_token_expiry - REFRESH_SKEW_SECS;
    if !needs_refresh {
        return Ok(CopilotRefreshOutcome {
            outer_rotated,
            inner_refreshed: false,
        });
    }

    let fresh = match get_copilot_token(
        &provider.copilot_github_token,
        &provider.copilot_enterprise_url,
    )
    .await
    {
        Ok(token) => token,
        Err(error)
            if is_bad_credentials(&error)
                && !outer_rotated
                && !provider.copilot_github_refresh_token.trim().is_empty() =>
        {
            outer_rotated = rotate_github_token(provider, true).await.map_err(|refresh_error| {
                format!(
                    "GitHub Copilot authorization is invalid and could not be renewed: {refresh_error}; run `ol providers login`"
                )
            })?;
            get_copilot_token(
                &provider.copilot_github_token,
                &provider.copilot_enterprise_url,
            )
            .await?
        }
        Err(error) => return Err(error),
    };
    provider.copilot_token = fresh.token;
    provider.copilot_token_expiry = fresh.expires_at;
    Ok(CopilotRefreshOutcome {
        outer_rotated,
        inner_refreshed: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_name_prefers_name_and_email() {
        let user = serde_json::json!({ "name": "Ada", "email": "ada@x.com", "login": "ada99" });
        assert_eq!(
            copilot_provider_name(&user),
            "GitHub Copilot (Ada · ada@x.com)"
        );
    }

    #[test]
    fn provider_name_falls_back_to_name_and_login() {
        let user = serde_json::json!({ "name": "Ada", "email": "", "login": "ada99" });
        assert_eq!(copilot_provider_name(&user), "GitHub Copilot (Ada · ada99)");
    }

    #[test]
    fn provider_name_uses_notification_email() {
        let user =
            serde_json::json!({ "name": "", "notification_email": "n@x.com", "login": "ada99" });
        assert_eq!(copilot_provider_name(&user), "GitHub Copilot (n@x.com)");
    }

    #[test]
    fn provider_name_falls_back_to_login() {
        let user = serde_json::json!({ "login": "ada99" });
        assert_eq!(copilot_provider_name(&user), "GitHub Copilot (ada99)");
    }

    #[test]
    fn provider_name_default() {
        let user = serde_json::json!({});
        assert_eq!(copilot_provider_name(&user), "GitHub Copilot");
    }

    #[test]
    fn base_url_defaults_to_public() {
        assert_eq!(copilot_base_url(""), "https://api.githubcopilot.com");
        assert_eq!(copilot_base_url("  "), "https://api.githubcopilot.com");
    }

    #[test]
    fn base_url_rewrites_enterprise_host() {
        assert_eq!(
            copilot_base_url("https://ghe.example.com/"),
            "https://copilot-api.ghe.example.com"
        );
        assert_eq!(
            copilot_base_url("ghe.example.com"),
            "https://copilot-api.ghe.example.com"
        );
    }

    #[test]
    fn refresh_skips_without_github_token() {
        let mut p = Provider {
            kind: crate::settings::ProviderKind::GithubCopilot,
            ..Provider::default()
        };
        let changed = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(refresh_copilot_token_if_needed(&mut p))
            .unwrap();
        assert!(!changed.changed());
    }

    #[test]
    fn access_token_response_updates_one_credential_generation() {
        let mut provider = Provider {
            copilot_github_token: "old-access".into(),
            copilot_github_refresh_token: "old-refresh".into(),
            ..Provider::default()
        };
        apply_access_token_response(
            &mut provider,
            AccessTokenResponse {
                access_token: "new-access".into(),
                refresh_token: "new-refresh".into(),
                expires_in: 3600,
                refresh_token_expires_in: 7200,
                error: String::new(),
            },
            100,
        );
        assert_eq!(provider.copilot_github_token, "new-access");
        assert_eq!(provider.copilot_github_refresh_token, "new-refresh");
        assert_eq!(provider.copilot_github_token_expiry, 3700);
        assert_eq!(provider.copilot_github_refresh_token_expiry, 7300);
    }

    #[test]
    fn access_token_response_preserves_omitted_refresh_token() {
        let mut provider = Provider {
            copilot_github_refresh_token: "keep-refresh".into(),
            ..Provider::default()
        };
        apply_access_token_response(
            &mut provider,
            AccessTokenResponse {
                access_token: "new-access".into(),
                refresh_token: String::new(),
                expires_in: 0,
                refresh_token_expires_in: 0,
                error: String::new(),
            },
            100,
        );
        assert_eq!(provider.copilot_github_refresh_token, "keep-refresh");
        assert_eq!(provider.copilot_github_token_expiry, 0);
        assert_eq!(provider.copilot_github_refresh_token_expiry, 0);
    }

    #[test]
    fn bad_credentials_detection_is_specific() {
        assert!(is_bad_credentials(
            "copilot token request failed with status 401 Unauthorized: {\"message\":\"Bad credentials\"}"
        ));
        assert!(!is_bad_credentials("500 Internal Server Error"));
    }
}
