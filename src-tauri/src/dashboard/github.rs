//! GitHub dashboard — shows open issues and PRs grouped by server → org → repo.
//! Supports multiple GitHub servers (github.com + GHE instances).
//! Token resolved via gh CLI auth or explicit settings.

use super::common::{now_human, render_page};
use crate::settings::{load_settings, GitHubServer};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Max pages of `/user/repos` to walk (100 repos per page → up to 500 repos).
const MAX_REPO_PAGES: u32 = 5;

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

/// Fetch repos the authenticated user can access (owner, collaborator, org
/// member) — paginated. This replaces enumerating `/orgs/{org}/repos` which
/// returns every repo in the org (often thousands).
async fn fetch_user_accessible_repos(server: &GitHubServer) -> Vec<Value> {
    let mut all: Vec<Value> = Vec::new();
    for page in 1..=MAX_REPO_PAGES {
        let path = format!(
            "/user/repos?per_page=100&page={}&affiliation=owner,collaborator,organization_member&sort=updated",
            page
        );
        match gh_get(server, &path).await {
            Ok(v) => match v.as_array() {
                Some(arr) if !arr.is_empty() => {
                    let len = arr.len();
                    all.extend(arr.iter().cloned());
                    if len < 100 {
                        break;
                    }
                }
                _ => break,
            },
            Err(_) => break,
        }
    }
    all
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

/// Auto-discover the authenticated user's login + orgs when none are configured.
/// Used so dashboards work out-of-the-box after `gh auth login`.
///
/// Returns `(owners, diagnostic_error)` — the error string is populated when
/// no orgs were discovered, so the dashboard can surface why.
async fn fetch_default_owners(server: &GitHubServer) -> (Vec<String>, Option<String>) {
    let mut owners: Vec<String> = vec![];
    let mut diagnostics: Vec<String> = vec![];

    // 1. Always try to add the authenticated user's own login as an owner.
    match gh_get(server, "/user").await {
        Ok(user) => {
            if let Some(login) = user.get("login").and_then(|v| v.as_str()) {
                owners.push(login.to_string());
            }
        }
        Err(e) => diagnostics.push(format!("/user: {e}")),
    }

    // 2. Prefer `gh` CLI for org enumeration — it uses GraphQL and handles
    //    SAML SSO + scope grants correctly, matching what `gh org list` shows.
    let hostname = if server.hostname.is_empty() {
        "github.com"
    } else {
        server.hostname.as_str()
    };
    let cli_orgs = fetch_orgs_via_gh_cli(hostname);
    match cli_orgs {
        Ok(orgs) => {
            for o in orgs {
                if !owners.iter().any(|x| x.eq_ignore_ascii_case(&o)) {
                    owners.push(o);
                }
            }
        }
        Err(e) => {
            diagnostics.push(format!("gh org list: {e}"));
            // 3. Fall back to REST /user/orgs — only finds orgs whose token
            //    has the `read:org` scope (and no pending SSO authorization).
            match gh_get(server, "/user/orgs?per_page=100").await {
                Ok(orgs) => {
                    if let Some(arr) = orgs.as_array() {
                        for o in arr {
                            if let Some(login) = o.get("login").and_then(|v| v.as_str()) {
                                if !owners.iter().any(|x| x.eq_ignore_ascii_case(login)) {
                                    owners.push(login.to_string());
                                }
                            }
                        }
                    }
                }
                Err(e) => diagnostics.push(format!("/user/orgs: {e}")),
            }
        }
    }

    let err = if owners.is_empty() && !diagnostics.is_empty() {
        Some(diagnostics.join(" | "))
    } else {
        None
    };
    (owners, err)
}

/// Enumerate orgs the user belongs to on `hostname` by shelling out to
/// `gh org list --hostname <host>`. Matches the orgs visible in the gh CLI,
/// including SAML-protected enterprise orgs that the REST `/user/orgs`
/// endpoint may omit.
fn fetch_orgs_via_gh_cli(hostname: &str) -> Result<Vec<String>, String> {
    let output = std::process::Command::new("gh")
        .args(["org", "list", "--hostname", hostname])
        .output()
        .map_err(|e| format!("spawn gh failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("gh exited with status {}", output.status)
        } else {
            stderr
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let orgs: Vec<String> = stdout
        .lines()
        .map(|l| l.trim())
        .filter(|l| {
            // Skip blank lines and the "Showing N of M organizations" footer.
            !l.is_empty()
                && !l.starts_with("Showing ")
                && !l.contains(' ')
                && l.chars()
                    .next()
                    .map_or(false, |c| c.is_ascii_alphanumeric())
        })
        .map(|l| l.to_string())
        .collect();
    Ok(orgs)
}

// ── data endpoint ─────────────────────────────────────────────────────────────

/// Lightweight listing: server → org → repo with name + URL + description only.
/// Issue / PR counts are fetched on-demand via [`github_repo_detail_json`] when
/// the user expands a repo in the UI.
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
        let (effective_orgs, discovery_err): (Vec<String>, Option<String>) =
            if server.orgs.is_empty() && has_token {
                fetch_default_owners(server).await
            } else {
                (server.orgs.clone(), None)
            };

        if effective_orgs.is_empty() {
            let error_msg = if !has_token {
                "No token resolved. Run `gh auth login --hostname <host>` or set token in settings.json.".to_string()
            } else if let Some(e) = discovery_err {
                format!("No orgs found for the authenticated user. ({e})")
            } else {
                "No orgs found for the authenticated user.".to_string()
            };
            server_data.push(json!({
                "hostname": server.hostname,
                "has_token": has_token,
                "error": error_msg,
                "orgs": []
            }));
            continue;
        }

        // ONE call (paginated) for repos the user can access, grouped by owner.
        let all_repos = fetch_user_accessible_repos(server).await;
        let mut by_owner: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for r in all_repos {
            let owner = r
                .get("owner")
                .and_then(|o| o.get("login"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if owner.is_empty() {
                continue;
            }
            if !effective_orgs
                .iter()
                .any(|o| o.eq_ignore_ascii_case(&owner))
            {
                continue;
            }
            by_owner.entry(owner).or_default().push(r);
        }

        let mut org_data: Vec<Value> = vec![];
        for org in &effective_orgs {
            let repos = by_owner.remove(org).unwrap_or_default();
            let repo_list: Vec<Value> = repos
                .iter()
                .map(|r| {
                    json!({
                        "name": r["name"].as_str().unwrap_or(""),
                        "full_name": r["full_name"].as_str().unwrap_or(""),
                        "html_url": r["html_url"].as_str().unwrap_or(""),
                        "description": r["description"].as_str().unwrap_or(""),
                        // GitHub's open_issues_count includes PRs — useful as a
                        // coarse hint so the UI can hide repos with 0 activity
                        // without an extra API call.
                        "open_issues_hint": r["open_issues_count"].as_u64().unwrap_or(0),
                        "updated_at": r["updated_at"].as_str().unwrap_or(""),
                    })
                })
                .collect();

            org_data.push(json!({
                "org": org,
                "repo_count": repo_list.len(),
                "repos": repo_list,
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

/// On-demand detail endpoint. Expects query string `host=<hostname>&owner=<org>&repo=<name>`.
/// Returns open issues + PRs for that repo.
pub async fn github_repo_detail_json(query: String) -> String {
    let params = parse_query(&query);
    let host = params.get("host").map(String::as_str).unwrap_or("");
    let owner = params.get("owner").map(String::as_str).unwrap_or("");
    let repo = params.get("repo").map(String::as_str).unwrap_or("");

    if host.is_empty() || owner.is_empty() || repo.is_empty() {
        return json!({"error": "Missing host, owner, or repo query parameter"}).to_string();
    }

    let settings = load_settings();
    let server = match settings
        .github_servers
        .iter()
        .find(|s| s.hostname.eq_ignore_ascii_case(host))
        .cloned()
    {
        Some(s) => s,
        None => {
            return json!({"error": format!("Unknown host: {}", host)}).to_string();
        }
    };

    let (issues, prs) = tokio::join!(
        fetch_issues(&server, owner, repo),
        fetch_prs(&server, owner, repo)
    );
    let pure_issues: Vec<Value> = issues
        .into_iter()
        .filter(|i| i.get("pull_request").is_none())
        .collect();

    json!({
        "host": host,
        "owner": owner,
        "repo": repo,
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
    })
    .to_string()
}

/// Minimal URL-decoded query string parser. Splits on `&`, supports `key=value`
/// pairs, and decodes `%xx` escapes and `+` → space.
fn parse_query(q: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for pair in q.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        out.insert(url_decode(k), url_decode(v));
    }
    out
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                match (hi, lo) {
                    (Some(h), Some(l)) => {
                        out.push((h * 16 + l) as u8);
                        i += 3;
                    }
                    _ => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
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
      <input id="repo-search" type="text" placeholder="Filter repos…"
        style="background:var(--surface);border:1px solid var(--border);color:var(--text);border-radius:6px;padding:4px 12px;font-size:13px;min-width:200px;flex:1">
      <label style="color:var(--text-secondary);font-size:13px;display:flex;align-items:center;gap:5px">
        <input id="active-only" type="checkbox" checked> active only
      </label>
      <button onclick="loadData()" style="background:var(--accent);color:#000;border:none;border-radius:6px;padding:5px 14px;cursor:pointer;font-size:13px">↺ Refresh</button>
    </div>

    <div id="content"></div>
    "##;

    let script = r##"
    let allData = null;
    // Cache repo detail responses so re-expanding doesn't refetch.
    const repoDetailCache = new Map(); // key: `${host}|${owner}|${repo}` → {issues, prs}

    async function loadData() {
      document.getElementById('meta').textContent = 'Fetching from GitHub…';
      document.getElementById('content').innerHTML = '<p style="color:var(--sub)">Loading…</p>';
      repoDetailCache.clear();
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
        const totalRepos = allData.servers.flatMap(s => s.orgs||[]).reduce((a,o) => a + (o.repo_count||0), 0);
        document.getElementById('meta').textContent = `${allData.servers.length} server(s) · ${totalRepos} accessible repos · ${allData.generated_at}`;
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
            oo.textContent = `${o.org} (${o.repo_count} repos)`;
            orgSel.appendChild(oo);
          }
        });
      });
      orgSel.value = seenOrgs.has(prevOrg) ? prevOrg : '';
    }

    function escapeHtml(s) {
      return String(s||'').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
    }

    function render() {
      if (!allData) return;
      const srvFilter = document.getElementById('server-filter').value;
      const orgFilter = document.getElementById('org-filter').value;
      const repoSearch = document.getElementById('repo-search').value.toLowerCase();
      const activeOnly = document.getElementById('active-only').checked;

      let html = '';
      for (const srv of allData.servers) {
        if (srvFilter && srv.hostname !== srvFilter) continue;
        if (srv.error) {
          html += `<div class="server-section"><div class="server-header">⊗ ${escapeHtml(srv.hostname)} <span style="color:var(--error);font-size:13px">${escapeHtml(srv.error)}</span></div></div>`;
          continue;
        }
        let srvHtml = '';
        for (const org of (srv.orgs||[])) {
          if (orgFilter && org.org !== orgFilter) continue;
          let orgRows = '';
          let shownRepos = 0;
          for (const repo of (org.repos||[])) {
            if (repoSearch && !repo.full_name.toLowerCase().includes(repoSearch)) continue;
            const count = repo.open_issues_hint || 0;
            if (activeOnly && count === 0) continue;
            shownRepos++;
            const cacheKey = `${srv.hostname}|${org.org}|${repo.name}`;
            orgRows += `<div class="repo-card" data-key="${escapeHtml(cacheKey)}">
              <div class="repo-header" onclick="toggleRepo(this, '${escapeHtml(srv.hostname)}', '${escapeHtml(org.org)}', '${escapeHtml(repo.name)}')">
                <span class="chev">▶</span>
                <a href="${escapeHtml(repo.html_url)}" target="_blank" class="repo-link" onclick="event.stopPropagation()">${escapeHtml(repo.full_name)}</a>
                <span class="repo-desc">${escapeHtml(repo.description)}</span>
                <span class="pill open">${count} open</span>
              </div>
              <div class="repo-detail" style="display:none"></div>
            </div>`;
          }
          if (!shownRepos) continue;
          srvHtml += `<div class="org-section">
            <div class="org-header">
              <span class="org-name">⊙ ${escapeHtml(org.org)}</span>
              <span class="org-meta">${shownRepos} repos shown</span>
            </div>
            ${orgRows}
          </div>`;
        }
        if (!srvHtml && !srv.error) srvHtml = '<p style="color:var(--sub);padding:8px 0">No repos match the filter.</p>';
        html += `<div class="server-section">
          <div class="server-header">
            <span class="server-name">◈ ${escapeHtml(srv.hostname)}</span>
            <span class="server-auth">${srv.has_token ? '✓ authenticated' : '✗ no token'}</span>
          </div>
          ${srvHtml}
        </div>`;
      }
      document.getElementById('content').innerHTML = html || '<p style="color:var(--sub)">No matching results.</p>';
    }

    async function toggleRepo(headerEl, host, owner, repo) {
      const card = headerEl.parentElement;
      const detail = card.querySelector('.repo-detail');
      const chev = headerEl.querySelector('.chev');
      const isOpen = detail.style.display !== 'none';
      if (isOpen) {
        detail.style.display = 'none';
        chev.textContent = '▶';
        return;
      }
      detail.style.display = 'block';
      chev.textContent = '▼';
      const cacheKey = `${host}|${owner}|${repo}`;
      if (repoDetailCache.has(cacheKey)) {
        renderRepoDetail(detail, repoDetailCache.get(cacheKey));
        return;
      }
      detail.innerHTML = '<div class="loading-row">Loading issues and PRs…</div>';
      try {
        const url = `/dashboard/github/repo?host=${encodeURIComponent(host)}&owner=${encodeURIComponent(owner)}&repo=${encodeURIComponent(repo)}`;
        const r = await fetch(url);
        const data = await r.json();
        if (data.error) {
          detail.innerHTML = `<div class="loading-row" style="color:var(--error)">${escapeHtml(data.error)}</div>`;
          return;
        }
        repoDetailCache.set(cacheKey, data);
        renderRepoDetail(detail, data);
      } catch(e) {
        detail.innerHTML = `<div class="loading-row" style="color:var(--error)">Error: ${escapeHtml(String(e))}</div>`;
      }
    }

    function renderRepoDetail(container, data) {
      const issues = data.issues || [];
      const prs = data.prs || [];
      if (issues.length === 0 && prs.length === 0) {
        container.innerHTML = '<div class="loading-row">No open issues or PRs.</div>';
        return;
      }
      let html = '';
      for (const i of issues) {
        const labels = (i.labels||[]).map(l => `<span class="label">${escapeHtml(l)}</span>`).join(' ');
        html += `<div class="item-row">
          <span class="badge issue">I</span>
          <a href="${escapeHtml(i.html_url)}" target="_blank" class="item-link">
            <span class="item-num">#${i.number}</span> ${escapeHtml(i.title)} ${labels}
          </a>
          <span class="item-meta">@${escapeHtml(i.user)}</span>
        </div>`;
      }
      for (const p of prs) {
        const draft = p.draft ? '<span class="draft">[draft]</span>' : '';
        html += `<div class="item-row">
          <span class="badge pr">PR</span>
          <a href="${escapeHtml(p.html_url)}" target="_blank" class="item-link">
            <span class="item-num">#${p.number}</span> ${escapeHtml(p.title)} ${draft}
            <span class="branch">${escapeHtml(p.head)}→${escapeHtml(p.base)}</span>
          </a>
          <span class="item-meta">@${escapeHtml(p.user)}</span>
        </div>`;
      }
      container.innerHTML = html;
    }

    document.getElementById('server-filter').addEventListener('change', () => { refreshOrgDropdown(); render(); });
    document.getElementById('org-filter').addEventListener('change', render);
    document.getElementById('repo-search').addEventListener('input', render);
    document.getElementById('active-only').addEventListener('change', render);

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
    .repo-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); margin-bottom:8px; overflow:hidden; }
    .repo-header { display:flex; align-items:center; padding:9px 14px; background:var(--bg-elevated); flex-wrap:wrap; gap:8px; cursor:pointer; user-select:none; }
    .repo-header:hover { background:var(--accent-hover); }
    .chev { color:var(--sub); font-size:10px; width:10px; display:inline-block; }
    .repo-link { color:var(--accent); text-decoration:none; font-weight:600; font-size:13px; }
    .repo-link:hover { text-decoration:underline; }
    .repo-desc { color:var(--sub); font-size:12px; flex:1; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
    .repo-detail { padding:4px 0; border-top:1px solid var(--border); }
    .loading-row { padding:10px 14px; color:var(--sub); font-size:13px; }
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
    .pill { border-radius:10px; padding:1px 8px; font-size:11px; flex-shrink:0; }
    .pill.open { background:rgba(52,120,246,0.15); color:var(--accent); }
    </style>"#;

    let full_body = format!("{}{}", style, body);
    render_page("GitHub", "github", &full_body, script)
}
