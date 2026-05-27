//! GitHub dashboard — shows open issues and PRs grouped by org/repo.
//! Data is fetched live from the GitHub API using the token and server
//! configured in AppSettings (github_token, github_server, github_orgs).

use super::common::{now_human, render_page};
use crate::settings::load_settings;
use serde_json::{json, Value};

// ── helpers ──────────────────────────────────────────────────────────────────

fn api_base() -> String {
    let s = load_settings();
    if s.github_server.is_empty() {
        "https://api.github.com".to_string()
    } else {
        s.github_server.trim_end_matches('/').to_string()
    }
}

fn auth_header() -> Option<String> {
    let s = load_settings();
    if s.github_token.is_empty() {
        None
    } else {
        Some(format!("Bearer {}", s.github_token))
    }
}

fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("OmniLauncher/1.0")
        .build()
        .map_err(|e| e.to_string())
}

async fn gh_get(path: &str) -> Result<Value, String> {
    let client = build_client()?;
    let url = format!("{}{}", api_base(), path);
    let mut req = client
        .get(&url)
        .header("Accept", "application/vnd.github+json");
    if let Some(tok) = auth_header() {
        req = req.header("Authorization", tok);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} — {}", resp.status(), url));
    }
    resp.json().await.map_err(|e| e.to_string())
}

// Fetch all repos for a given owner (user or org)
async fn fetch_repos(owner: &str) -> Vec<Value> {
    // Try org first, fall back to user
    let path = format!("/orgs/{}/repos?sort=updated&per_page=50&type=all", owner);
    if let Ok(v) = gh_get(&path).await {
        if let Some(arr) = v.as_array() {
            return arr.clone();
        }
    }
    let path = format!("/users/{}/repos?sort=updated&per_page=50&type=all", owner);
    if let Ok(v) = gh_get(&path).await {
        if let Some(arr) = v.as_array() {
            return arr.clone();
        }
    }
    vec![]
}

async fn fetch_issues(owner: &str, repo: &str) -> Vec<Value> {
    let path = format!(
        "/repos/{}/{}/issues?state=open&per_page=30",
        owner, repo
    );
    match gh_get(&path).await {
        Ok(v) => v.as_array().cloned().unwrap_or_default(),
        Err(_) => vec![],
    }
}

async fn fetch_prs(owner: &str, repo: &str) -> Vec<Value> {
    let path = format!(
        "/repos/{}/{}/pulls?state=open&per_page=30",
        owner, repo
    );
    match gh_get(&path).await {
        Ok(v) => v.as_array().cloned().unwrap_or_default(),
        Err(_) => vec![],
    }
}

// ── data endpoint ─────────────────────────────────────────────────────────────

pub async fn github_data_json() -> String {
    let settings = load_settings();
    let orgs = settings.github_orgs.clone();

    if orgs.is_empty() {
        return json!({
            "generated_at": now_human(),
            "error": "No github_orgs configured. Add orgs/owners in Settings.",
            "orgs": []
        })
        .to_string();
    }

    let mut org_data: Vec<Value> = vec![];

    for org in &orgs {
        let repos = fetch_repos(org).await;
        let mut repo_data: Vec<Value> = vec![];

        for repo in &repos {
            let repo_name = repo["name"].as_str().unwrap_or("").to_string();
            let full_name = repo["full_name"].as_str().unwrap_or("").to_string();
            let html_url = repo["html_url"].as_str().unwrap_or("").to_string();
            let description = repo["description"].as_str().unwrap_or("").to_string();

            let issues = fetch_issues(org, &repo_name).await;
            let prs = fetch_prs(org, &repo_name).await;

            // Filter out PRs from issues list (GitHub returns PRs as issues too)
            let pure_issues: Vec<Value> = issues
                .into_iter()
                .filter(|i| i.get("pull_request").is_none())
                .collect();

            if pure_issues.is_empty() && prs.is_empty() {
                continue; // skip repos with no activity
            }

            repo_data.push(json!({
                "name": repo_name,
                "full_name": full_name,
                "html_url": html_url,
                "description": description,
                "open_issues_count": pure_issues.len(),
                "open_prs_count": prs.len(),
                "issues": pure_issues.iter().map(|i| json!({
                    "number": i["number"],
                    "title": i["title"].as_str().unwrap_or(""),
                    "user": i["user"]["login"].as_str().unwrap_or(""),
                    "labels": i["labels"].as_array().map(|ls| ls.iter().map(|l| l["name"].as_str().unwrap_or("")).collect::<Vec<_>>()).unwrap_or_default(),
                    "created_at": i["created_at"].as_str().unwrap_or(""),
                    "html_url": i["html_url"].as_str().unwrap_or(""),
                })).collect::<Vec<_>>(),
                "prs": prs.iter().map(|p| json!({
                    "number": p["number"],
                    "title": p["title"].as_str().unwrap_or(""),
                    "user": p["user"]["login"].as_str().unwrap_or(""),
                    "head": p["head"]["ref"].as_str().unwrap_or(""),
                    "base": p["base"]["ref"].as_str().unwrap_or(""),
                    "draft": p["draft"].as_bool().unwrap_or(false),
                    "created_at": p["created_at"].as_str().unwrap_or(""),
                    "html_url": p["html_url"].as_str().unwrap_or(""),
                })).collect::<Vec<_>>(),
            }));
        }

        let total_issues: usize = repo_data
            .iter()
            .map(|r| r["open_issues_count"].as_u64().unwrap_or(0) as usize)
            .sum();
        let total_prs: usize = repo_data
            .iter()
            .map(|r| r["open_prs_count"].as_u64().unwrap_or(0) as usize)
            .sum();

        org_data.push(json!({
            "org": org,
            "repo_count": repo_data.len(),
            "total_issues": total_issues,
            "total_prs": total_prs,
            "repos": repo_data,
        }));
    }

    json!({
        "generated_at": now_human(),
        "orgs": org_data,
    })
    .to_string()
}

// ── HTML page ─────────────────────────────────────────────────────────────────

pub fn github_html() -> String {
    let body = r##"
    <div class="page-header">
      <h1 class="page-title">⚙ GitHub</h1>
      <div id="meta" class="page-lede">Loading…</div>
    </div>

    <div id="filter-bar" style="display:flex;gap:12px;flex-wrap:wrap;margin-bottom:20px;align-items:center">
      <label style="color:var(--text-secondary);font-size:13px">Org:</label>
      <select id="org-filter" style="background:var(--surface);border:1px solid var(--border);color:var(--text);border-radius:6px;padding:4px 10px;font-size:13px">
        <option value="">All orgs</option>
      </select>
      <label style="color:var(--text-secondary);font-size:13px">Type:</label>
      <select id="type-filter" style="background:var(--surface);border:1px solid var(--border);color:var(--text);border-radius:6px;padding:4px 10px;font-size:13px">
        <option value="">Issues &amp; PRs</option>
        <option value="issues">Issues only</option>
        <option value="prs">PRs only</option>
      </select>
      <input id="search" type="text" placeholder="Filter by title / repo…"
        style="background:var(--surface);border:1px solid var(--border);color:var(--text);border-radius:6px;padding:4px 12px;font-size:13px;min-width:200px;flex:1">
      <button onclick="loadData()" style="background:var(--accent);color:#000;border:none;border-radius:6px;padding:5px 14px;cursor:pointer;font-size:13px">↺ Refresh</button>
    </div>

    <div id="content"></div>
    "##;

    let script = r##"
    let allData = null;

    async function loadData() {
      document.getElementById('meta').textContent = 'Fetching from GitHub…';
      document.getElementById('content').innerHTML = '<p style="color:var(--sub)">Loading…</p>';
      try {
        const r = await fetch('/dashboard/github/data');
        allData = await r.json();
        if (allData.error) {
          document.getElementById('meta').textContent = allData.error;
          document.getElementById('content').innerHTML = `<p style="color:var(--error)">${allData.error}</p>`;
          return;
        }
        populateOrgFilter(allData.orgs);
        render();
        document.getElementById('meta').textContent = `Updated ${allData.generated_at}`;
      } catch(e) {
        document.getElementById('meta').textContent = 'Error: ' + e;
      }
    }

    function populateOrgFilter(orgs) {
      const sel = document.getElementById('org-filter');
      // preserve existing options after "All orgs"
      while (sel.options.length > 1) sel.remove(1);
      orgs.forEach(o => {
        const opt = document.createElement('option');
        opt.value = o.org;
        opt.textContent = `${o.org} (${o.total_issues}🔴 ${o.total_prs}🟢)`;
        sel.appendChild(opt);
      });
    }

    function render() {
      if (!allData) return;
      const orgFilter = document.getElementById('org-filter').value;
      const typeFilter = document.getElementById('type-filter').value;
      const search = document.getElementById('search').value.toLowerCase();

      let html = '';
      for (const org of allData.orgs) {
        if (orgFilter && org.org !== orgFilter) continue;
        let orgHtml = '';
        for (const repo of org.repos) {
          if (search && !repo.full_name.toLowerCase().includes(search)) {
            // still check individual items
          }
          let items = '';
          if (typeFilter !== 'prs') {
            for (const i of repo.issues) {
              if (search && !i.title.toLowerCase().includes(search) && !repo.full_name.toLowerCase().includes(search)) continue;
              const labels = i.labels.map(l => `<span style="background:var(--surface-2);border-radius:3px;padding:1px 5px;font-size:11px">${l}</span>`).join(' ');
              items += `<div class="item-row">
                <span class="badge issue">I</span>
                <a href="${i.html_url}" target="_blank" style="color:var(--text);text-decoration:none;flex:1">
                  <span style="color:var(--sub);font-size:12px">#${i.number}</span>
                  ${i.title}
                  ${labels}
                </a>
                <span class="item-meta">@${i.user}</span>
              </div>`;
            }
          }
          if (typeFilter !== 'issues') {
            for (const p of repo.prs) {
              if (search && !p.title.toLowerCase().includes(search) && !repo.full_name.toLowerCase().includes(search)) continue;
              const draft = p.draft ? '<span style="color:var(--sub);font-size:11px"> [draft]</span>' : '';
              items += `<div class="item-row">
                <span class="badge pr">PR</span>
                <a href="${p.html_url}" target="_blank" style="color:var(--text);text-decoration:none;flex:1">
                  <span style="color:var(--sub);font-size:12px">#${p.number}</span>
                  ${p.title}${draft}
                  <span style="color:var(--sub);font-size:11px"> ${p.head}→${p.base}</span>
                </a>
                <span class="item-meta">@${p.user}</span>
              </div>`;
            }
          }
          if (!items) continue;
          orgHtml += `
            <div class="repo-card">
              <div class="repo-header">
                <a href="${repo.html_url}" target="_blank" style="color:var(--accent);text-decoration:none;font-weight:600">${repo.full_name}</a>
                <span style="color:var(--sub);font-size:12px;margin-left:8px">${repo.description}</span>
                <div style="margin-left:auto;display:flex;gap:8px">
                  <span class="pill issue">${repo.open_issues_count} issues</span>
                  <span class="pill pr">${repo.open_prs_count} PRs</span>
                </div>
              </div>
              <div class="item-list">${items}</div>
            </div>`;
        }
        if (!orgHtml) continue;
        html += `
          <div class="org-section">
            <div class="org-header">
              <span style="font-size:16px;font-weight:700">⊙ ${org.org}</span>
              <span style="color:var(--sub);font-size:13px;margin-left:12px">${org.repo_count} repos · ${org.total_issues} open issues · ${org.total_prs} open PRs</span>
            </div>
            ${orgHtml}
          </div>`;
      }
      document.getElementById('content').innerHTML = html || '<p style="color:var(--sub)">No matching results.</p>';
    }

    document.getElementById('org-filter').addEventListener('change', render);
    document.getElementById('type-filter').addEventListener('change', render);
    document.getElementById('search').addEventListener('input', render);

    loadData();
    "##;

    let style = r#"<style>
    .org-section { margin-bottom: 32px; }
    .org-header { padding: 8px 0 12px; border-bottom: 1px solid var(--border); margin-bottom: 12px; display: flex; align-items: center; }
    .repo-card { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); margin-bottom: 12px; overflow: hidden; }
    .repo-header { display: flex; align-items: center; padding: 10px 14px; background: var(--bg-elevated); flex-wrap: wrap; gap: 4px; }
    .item-list { padding: 6px 0; }
    .item-row { display: flex; align-items: center; gap: 8px; padding: 6px 14px; border-bottom: 1px solid var(--border); font-size: 13px; }
    .item-row:last-child { border-bottom: none; }
    .item-row:hover { background: var(--accent-hover); }
    .badge { border-radius: 4px; padding: 1px 5px; font-size: 11px; font-weight: 700; flex-shrink: 0; }
    .badge.issue { background: rgba(234,67,53,0.2); color: #ea4335; }
    .badge.pr { background: rgba(52,168,83,0.2); color: #34a853; }
    .pill { border-radius: 10px; padding: 1px 8px; font-size: 11px; }
    .pill.issue { background: rgba(234,67,53,0.15); color: #ea4335; }
    .pill.pr { background: rgba(52,168,83,0.15); color: #34a853; }
    .item-meta { color: var(--sub); font-size: 11px; white-space: nowrap; }
    </style>"#;

    let full_body = format!("{}{}", style, body);
    render_page("GitHub", "github", &full_body, script)
}
