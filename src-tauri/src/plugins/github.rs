use crate::plugins::{Plugin, Query, QueryResult};
use crate::settings::load_settings;
use async_trait::async_trait;

pub struct GitHubPlugin;

fn gh_config() -> (String, String) {
    let s = load_settings();
    let server = if s.github_server.is_empty() {
        "https://api.github.com".to_string()
    } else {
        s.github_server.trim_end_matches('/').to_string()
    };
    (server, s.github_token)
}

async fn gh_get(path: &str) -> Result<serde_json::Value, String> {
    let (server, token) = gh_config();
    let url = format!("{}{}", server, path);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("OmniLauncher/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client.get(&url).header("Accept", "application/vnd.github+json");
    if !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token));
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}: {}", resp.status(), url));
    }
    resp.json().await.map_err(|e| e.to_string())
}

#[async_trait]
impl Plugin for GitHubPlugin {
    fn name(&self) -> &str {
        "github"
    }

    fn description(&self) -> &str {
        "GitHub: search repos, list issues/PRs, open in browser"
    }

    fn keyword(&self) -> Option<&str> {
        Some("gh ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let term = q.raw.strip_prefix("gh ").unwrap_or("").trim();
        let commands = vec![
            ("issues", "🔴", "Open issues assigned to you"),
            ("prs", "🟢", "Open pull requests"),
            ("repos", "📁", "Your repositories"),
            ("dashboard", "📊", "Open GitHub dashboard"),
        ];

        commands
            .into_iter()
            .filter(|(name, _, _)| term.is_empty() || name.contains(term))
            .map(|(name, icon, desc)| QueryResult {
                id: format!("github:{}", name),
                title: desc.to_string(),
                subtitle: Some(format!("gh {}", name)),
                icon: Some(icon.to_string()),
                score: 70,
                action_type: "plugin_execute".to_string(),
                action_data: format!("github:{}", name),
            })
            .collect()
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "github",
                "description": "GitHub API: list/search repos, issues, PRs. Supports GitHub.com and GitHub Enterprise.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["list_repos", "list_issues", "list_prs", "search_repos", "get_issue", "get_pr"],
                            "description": "Action to perform"
                        },
                        "org": { "type": "string", "description": "Organization or owner name" },
                        "repo": { "type": "string", "description": "Repository name (without owner prefix)" },
                        "query": { "type": "string", "description": "Search query for search_repos" },
                        "number": { "type": "integer", "description": "Issue or PR number" },
                        "state": { "type": "string", "enum": ["open", "closed", "all"], "description": "Filter by state (default: open)" }
                    },
                    "required": ["action"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let action = args["action"].as_str().unwrap_or("");
        let org = args["org"].as_str().unwrap_or("");
        let repo = args["repo"].as_str().unwrap_or("");
        let state = args["state"].as_str().unwrap_or("open");

        match action {
            "list_repos" => {
                let path = if org.is_empty() {
                    "/user/repos?sort=updated&per_page=30".to_string()
                } else {
                    format!("/orgs/{}/repos?sort=updated&per_page=30", org)
                };
                match gh_get(&path).await {
                    Ok(v) => {
                        let repos = v.as_array().map(|a| {
                            a.iter().map(|r| format!(
                                "• {} — {} ⭐{}",
                                r["full_name"].as_str().unwrap_or("?"),
                                r["description"].as_str().unwrap_or(""),
                                r["stargazers_count"].as_i64().unwrap_or(0)
                            )).collect::<Vec<_>>().join("\n")
                        }).unwrap_or_default();
                        repos
                    }
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
                    format!("/repos/{}/{}/issues?state={}&per_page=30", owner, repo, state)
                };
                match gh_get(&path).await {
                    Ok(v) => format_issues(&v),
                    Err(e) => format!("Error: {}", e),
                }
            }
            "list_prs" => {
                let path = if org.is_empty() && repo.is_empty() {
                    "/pulls?state=open&per_page=30".to_string()
                } else {
                    let owner = if org.is_empty() { "me" } else { org };
                    format!("/repos/{}/{}/pulls?state={}&per_page=30", owner, repo, state)
                };
                match gh_get(&path).await {
                    Ok(v) => format_prs(&v),
                    Err(e) => format!("Error: {}", e),
                }
            }
            "search_repos" => {
                let q = args["query"].as_str().unwrap_or("");
                if q.is_empty() {
                    return "Error: query is required for search_repos".to_string();
                }
                let encoded_q: String = q.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c.to_string() } else { format!("%{:02X}", c as u32) }).collect();
                let path = format!("/search/repositories?q={}&per_page=20", encoded_q);
                match gh_get(&path).await {
                    Ok(v) => {
                        let items = v["items"].as_array().map(|a| {
                            a.iter().map(|r| format!(
                                "• {} — {} ⭐{}",
                                r["full_name"].as_str().unwrap_or("?"),
                                r["description"].as_str().unwrap_or(""),
                                r["stargazers_count"].as_i64().unwrap_or(0)
                            )).collect::<Vec<_>>().join("\n")
                        }).unwrap_or_default();
                        items
                    }
                    Err(e) => format!("Error: {}", e),
                }
            }
            "get_issue" => {
                let num = args["number"].as_i64().unwrap_or(0);
                if org.is_empty() || repo.is_empty() || num == 0 {
                    return "Error: org, repo, and number are required".to_string();
                }
                let path = format!("/repos/{}/{}/issues/{}", org, repo, num);
                match gh_get(&path).await {
                    Ok(v) => format!(
                        "#{} [{}] {}\nby @{}\n\n{}",
                        v["number"].as_i64().unwrap_or(0),
                        v["state"].as_str().unwrap_or("?"),
                        v["title"].as_str().unwrap_or(""),
                        v["user"]["login"].as_str().unwrap_or("?"),
                        v["body"].as_str().unwrap_or("(no body)").chars().take(2000).collect::<String>()
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
                match gh_get(&path).await {
                    Ok(v) => format!(
                        "PR #{} [{}] {}\nby @{} | {} → {}\n\n{}",
                        v["number"].as_i64().unwrap_or(0),
                        v["state"].as_str().unwrap_or("?"),
                        v["title"].as_str().unwrap_or(""),
                        v["user"]["login"].as_str().unwrap_or("?"),
                        v["head"]["ref"].as_str().unwrap_or("?"),
                        v["base"]["ref"].as_str().unwrap_or("?"),
                        v["body"].as_str().unwrap_or("(no body)").chars().take(2000).collect::<String>()
                    ),
                    Err(e) => format!("Error: {}", e),
                }
            }
            _ => format!("Unknown action: {}", action),
        }
    }
}

fn format_issues(v: &serde_json::Value) -> String {
    v.as_array().map(|a| {
        if a.is_empty() { return "No issues found.".to_string(); }
        a.iter().map(|i| format!(
            "#{} [{}] {} (@{})",
            i["number"].as_i64().unwrap_or(0),
            i["state"].as_str().unwrap_or("?"),
            i["title"].as_str().unwrap_or(""),
            i["user"]["login"].as_str().unwrap_or("?"),
        )).collect::<Vec<_>>().join("\n")
    }).unwrap_or_else(|| "No results.".to_string())
}

fn format_prs(v: &serde_json::Value) -> String {
    v.as_array().map(|a| {
        if a.is_empty() { return "No PRs found.".to_string(); }
        a.iter().map(|p| format!(
            "#{} [{}] {} (@{}) {} → {}",
            p["number"].as_i64().unwrap_or(0),
            p["state"].as_str().unwrap_or("?"),
            p["title"].as_str().unwrap_or(""),
            p["user"]["login"].as_str().unwrap_or("?"),
            p["head"]["ref"].as_str().unwrap_or("?"),
            p["base"]["ref"].as_str().unwrap_or("?"),
        )).collect::<Vec<_>>().join("\n")
    }).unwrap_or_else(|| "No results.".to_string())
}
