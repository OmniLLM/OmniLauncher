use crate::plugins::{Plugin, Query, QueryResult};
use crate::settings::{load_settings, GitHubServer};
use async_trait::async_trait;
use reqwest::Client;
use std::sync::LazyLock;

static GH_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("OmniLauncher/1.0")
        .build()
        .unwrap_or_default()
});

pub struct GitHubPlugin;

// ── HTTP helper ───────────────────────────────────────────────────────────────

async fn gh_get(server: &GitHubServer, path: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}{}", server.effective_api_base(), path);
    let mut req = GH_CLIENT
        .get(&url)
        .header("Accept", "application/vnd.github+json");
    if let Some(tok) = server.resolve_token() {
        req = req.header("Authorization", format!("Bearer {}", tok));
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} — {}", resp.status(), url));
    }
    resp.json().await.map_err(|e| e.to_string())
}

// Convenience: gh_get on the first configured server matching a hostname, or first server.
fn pick_server(servers: &[GitHubServer], hostname: Option<&str>) -> Option<GitHubServer> {
    if servers.is_empty() {
        return None;
    }
    if let Some(h) = hostname {
        if let Some(s) = servers.iter().find(|s| s.hostname == h) {
            return Some(s.clone());
        }
    }
    Some(servers[0].clone())
}

// ── Plugin impl ───────────────────────────────────────────────────────────────

#[async_trait]
impl Plugin for GitHubPlugin {
    fn name(&self) -> &str {
        "github"
    }

    fn description(&self) -> &str {
        "GitHub: search repos, list issues/PRs across multiple servers"
    }

    fn keyword(&self) -> Option<&str> {
        Some("gh ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let term = q.raw.strip_prefix("gh ").unwrap_or("").trim();
        let settings = load_settings();

        // Show one set of shortcuts per configured server
        let servers: Vec<_> = if settings.github_servers.is_empty() {
            vec![GitHubServer {
                hostname: "github.com".to_string(),
                ..Default::default()
            }]
        } else {
            settings.github_servers.clone()
        };

        let mut results = vec![];
        for server in &servers {
            let host = if server.hostname.is_empty() {
                "github.com"
            } else {
                &server.hostname
            };
            let commands = [
                ("issues", "🔴", "Open issues"),
                ("prs", "🟢", "Open pull requests"),
                ("repos", "📁", "Repositories"),
                ("dashboard", "📊", "GitHub dashboard"),
            ];
            for (name, icon, desc) in &commands {
                if !term.is_empty() && !name.contains(term) {
                    continue;
                }
                results.push(QueryResult {
                    id: format!("github:{}:{}", host, name),
                    title: format!("{} — {}", desc, host),
                    subtitle: Some(format!("gh {} on {}", name, host)),
                    icon: Some(icon.to_string()),
                    score: 70,
                    action_type: "plugin_execute".to_string(),
                    action_data: format!("github:{}:{}", host, name),
                });
            }
        }
        results
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "github",
                "description": "GitHub API: list/search repos, issues, PRs. Supports multiple servers (github.com + GitHub Enterprise). Token resolved automatically via gh CLI auth or explicit config.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["list_servers", "list_repos", "list_issues", "list_prs", "search_repos", "get_issue", "get_pr"],
                            "description": "Action to perform. Use list_servers to see configured GitHub connections."
                        },
                        "hostname": {
                            "type": "string",
                            "description": "GitHub server hostname, e.g. 'github.com' or 'github.company.com'. Defaults to first configured server."
                        },
                        "org": { "type": "string", "description": "Organization or owner name" },
                        "repo": { "type": "string", "description": "Repository name (without owner)" },
                        "query": { "type": "string", "description": "Search query for search_repos" },
                        "number": { "type": "integer", "description": "Issue or PR number" },
                        "state": {
                            "type": "string",
                            "enum": ["open", "closed", "all"],
                            "description": "Filter by state (default: open)"
                        }
                    },
                    "required": ["action"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let action = args["action"].as_str().unwrap_or("");
        let hostname = args["hostname"].as_str();
        let org = args["org"].as_str().unwrap_or("");
        let repo = args["repo"].as_str().unwrap_or("");
        let state = args["state"].as_str().unwrap_or("open");

        let settings = load_settings();

        if action == "list_servers" {
            if settings.github_servers.is_empty() {
                return "No servers configured. Add github_servers in settings.json, or gh auth login first.".to_string();
            }
            return settings
                .github_servers
                .iter()
                .map(|s| {
                    let auth = if s.resolve_token().is_some() {
                        "✓ auth"
                    } else {
                        "✗ no token"
                    };
                    format!(
                        "• {} [{}] orgs: {}",
                        s.hostname,
                        auth,
                        if s.orgs.is_empty() {
                            "(none configured)".to_string()
                        } else {
                            s.orgs.join(", ")
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
        }

        let server = match pick_server(&settings.github_servers, hostname) {
            Some(s) => s,
            None => {
                // Fallback: use gh CLI for github.com
                GitHubServer {
                    hostname: "github.com".to_string(),
                    ..Default::default()
                }
            }
        };

        match action {
            "list_repos" => {
                let path = if org.is_empty() {
                    "/user/repos?sort=updated&per_page=30".to_string()
                } else {
                    format!("/orgs/{}/repos?sort=updated&per_page=30", org)
                };
                match gh_get(&server, &path).await {
                    Ok(v) => v
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .map(|r| {
                                    format!(
                                        "• {} — {} ⭐{}",
                                        r["full_name"].as_str().unwrap_or("?"),
                                        r["description"].as_str().unwrap_or(""),
                                        r["stargazers_count"].as_i64().unwrap_or(0)
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default(),
                    Err(e) => format!("Error: {}", e),
                }
            }
            "list_issues" => {
                let path = if org.is_empty() && repo.is_empty() {
                    format!("/issues?state={}&per_page=30", state)
                } else if repo.is_empty() {
                    format!("/orgs/{}/issues?state={}&per_page=30", org, state)
                } else {
                    let owner = if org.is_empty() { "me" } else { org };
                    format!(
                        "/repos/{}/{}/issues?state={}&per_page=30",
                        owner, repo, state
                    )
                };
                match gh_get(&server, &path).await {
                    Ok(v) => format_issues(&v),
                    Err(e) => format!("Error: {}", e),
                }
            }
            "list_prs" => {
                let path = if org.is_empty() && repo.is_empty() {
                    format!("/pulls?state={}&per_page=30", state)
                } else {
                    let owner = if org.is_empty() { "me" } else { org };
                    format!(
                        "/repos/{}/{}/pulls?state={}&per_page=30",
                        owner, repo, state
                    )
                };
                match gh_get(&server, &path).await {
                    Ok(v) => format_prs(&v),
                    Err(e) => format!("Error: {}", e),
                }
            }
            "search_repos" => {
                let q = args["query"].as_str().unwrap_or("");
                if q.is_empty() {
                    return "Error: query is required for search_repos".to_string();
                }
                let encoded: String = q
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                            c.to_string()
                        } else {
                            format!("%{:02X}", c as u32)
                        }
                    })
                    .collect();
                let path = format!("/search/repositories?q={}&per_page=20", encoded);
                match gh_get(&server, &path).await {
                    Ok(v) => v["items"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .map(|r| {
                                    format!(
                                        "• {} — {} ⭐{}",
                                        r["full_name"].as_str().unwrap_or("?"),
                                        r["description"].as_str().unwrap_or(""),
                                        r["stargazers_count"].as_i64().unwrap_or(0)
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default(),
                    Err(e) => format!("Error: {}", e),
                }
            }
            "get_issue" => {
                let num = args["number"].as_i64().unwrap_or(0);
                if org.is_empty() || repo.is_empty() || num == 0 {
                    return "Error: org, repo, and number are required".to_string();
                }
                let path = format!("/repos/{}/{}/issues/{}", org, repo, num);
                match gh_get(&server, &path).await {
                    Ok(v) => format!(
                        "#{} [{}] {}\nby @{}\n\n{}",
                        v["number"].as_i64().unwrap_or(0),
                        v["state"].as_str().unwrap_or("?"),
                        v["title"].as_str().unwrap_or(""),
                        v["user"]["login"].as_str().unwrap_or("?"),
                        v["body"]
                            .as_str()
                            .unwrap_or("(no body)")
                            .chars()
                            .take(2000)
                            .collect::<String>()
                    ),
                    Err(e) => format!("Error: {}", e),
                }
            }
            "get_pr" => {
                let num = args["number"].as_i64().unwrap_or(0);
                if org.is_empty() || repo.is_empty() || num == 0 {
                    return "Error: org, repo, and number are required".to_string();
                }
                let path = format!("/repos/{}/{}/pulls/{}", org, repo, num);
                match gh_get(&server, &path).await {
                    Ok(v) => format!(
                        "PR #{} [{}] {}\nby @{} | {} → {}\n\n{}",
                        v["number"].as_i64().unwrap_or(0),
                        v["state"].as_str().unwrap_or("?"),
                        v["title"].as_str().unwrap_or(""),
                        v["user"]["login"].as_str().unwrap_or("?"),
                        v["head"]["ref"].as_str().unwrap_or("?"),
                        v["base"]["ref"].as_str().unwrap_or("?"),
                        v["body"]
                            .as_str()
                            .unwrap_or("(no body)")
                            .chars()
                            .take(2000)
                            .collect::<String>()
                    ),
                    Err(e) => format!("Error: {}", e),
                }
            }
            _ => format!("Unknown action: {}", action),
        }
    }
}

fn format_issues(v: &serde_json::Value) -> String {
    v.as_array()
        .map(|a| {
            if a.is_empty() {
                return "No issues found.".to_string();
            }
            a.iter()
                .map(|i| {
                    format!(
                        "#{} [{}] {} (@{})",
                        i["number"].as_i64().unwrap_or(0),
                        i["state"].as_str().unwrap_or("?"),
                        i["title"].as_str().unwrap_or(""),
                        i["user"]["login"].as_str().unwrap_or("?"),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| "No results.".to_string())
}

fn format_prs(v: &serde_json::Value) -> String {
    v.as_array()
        .map(|a| {
            if a.is_empty() {
                return "No PRs found.".to_string();
            }
            a.iter()
                .map(|p| {
                    format!(
                        "#{} [{}] {} (@{}) {} → {}",
                        p["number"].as_i64().unwrap_or(0),
                        p["state"].as_str().unwrap_or("?"),
                        p["title"].as_str().unwrap_or(""),
                        p["user"]["login"].as_str().unwrap_or("?"),
                        p["head"]["ref"].as_str().unwrap_or("?"),
                        p["base"]["ref"].as_str().unwrap_or("?"),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| "No results.".to_string())
}
