//! GitHub dashboard — shows open issues and PRs grouped by server → org → repo.
//! Supports multiple GitHub servers (github.com + GHE instances).
//! Token resolved via gh CLI auth or explicit settings.

use super::common::{now_human, render_page};
use crate::settings::{load_settings, GitHubServer};
use serde_json::{json, Value};

// ── HTTP helper ───────────────────────────────────────────────────────────────

async fn gh_get(server: &GitHubServer, path: &str) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("OmniLauncher/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}{}", server.effective_api_base(), path);
    let mut req = client
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

async fn fetch_repos(server: &GitHubServer, owner: &str) -> Vec<Value> {
    let path = format!("/orgs/{}/repos?sort=updated&per_page=50&type=all", owner);
    if let Ok(v) = gh_get(server, &path).await {
        if let Some(arr) = v.as_array() {
            return arr.clone();
        }
    }
    let path = format!("/users/{}/repos?sort=updated&per_page=50&type=all", owner);
    if let Ok(v) = gh_get(server, &path).await {
        if let Some(arr) = v.as_array() {
            return arr.clone();
        }
    }
    vec![]
}

async fn fetch_issues(server: &GitHubServer, owner: &str, repo: &str) -> Vec<Value> {
    let path = format!("/repos/{}/{}/issues?state=open&per_page=30", owner, repo);
    match gh_get(server, &path).await {
        Ok(v) => v.as_array().cloned().unwrap_or_default(),
        Err(_) => vec![],
    }
}

async fn fetch_prs(server: &GitHubServer, owner: &str, repo: &str) -> Vec<Value> {
    let path = format!("/repos/{}/{}/pulls?state=open&per_page=30", owner, repo);
    match gh_get(server, &path).await {
        Ok(v) => v.as_array().cloned().unwrap_or_default(),
        Err(_) => vec![],
    }
}

// ── data endpoint ─────────────────────────────────────────────────────────────

pub async fn github_data_json() -> String {
    let settings = load_settings();
    let servers = settings.github_servers.clone();

    if servers.is_empty() {
        return json!({
            "generated_at": now_human(),
            "error": "No github_servers configured. Add entries to settings.json or run: gh auth login",
            "servers": []
        })
        .to_string();
    }

    let mut server_data: Vec<Value> = vec![];

    for server in &servers {
        let has_token = server.resolve_token().is_some();
        if server.orgs.is_empty() {
            server_data.push(json!({
                "hostname": server.hostname,
                "has_token": has_token,
                "error": "No orgs configured for this server.",
                "orgs": []
            }));
            continue;
        }

        let mut org_data: Vec<Value> = vec![];

        for org in &server.orgs {
            let repos = fetch_repos(server, org).await;
            let mut repo_data: Vec<Value> = vec![];

            for repo in &repos {
                let repo_name = repo["name"].as_str().unwrap_or("").to_string();
                let full_name = repo["full_name"].as_str().unwrap_or("").to_string();
                let html_url = repo["html_url"].as_str().unwrap_or("").to_string();
                let description = repo["description"].as_str().unwrap_or("").to_string();

                let issues = fetch_issues(server, org, &repo_name).await;
                let prs = fetch_prs(server, org, &repo_name).await;

                // Filter out PRs from issues list
                let pure_issues: Vec<Value> = issues
                    .into_iter()
                    .filter(|i| i.get("pull_request").is_none())
                    .collect();

                if pure_issues.is_empty() && prs.is_empty() {
                    continue;
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

        server_data.push(json!({
            "hostname": server.hostname,
            "has_token": has_token,
            "orgs": org_data,
        }));
    }

    json!({
        "generated_at": now_human(),
        "servers": server_data,
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
      <label style="color:var(--text-secondary);font-size:13px">Server:</label>
      <select id="server-filter" style="background:var(--surface);border:1px solid var(--border);color:var(--text);border-radius:6px;padding:4px 10px;font-size:13px">
        <option value="">All servers</option>
      </select>
      <label style="color:var(--text-secondary);font-size:13px">Org:</label>
      <select id="org-filter" style="background:var(--surface);border:1px solid var(--border);color:var(--text);border-radius:6px;padding:4px 10px;font-size:13px">
        <option value="">All orgs</option>
      </select>
      <label style="color:var(--text-secondary);font-size:13px">Repo:</label>
      <select id="repo-filter" style="background:var(--surface);border:1px solid var(--border);color:var(--text);border-radius:6px;padding:4px 10px;font-size:13px">
        <option value="">All repos</option>
      </select>
      <label style="color:var(--text-secondary);font-size:13px">Type:</label>
      <select id="type-filter" style="background:var(--surface);border:1px solid var(--border);color:var(--text);border-radius:6px;padding:4px 10px;font-size:13px">
        <option value="">Issues &amp; PRs</option>
        <option value="issues">Issues only</option>
        <option value="prs">PRs only</option>
      </select>
      <input id="search" type="text" placeholder="Filter by title…"
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
        populateFilters(allData.servers);
        render();
        const totalI = allData.servers.flatMap(s => s.orgs||[]).reduce((a,o) => a + (o.total_issues||0), 0);
        const totalP = allData.servers.flatMap(s => s.orgs||[]).reduce((a,o) => a + (o.total_prs||0), 0);
        document.getElementById('meta').textContent = `${allData.servers.length} server(s) · ${totalI} open issues · ${totalP} open PRs · ${allData.generated_at}`;
      } catch(e) {
        document.getElementById('meta').textContent = 'Error: ' + e;
      }
    }

    function populateFilters(servers) {
      const srvSel = document.getElementById('server-filter');
      while (srvSel.options.length > 1) srvSel.remove(1);
      servers.forEach(s => {
        const opt = document.createElement('option');
        opt.value = s.hostname;
        opt.textContent = s.hostname + (s.has_token ? ' ✓' : ' ✗');
        srvSel.appendChild(opt);
      });
      refreshOrgDropdown();
    }

    function refreshOrgDropdown() {
      if (!allData) return;
      const srvFilter = document.getElementById('server-filter').value;
      const orgSel = document.getElementById('org-filter');
      const prevOrg = orgSel.value;
      while (orgSel.options.length > 1) orgSel.remove(1);
      const seenOrgs = new Set();
      allData.servers.forEach(s => {
        if (srvFilter && s.hostname !== srvFilter) return;
        (s.orgs||[]).forEach(o => {
          if (!seenOrgs.has(o.org)) {
            seenOrgs.add(o.org);
            const oo = document.createElement('option');
            oo.value = o.org;
            oo.textContent = `${o.org} (${o.total_issues||0}🔴 ${o.total_prs||0}🟢)`;
            orgSel.appendChild(oo);
          }
        });
      });
      orgSel.value = seenOrgs.has(prevOrg) ? prevOrg : '';
      refreshRepoDropdown();
    }

    function refreshRepoDropdown() {
      if (!allData) return;
      const srvFilter = document.getElementById('server-filter').value;
      const orgFilter = document.getElementById('org-filter').value;
      const repoSel = document.getElementById('repo-filter');
      const prevRepo = repoSel.value;
      while (repoSel.options.length > 1) repoSel.remove(1);
      const seenRepos = new Set();
      allData.servers.forEach(s => {
        if (srvFilter && s.hostname !== srvFilter) return;
        (s.orgs||[]).forEach(o => {
          if (orgFilter && o.org !== orgFilter) return;
          (o.repos||[]).forEach(r => {
            if (!seenRepos.has(r.full_name)) {
              seenRepos.add(r.full_name);
              const opt = document.createElement('option');
              opt.value = r.full_name;
              opt.textContent = `${r.name} (${r.open_issues_count}🔴 ${r.open_prs_count}🟢)`;
              repoSel.appendChild(opt);
            }
          });
        });
      });
      repoSel.value = seenRepos.has(prevRepo) ? prevRepo : '';
    }

    function render() {
      if (!allData) return;
      const srvFilter = document.getElementById('server-filter').value;
      const orgFilter = document.getElementById('org-filter').value;
      const repoFilter = document.getElementById('repo-filter').value;
      const typeFilter = document.getElementById('type-filter').value;
      const search = document.getElementById('search').value.toLowerCase();

      let html = '';
      for (const srv of allData.servers) {
        if (srvFilter && srv.hostname !== srvFilter) continue;
        if (srv.error) {
          html += `<div class="server-section"><div class="server-header">⊗ ${srv.hostname} <span style="color:var(--error);font-size:13px">${srv.error}</span></div></div>`;
          continue;
        }
        let srvHtml = '';
        for (const org of (srv.orgs||[])) {
          if (orgFilter && org.org !== orgFilter) continue;
          let orgHtml = '';
          for (const repo of org.repos) {
            if (repoFilter && repo.full_name !== repoFilter) continue;
            let items = '';
            if (typeFilter !== 'prs') {
              for (const i of repo.issues) {
                if (search && !i.title.toLowerCase().includes(search) && !repo.full_name.toLowerCase().includes(search)) continue;
                const labels = (i.labels||[]).map(l => `<span class="label">${l}</span>`).join(' ');
                items += `<div class="item-row">
                  <span class="badge issue">I</span>
                  <a href="${i.html_url}" target="_blank" class="item-link">
                    <span class="item-num">#${i.number}</span> ${i.title} ${labels}
                  </a>
                  <span class="item-meta">@${i.user}</span>
                </div>`;
              }
            }
            if (typeFilter !== 'issues') {
              for (const p of repo.prs) {
                if (search && !p.title.toLowerCase().includes(search) && !repo.full_name.toLowerCase().includes(search)) continue;
                const draft = p.draft ? '<span class="draft">[draft]</span>' : '';
                items += `<div class="item-row">
                  <span class="badge pr">PR</span>
                  <a href="${p.html_url}" target="_blank" class="item-link">
                    <span class="item-num">#${p.number}</span> ${p.title} ${draft}
                    <span class="branch">${p.head}→${p.base}</span>
                  </a>
                  <span class="item-meta">@${p.user}</span>
                </div>`;
              }
            }
            if (!items) continue;
            orgHtml += `<div class="repo-card">
              <div class="repo-header">
                <a href="${repo.html_url}" target="_blank" class="repo-link">${repo.full_name}</a>
                <span class="repo-desc">${repo.description}</span>
                <div style="margin-left:auto;display:flex;gap:6px">
                  <span class="pill issue">${repo.open_issues_count} issues</span>
                  <span class="pill pr">${repo.open_prs_count} PRs</span>
                </div>
              </div>
              <div class="item-list">${items}</div>
            </div>`;
          }
          if (!orgHtml) continue;
          srvHtml += `<div class="org-section">
            <div class="org-header">
              <span class="org-name">⊙ ${org.org}</span>
              <span class="org-meta">${org.repo_count} repos · ${org.total_issues} issues · ${org.total_prs} PRs</span>
            </div>
            ${orgHtml}
          </div>`;
        }
        if (!srvHtml && !srv.error) srvHtml = '<p style="color:var(--sub);padding:8px 0">No open issues or PRs.</p>';
        html += `<div class="server-section">
          <div class="server-header">
            <span class="server-name">◈ ${srv.hostname}</span>
            <span class="server-auth">${srv.has_token ? '✓ authenticated' : '✗ no token'}</span>
          </div>
          ${srvHtml}
        </div>`;
      }
      document.getElementById('content').innerHTML = html || '<p style="color:var(--sub)">No matching results.</p>';
    }

    document.getElementById('server-filter').addEventListener('change', () => { refreshOrgDropdown(); render(); });
    document.getElementById('org-filter').addEventListener('change', () => { refreshRepoDropdown(); render(); });
    document.getElementById('repo-filter').addEventListener('change', render);
    document.getElementById('type-filter').addEventListener('change', render);
    document.getElementById('search').addEventListener('input', render);

    loadData();
    "##;

    let style = r#"<style>
    .server-section { margin-bottom: 36px; }
    .server-header { display:flex; align-items:center; gap:10px; padding:10px 14px; background:var(--bg-elevated); border:1px solid var(--border); border-radius:var(--radius); margin-bottom:14px; }
    .server-name { font-size:15px; font-weight:700; color:var(--accent); }
    .server-auth { font-size:12px; color:var(--sub); }
    .org-section { margin-bottom:20px; padding-left:8px; }
    .org-header { display:flex; align-items:center; gap:8px; padding:6px 0 10px; border-bottom:1px solid var(--border); margin-bottom:10px; }
    .org-name { font-size:14px; font-weight:600; }
    .org-meta { font-size:12px; color:var(--sub); }
    .repo-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); margin-bottom:10px; overflow:hidden; }
    .repo-header { display:flex; align-items:center; padding:9px 14px; background:var(--bg-elevated); flex-wrap:wrap; gap:4px; }
    .repo-link { color:var(--accent); text-decoration:none; font-weight:600; font-size:13px; }
    .repo-link:hover { text-decoration:underline; }
    .repo-desc { color:var(--sub); font-size:12px; margin-left:6px; }
    .item-list { padding:4px 0; }
    .item-row { display:flex; align-items:center; gap:8px; padding:6px 14px; border-bottom:1px solid var(--border); font-size:13px; }
    .item-row:last-child { border-bottom:none; }
    .item-row:hover { background:var(--accent-hover); }
    .badge { border-radius:4px; padding:1px 5px; font-size:11px; font-weight:700; flex-shrink:0; }
    .badge.issue { background:rgba(234,67,53,0.2); color:#ea4335; }
    .badge.pr { background:rgba(52,168,83,0.2); color:#34a853; }
    .item-link { color:var(--text); text-decoration:none; flex:1; display:flex; align-items:center; gap:5px; flex-wrap:wrap; }
    .item-link:hover { color:var(--accent); }
    .item-num { color:var(--sub); font-size:12px; }
    .item-meta { color:var(--sub); font-size:11px; white-space:nowrap; }
    .label { background:var(--surface-2); border-radius:3px; padding:1px 5px; font-size:11px; }
    .draft { color:var(--sub); font-size:11px; }
    .branch { color:var(--sub); font-size:11px; }
    .pill { border-radius:10px; padding:1px 8px; font-size:11px; }
    .pill.issue { background:rgba(234,67,53,0.15); color:#ea4335; }
    .pill.pr { background:rgba(52,168,83,0.15); color:#34a853; }
    </style>"#;

    let full_body = format!("{}{}", style, body);
    render_page("GitHub", "github", &full_body, script)
}