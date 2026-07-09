use serde::Serialize;

use crate::AppSettings;

#[derive(Debug, Clone, Serialize)]
struct HubUpsertRequest {
    name: String,
    base_url: String,
    prefix: String,
    auth: HubAuth,
}

#[derive(Debug, Clone, Serialize)]
struct HubAuth {
    scheme: String,
    token: String,
}

fn default_upstream_base_url(settings: &AppSettings) -> String {
    format!("http://127.0.0.1:{}", settings.a2a_port)
}

fn upstream_base_url(settings: &AppSettings) -> String {
    let configured = settings.a2a_public_url.trim();
    if !configured.is_empty() {
        configured.trim_end_matches('/').to_string()
    } else {
        default_upstream_base_url(settings)
    }
}

fn build_upsert_request(settings: &AppSettings, a2a_token: &str) -> Result<HubUpsertRequest, String> {
    let token = a2a_token.trim();
    if token.is_empty() {
        return Err("A2A token is empty; enable A2A once so a token is generated".to_string());
    }

    let name = settings.a2a_hub_upstream_name.trim();
    if name.is_empty() {
        return Err("A2A hub upstream name is empty".to_string());
    }

    Ok(HubUpsertRequest {
        name: name.to_string(),
        base_url: upstream_base_url(settings),
        prefix: settings.a2a_hub_prefix.trim().to_string(),
        auth: HubAuth {
            scheme: "bearer".to_string(),
            token: token.to_string(),
        },
    })
}

/// Register (or update) this OmniLauncher A2A server as an upstream in
/// omni-agent-hub. The hub must expose its admin API and the caller must provide
/// `settings.a2a_hub_admin_key` (usually via OMNILAUNCHER_A2A_HUB_ADMIN_KEY).
pub async fn register_with_hub(settings: &AppSettings, a2a_token: &str) -> Result<(), String> {
    let hub_url = settings.a2a_hub_url.trim().trim_end_matches('/');
    if hub_url.is_empty() {
        return Err("A2A hub URL is empty".to_string());
    }
    let admin_key = settings.a2a_hub_admin_key.trim();
    if admin_key.is_empty() {
        return Err(
            "A2A hub admin key is empty; set OMNILAUNCHER_A2A_HUB_ADMIN_KEY or --a2a-hub-admin-key"
                .to_string(),
        );
    }

    let payload = build_upsert_request(settings, a2a_token)?;
    let endpoint = format!("{hub_url}/admin/upstreams/upsert");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|err| format!("building hub registration client: {err}"))?;

    let response = client
        .post(&endpoint)
        .bearer_auth(admin_key)
        .json(&payload)
        .send()
        .await
        .map_err(|err| format!("registering upstream with hub at {endpoint}: {err}"))?;

    let status = response.status();
    if status.is_success() {
        log::info!(
            "a2a: registered upstream '{}' with hub {} as {}",
            payload.name,
            hub_url,
            payload.base_url
        );
        return Ok(());
    }

    let body = response.text().await.unwrap_or_default();
    Err(format!(
        "hub registration failed: HTTP {} from {}: {}",
        status.as_u16(),
        endpoint,
        body.trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_upsert_request_defaults_to_loopback_a2a_url() {
        let mut settings = AppSettings::default();
        settings.a2a_port = 19999;
        settings.a2a_hub_upstream_name = "omnilauncher".to_string();
        settings.a2a_hub_prefix = "@ol".to_string();

        let req = build_upsert_request(&settings, "tok").unwrap();

        assert_eq!(req.name, "omnilauncher");
        assert_eq!(req.base_url, "http://127.0.0.1:19999");
        assert_eq!(req.prefix, "@ol");
        assert_eq!(req.auth.scheme, "bearer");
        assert_eq!(req.auth.token, "tok");
    }

    #[test]
    fn build_upsert_request_uses_public_url_when_configured() {
        let mut settings = AppSettings::default();
        settings.a2a_public_url = "https://agent.example.com/".to_string();
        settings.a2a_hub_upstream_name = "desktop-agent".to_string();

        let req = build_upsert_request(&settings, "tok").unwrap();

        assert_eq!(req.base_url, "https://agent.example.com");
    }

    #[test]
    fn build_upsert_request_rejects_empty_token() {
        let settings = AppSettings::default();
        assert!(build_upsert_request(&settings, " ").is_err());
    }
}
