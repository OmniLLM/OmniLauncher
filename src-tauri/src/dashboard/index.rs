//! Dashboard index page — landing page listing all available dashboards.

use super::common::{
    self, count_query, now_human, open_db, render_page, today,
};
use serde_json::{json, Value};

fn summary() -> Value {
    let conn = match open_db() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("dashboard index: db open failed: {e}");
            return json!({ "generated_at": now_human(), "error": format!("{e}") });
        }
    };
    let t = today();

    // Discover every user table so we can show the database card count.
    let mut tables_count = 0i64;
    if let Ok(mut stmt) = conn.prepare(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' \
         AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '\\_%' ESCAPE '\\'",
    ) {
        if let Ok(n) = stmt.query_row([], |r| r.get::<_, i64>(0)) {
            tables_count = n;
        }
    }

    let todos_total = count_query(&conn, "SELECT COUNT(*) FROM todos");
    let todos_done = count_query(&conn, "SELECT COUNT(*) FROM todos WHERE status='done'");
    let todos_open = (todos_total - todos_done).max(0);
    let todos_today = conn
        .query_row(
            "SELECT COUNT(*) FROM todos WHERE substr(created_at,1,10) = ?1",
            [&t],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);

    let conv_total = count_query(&conn, "SELECT COUNT(*) FROM conversation_messages");
    let conv_today = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_messages WHERE substr(created_at,1,10) = ?1",
            [&t],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);

    let jobs_total = count_query(&conn, "SELECT COUNT(*) FROM scheduled_jobs");
    let jobs_enabled = count_query(
        &conn,
        "SELECT COUNT(*) FROM scheduled_jobs WHERE enabled=1",
    );

    json!({
        "generated_at": now_human(),
        "today": t,
        "todos":        { "total": todos_total, "open": todos_open, "done": todos_done, "today": todos_today },
        "conversation": { "total": conv_total, "today": conv_today },
        "jobs":         { "total": jobs_total, "enabled": jobs_enabled },
        "tables":       { "count": tables_count },
    })
}

pub fn index_html() -> String {
    let _ = common::open_db; // silence unused re-export when feature-gated
    let body = r##"
    <h1 class="page-title">Dashboards</h1>
    <p style="color:var(--sub);margin:-12px 0 24px;font-size:13px">
      Live views of OmniLauncher's local data. Click any card to open the full dashboard.
    </p>
    <div class="dash-grid" id="cards"></div>

    <style>
      .dash-grid {
        display: grid; gap: 18px;
        grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
      }
      .dash-card {
        display: flex; flex-direction: column; gap: 12px;
        padding: 22px;
        background: linear-gradient(180deg, var(--surface), var(--bg-2));
        border: 1px solid var(--border);
        border-radius: 16px;
        text-decoration: none;
        color: var(--text);
        box-shadow: 0 8px 24px rgba(0,0,0,0.22);
        transition: transform 180ms ease, box-shadow 180ms ease, border-color 180ms ease;
        position: relative;
        overflow: hidden;
      }
      .dash-card::before {
        content: ""; position: absolute; inset: 0;
        background: radial-gradient(400px 200px at 100% 0%, var(--card-glow, rgba(94,161,255,0.18)), transparent 70%);
        pointer-events: none;
      }
      .dash-card:hover {
        transform: translateY(-3px);
        border-color: var(--card-accent, var(--accent));
        box-shadow: 0 14px 32px rgba(0,0,0,0.32);
      }
      .dash-card .head {
        display: flex; align-items: center; gap: 12px;
        position: relative;
      }
      .dash-card .icon {
        width: 40px; height: 40px; flex-shrink: 0;
        border-radius: 11px;
        display: inline-flex; align-items: center; justify-content: center;
        font-size: 20px; font-weight: 700; color: #fff;
        background: var(--card-accent, var(--accent));
        box-shadow: 0 6px 14px rgba(0,0,0,0.28);
      }
      .dash-card .title { font-size: 16px; font-weight: 700; }
      .dash-card .desc { color: var(--sub); font-size: 12.5px; line-height: 1.5;
        position: relative; }
      .dash-card .metrics {
        display: flex; gap: 18px; margin-top: 4px;
        position: relative;
      }
      .dash-card .metric { display: flex; flex-direction: column; gap: 2px; }
      .dash-card .metric .v { font-size: 22px; font-weight: 700;
        color: var(--card-accent, var(--accent)); }
      .dash-card .metric .k { font-size: 10.5px; color: var(--sub);
        text-transform: uppercase; letter-spacing: 0.08em; }
      .dash-card .cta {
        margin-top: auto; padding-top: 6px;
        font-size: 12px; color: var(--sub);
        display: flex; align-items: center; gap: 6px;
        position: relative;
      }
      .dash-card .cta .arr {
        transition: transform 180ms ease;
      }
      .dash-card:hover .cta { color: var(--text); }
      .dash-card:hover .cta .arr { transform: translateX(4px); }
    </style>
    "##;

    let script = r##"
    const CARDS = [
      { key: "todos",        href: "/dashboard/todos",        icon: "✓",
        title: "Todos",
        desc: "Completion rate, priority distribution, overdue items, daily activity, and recent entries.",
        accent: "#61d09a", glow: "rgba(97,208,154,0.18)",
        metrics: d => [
          { k: "open",  v: d.todos.open  },
          { k: "done",  v: d.todos.done  },
          { k: "today", v: d.todos.today },
        ] },
      { key: "conversation", href: "/dashboard/conversation", icon: "✦",
        title: "AI Conversation",
        desc: "Total messages exchanged, 14-day activity, user-vs-assistant breakdown and recent turns.",
        accent: "#b389f3", glow: "rgba(179,137,243,0.18)",
        metrics: d => [
          { k: "total", v: d.conversation.total },
          { k: "today", v: d.conversation.today },
        ] },
      { key: "jobs",         href: "/dashboard/jobs",         icon: "⏱",
        title: "Scheduler",
        desc: "All scheduled jobs, enabled state, next run, total executions.",
        accent: "#e7c66b", glow: "rgba(231,198,107,0.18)",
        metrics: d => [
          { k: "enabled", v: d.jobs.enabled },
          { k: "total",   v: d.jobs.total },
        ] },
      { key: "tables",       href: "/dashboard/tables",       icon: "⛁",
        title: "Database",
        desc: "Auto-discovered SQLite tables with row counts, columns, and live sample rows.",
        accent: "#5fd0d8", glow: "rgba(95,208,216,0.18)",
        metrics: d => [
          { k: "tables", v: d.tables.count },
        ] },
    ];

    function renderCards(d) {
      const html = CARDS.map(c => {
        const ms = (c.metrics(d)||[]).map(m =>
          `<div class="metric"><div class="v">${esc(m.v)}</div><div class="k">${esc(m.k)}</div></div>`
        ).join('');
        return `<a class="dash-card" href="${c.href}"
                   style="--card-accent:${c.accent};--card-glow:${c.glow}">
          <div class="head">
            <div class="icon">${c.icon}</div>
            <div class="title">${esc(c.title)}</div>
          </div>
          <div class="desc">${esc(c.desc)}</div>
          <div class="metrics">${ms}</div>
          <div class="cta">Open dashboard <span class="arr">→</span></div>
        </a>`;
      }).join('');
      document.getElementById('cards').innerHTML = html;
    }

    async function refresh() {
      try {
        const res = await fetch('/dashboard/data', { cache: 'no-store' });
        if (!res.ok) return;
        const d = await res.json();
        if (d.error) {
          document.getElementById('cards').innerHTML =
            `<div class="empty">${esc(d.error)}</div>`;
          return;
        }
        renderCards(d);
        stampGenerated(d);
      } catch (_) {}
    }
    refresh();
    setInterval(refresh, 5000);
    "##;

    render_page("Dashboards", "index", body, script)
}

pub fn index_data_json() -> String {
    serde_json::to_string(&summary()).unwrap_or_else(|_| "{}".to_string())
}
