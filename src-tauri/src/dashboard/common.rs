//! Shared helpers and HTML layout for dashboard pages.

use crate::{db, path_config};
use rusqlite::Connection;

pub fn open_db() -> rusqlite::Result<Connection> {
    let dir = path_config::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let conn = Connection::open(dir.join("omnilauncher.sqlite"))?;
    db::run_migrations(&conn)?;
    Ok(conn)
}

pub fn count_query(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0)
}

pub fn collect_kv(conn: &Connection, sql: &str) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(sql) {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        }) {
            for row in rows.flatten() {
                out.push(row);
            }
        }
    }
    out
}

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub fn now_human() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Shared CSS variables, base layout, top nav. The page-specific body html
/// and the data-loading script are injected by each page.
///
/// `{title}`, `{active}`, `{body}`, `{script}` are placeholder tokens replaced
/// at render time. No `format!` is used to avoid `{}` escaping issues with the
/// inline JS.
pub fn render_page(title: &str, active: &str, body: &str, script: &str) -> String {
    LAYOUT
        .replace("{{TITLE}}", title)
        .replace("{{ACTIVE}}", active)
        .replace("{{BODY}}", body)
        .replace("{{SCRIPT}}", script)
}

const LAYOUT: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width,initial-scale=1" />
<title>OmniLauncher · {{TITLE}}</title>
<style>
  :root {
    --bg:        #0f1117;
    --bg-2:      #141826;
    --surface:   #1b2030;
    --surface-2: #232839;
    --border:    #2a3147;
    --text:      #e6e8ef;
    --sub:       #9aa3b6;
    --accent:    #5ea1ff;
    --green:     #61d09a;
    --yellow:    #e7c66b;
    --red:       #f06c6c;
    --purple:    #b389f3;
    --cyan:      #5fd0d8;
  }
  * { box-sizing: border-box; }
  html, body { margin:0; padding:0; background: var(--bg); color: var(--text);
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, Roboto, sans-serif;
    -webkit-font-smoothing: antialiased; }
  body {
    background:
      radial-gradient(1200px 600px at 90% -10%, rgba(94,161,255,0.08), transparent 60%),
      radial-gradient(900px 500px at -10% 20%, rgba(179,137,243,0.07), transparent 60%),
      var(--bg);
    min-height: 100vh;
  }

  nav.topbar {
    position: sticky; top: 0; z-index: 10;
    backdrop-filter: blur(10px);
    background: rgba(15,17,23,0.78);
    border-bottom: 1px solid var(--border);
    padding: 12px 32px;
    display: flex; align-items: center; gap: 20px; flex-wrap: wrap;
  }
  .nav-container {
    max-width: 1400px;
    width: 100%;
    margin: 0 auto;
    display: flex;
    align-items: center;
    gap: 20px;
    flex-wrap: wrap;
  }
  nav.topbar .brand { font-weight: 700; font-size: 15px; letter-spacing: 0.01em; }
  nav.topbar .brand .accent { color: var(--accent); }
  nav.topbar .links { display: flex; gap: 4px; flex-wrap: wrap; }
  nav.topbar a {
    color: var(--sub); text-decoration: none;
    padding: 6px 12px; border-radius: 8px; font-size: 13px;
    transition: background 150ms, color 150ms;
  }
  nav.topbar a:hover { background: var(--surface); color: var(--text); }
  nav.topbar a.active {
    background: var(--surface); color: var(--text);
    box-shadow: inset 0 -2px 0 var(--accent);
  }
  nav.topbar .right {
    margin-left: auto; color: var(--sub); font-size: 12px;
  }
  nav.topbar .pulse {
    display:inline-block; width:8px; height:8px; border-radius:50%;
    background: var(--green); margin-right:6px; vertical-align: middle;
    box-shadow: 0 0 0 0 rgba(97,208,154,0.6); animation: pulse 1.6s infinite;
  }
  @keyframes pulse {
    0%   { box-shadow: 0 0 0 0 rgba(97,208,154,0.6); }
    70%  { box-shadow: 0 0 0 10px rgba(97,208,154,0); }
    100% { box-shadow: 0 0 0 0 rgba(97,208,154,0); }
  }

  main { max-width: 1400px; margin: 0 auto; padding: 24px 32px 60px; }
  h1.page-title { font-size: 22px; margin: 0 0 24px; font-weight: 700; }

  .grid-stats { display: grid; gap: 14px;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); margin-bottom: 24px; }
  .stat {
    background: linear-gradient(180deg, var(--surface), var(--bg-2));
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 14px 16px;
    box-shadow: 0 6px 18px rgba(0,0,0,0.18);
  }
  .stat .label { color: var(--sub); font-size: 11px; text-transform: uppercase;
    letter-spacing: 0.08em; margin-bottom: 6px; }
  .stat .value { font-size: 28px; font-weight: 700; line-height: 1.1; }
  .stat .sub { color: var(--sub); font-size: 12px; margin-top: 4px; }
  .stat.green  .value { color: var(--green);  }
  .stat.yellow .value { color: var(--yellow); }
  .stat.red    .value { color: var(--red);    }
  .stat.accent .value { color: var(--accent); }
  .stat.purple .value { color: var(--purple); }
  .stat.cyan   .value { color: var(--cyan);   }

  .grid-main { display: grid; gap: 16px;
    grid-template-columns: repeat(auto-fit, minmax(360px, 1fr)); }
  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 16px 18px;
    box-shadow: 0 6px 18px rgba(0,0,0,0.18);
  }
  .card h2 { font-size: 13px; text-transform: uppercase; letter-spacing: 0.08em;
    color: var(--sub); margin: 0 0 12px; font-weight: 600; }

  .bar-row { display:flex; align-items:center; gap: 10px; margin-bottom: 8px;
    font-size: 13px; }
  .bar-row .label { min-width: 110px; color: var(--text); }
  .bar-row .count { min-width: 36px; text-align: right; color: var(--sub); }
  .bar { flex: 1; height: 8px; background: var(--surface-2);
    border-radius: 999px; overflow: hidden; }
  .bar > span { display:block; height: 100%; border-radius: 999px;
    transition: width 360ms ease; }
  .bar.todo > span        { background: linear-gradient(90deg, #6b7287, #8a93ad); }
  .bar.in_progress > span { background: linear-gradient(90deg, #5ea1ff, #82bdff); }
  .bar.blocked > span     { background: linear-gradient(90deg, #f06c6c, #ff9c9c); }
  .bar.done > span        { background: linear-gradient(90deg, #61d09a, #88e3b6); }

  .chart { display: flex; align-items: flex-end; gap: 4px;
    height: 88px; padding: 6px 2px 0; }
  .chart .col { flex: 1; display: flex; flex-direction: column;
    align-items: stretch; justify-content: flex-end; min-width: 8px; }
  .chart .col > span {
    background: linear-gradient(180deg, var(--accent), #2d6fcc);
    border-radius: 4px 4px 0 0; min-height: 2px;
    transition: height 360ms ease;
  }
  .chart .col.b > span { background: linear-gradient(180deg, var(--green), #2f8a64); }
  .chart-axis { display:flex; gap:4px; margin-top: 6px;
    color: var(--sub); font-size: 10px; }
  .chart-axis .col { flex: 1; text-align:center; min-width: 8px;
    overflow:hidden; white-space:nowrap; }

  .list { display:flex; flex-direction:column; gap: 8px; }
  .list .item {
    display:flex; align-items:center; gap: 10px;
    padding: 8px 10px; background: var(--bg-2);
    border: 1px solid var(--border); border-radius: 10px;
    font-size: 13px;
  }
  .list .item .id { color: var(--sub); font-size: 11px;
    font-family: ui-monospace, "Cascadia Code", monospace; min-width: 36px; }
  .list .item .body { flex: 1; overflow: hidden; text-overflow: ellipsis;
    white-space: nowrap; }
  .list .item .meta { color: var(--sub); font-size: 11px; }

  .badge {
    display:inline-block; padding: 2px 8px; border-radius: 999px;
    font-size: 10px; font-weight: 600; letter-spacing: 0.04em;
    text-transform: uppercase; border: 1px solid transparent;
  }
  .badge.todo        { background:#2a3147; color:#a9b1c8; }
  .badge.in_progress { background:#1e3a66; color:#82bdff; border-color:#2d5694; }
  .badge.blocked     { background:#4a2027; color:#ffb0b0; border-color:#7a2f3a; }
  .badge.done        { background:#1e4a36; color:#88e3b6; border-color:#2c6e51; }

  .pri { font-size: 11px; font-weight: 600; padding: 1px 6px;
    border-radius: 4px; border: 1px solid transparent; }
  .pri-1 { color: #ff8a8a; background:#3a1f24; border-color:#5a2c33; }
  .pri-2 { color: #ffba6e; background:#3a2918; border-color:#5a3d24; }
  .pri-3 { color: #a9b1c8; background:#272c3d; border-color:#363c52; }
  .pri-4 { color: #82bdff; background:#1a2842; border-color:#2a3b5d; }
  .pri-5 { color: #88e3b6; background:#173527; border-color:#26513e; }

  .tag-cloud { display: flex; flex-wrap: wrap; gap: 6px; }
  .tag { display:inline-flex; gap:6px; align-items:center;
    padding: 4px 9px; border-radius: 999px;
    background: var(--bg-2); border: 1px solid var(--border); font-size: 11px; }
  .tag .c { color: var(--accent); font-weight: 700; }

  .msg { display:flex; flex-direction:column; gap:4px;
    padding: 10px 12px; border-radius: 10px;
    background: var(--bg-2); border: 1px solid var(--border);
    font-size: 12.5px; }
  .msg .head { display:flex; justify-content: space-between;
    color: var(--sub); font-size: 11px; }
  .msg.user      { border-left: 3px solid var(--accent); }
  .msg.assistant { border-left: 3px solid var(--purple); }
  .msg .content { color: var(--text); line-height: 1.45;
    display: -webkit-box; -webkit-line-clamp: 6; -webkit-box-orient: vertical;
    overflow: hidden; white-space: pre-wrap; }

  .empty { color: var(--sub); font-size: 12px; padding: 10px 4px; }

  .donut-wrap { display:flex; align-items:center; gap:18px; }
  .donut { width: 110px; height: 110px; flex-shrink:0; transform: rotate(-90deg); }
  .donut circle { fill: none; stroke-width: 14; }
  .donut .track { stroke: var(--surface-2); }
  .donut-center { display:flex; flex-direction:column; align-items:flex-start; }
  .donut-center .pct { font-size: 30px; font-weight: 700; }
  .donut-center .lbl { color: var(--sub); font-size: 11px;
    text-transform: uppercase; letter-spacing: 0.08em; }
  .legend { display:flex; flex-direction:column; gap:6px; font-size: 12px; }
  .legend .item { display:flex; align-items:center; gap:8px; color: var(--sub); }
  .legend .item .dot { width: 10px; height: 10px; border-radius: 50%; }

  footer { margin: 28px 32px; text-align:center; color: var(--sub); font-size: 11px; }
</style>
</head>
<body>
  <nav class="topbar">
    <div class="nav-container">
      <div class="brand"><span class="accent">Omni</span>Launcher</div>
      <div class="links">
        <a href="/dashboard"              data-key="index"        >Overview</a>
        <a href="/dashboard/todos"        data-key="todos"        >Todos</a>
        <a href="/dashboard/conversation" data-key="conversation" >AI Conversation</a>
        <a href="/dashboard/jobs"         data-key="jobs"         >Scheduler</a>
        <a href="/dashboard/tables"       data-key="tables"       >Database</a>
      </div>
      <div class="right"><span class="pulse"></span>Live · <span id="generated">—</span></div>
    </div>
  </nav>

  <main>
    {{BODY}}
  </main>

  <footer>Auto-refreshes every 5s · served by OmniLauncher live server</footer>

<script>
function esc(s) {
  return String(s ?? '').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}
function setActive() {
  const key = "{{ACTIVE}}";
  document.querySelectorAll('nav.topbar a').forEach(a => {
    if (a.dataset.key === key) a.classList.add('active');
  });
}
function stampGenerated(d) {
  const g = document.getElementById('generated');
  if (g && d && d.generated_at) g.textContent = d.generated_at;
}
setActive();
{{SCRIPT}}
</script>
</body>
</html>
"##;
