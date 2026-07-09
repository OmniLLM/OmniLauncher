use crate::settings::{provider_caps, Provider, ProviderKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRequest {
    pub chat_url: String,
    pub headers: Vec<(String, String)>,
    pub model: String,
}

impl ResolvedRequest {
    pub fn openai_compatible(base_url: &str, api_key: &str, model: &str) -> Self {
        let mut headers = Vec::new();
        if !api_key.trim().is_empty() {
            headers.push((
                "Authorization".to_string(),
                format!("Bearer {}", api_key.trim()),
            ));
        }
        Self {
            chat_url: format!("{}/v1/chat/completions", base_url.trim_end_matches('/')),
            headers,
            model: model.to_string(),
        }
    }
}

pub fn resolve_provider(provider: &Provider) -> Result<ResolvedRequest, String> {
    let caps = provider_caps(provider.kind);
    if caps.requires_api_key && provider.api_key.trim().is_empty() {
        log::warn!(
            "provider '{}' ({}) has no API key configured",
            provider.name,
            provider.kind
        );
    }

    match provider.kind {
        ProviderKind::Custom => Ok(ResolvedRequest::openai_compatible(
            &provider.base_url,
            &provider.api_key,
            &provider.model,
        )),
        ProviderKind::AzureFoundry => {
            let mut headers = Vec::new();
            if !provider.api_key.trim().is_empty() {
                headers.push((
                    "Authorization".to_string(),
                    format!("Bearer {}", provider.api_key.trim()),
                ));
            }
            Ok(ResolvedRequest {
                chat_url: format!(
                    "{}/chat/completions",
                    provider.base_url.trim_end_matches('/')
                ),
                headers,
                model: provider.model.clone(),
            })
        }
        ProviderKind::GithubCopilot => {
            let token = provider.copilot_token.trim();
            if token.is_empty() {
                return Err(format!(
                    "provider '{}' needs GitHub Copilot auth; run provider login once token flow is configured",
                    provider.name
                ));
            }
            let base = copilot_base_url(provider);
            let mut headers = vec![
                ("Authorization".to_string(), format!("Bearer {token}")),
                (
                    "copilot-integration-id".to_string(),
                    "vscode-chat".to_string(),
                ),
                ("Editor-Version".to_string(), "vscode/1.83.1".to_string()),
                (
                    "Editor-Plugin-Version".to_string(),
                    "copilot-chat/0.26.7".to_string(),
                ),
                (
                    "User-Agent".to_string(),
                    "GitHubCopilotChat/0.26.7".to_string(),
                ),
                (
                    "OpenAI-Intent".to_string(),
                    "conversation-panel".to_string(),
                ),
                ("X-Github-Api-Version".to_string(), "2025-04-01".to_string()),
                (
                    "X-Vscode-User-Agent-Library-Version".to_string(),
                    "electron-fetch".to_string(),
                ),
            ];
            headers.retain(|(_, v)| !v.trim().is_empty());
            Ok(ResolvedRequest {
                chat_url: format!("{}/chat/completions", base.trim_end_matches('/')),
                headers,
                model: provider.model.clone(),
            })
        }
    }
}

fn copilot_base_url(provider: &Provider) -> String {
    let enterprise = provider.copilot_enterprise_url.trim();
    if enterprise.is_empty() {
        return "https://api.githubcopilot.com".to_string();
    }
    let host = enterprise
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    format!("https://copilot-api.{host}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_provider_resolves_to_v1_chat_completions() {
        let p = Provider {
            kind: ProviderKind::Custom,
            base_url: "http://localhost:5000/".to_string(),
            api_key: "sk".to_string(),
            model: "gpt".to_string(),
            ..Provider::default()
        };
        let r = resolve_provider(&p).unwrap();
        assert_eq!(r.chat_url, "http://localhost:5000/v1/chat/completions");
        assert_eq!(r.model, "gpt");
        assert_eq!(
            r.headers,
            vec![("Authorization".to_string(), "Bearer sk".to_string())]
        );
    }

    #[test]
    fn azure_foundry_resolves_without_v1_suffix() {
        let p = Provider {
            kind: ProviderKind::AzureFoundry,
            base_url: "https://example.services.ai.azure.com/models".to_string(),
            api_key: "az".to_string(),
            model: "gpt-5".to_string(),
            ..Provider::default()
        };
        let r = resolve_provider(&p).unwrap();
        assert_eq!(
            r.chat_url,
            "https://example.services.ai.azure.com/models/chat/completions"
        );
        assert_eq!(
            r.headers,
            vec![("Authorization".to_string(), "Bearer az".to_string())]
        );
    }

    #[test]
    fn copilot_provider_requires_token_and_adds_editor_headers() {
        let missing = Provider {
            kind: ProviderKind::GithubCopilot,
            model: "gpt-4.1".to_string(),
            ..Provider::default()
        };
        assert!(resolve_provider(&missing).is_err());

        let p = Provider {
            kind: ProviderKind::GithubCopilot,
            copilot_token: "cop".to_string(),
            model: "gpt-4.1".to_string(),
            ..Provider::default()
        };
        let r = resolve_provider(&p).unwrap();
        assert_eq!(r.chat_url, "https://api.githubcopilot.com/chat/completions");
        assert!(r
            .headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer cop"));
        assert!(r.headers.iter().any(|(k, _)| k == "copilot-integration-id"));
    }
}
