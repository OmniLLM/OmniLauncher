//! Dashboard index page — landing page listing all available dashboards.

use super::common::{self, count_query, now_human, open_db, render_page, today};
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
    let jobs_enabled = count_query(&conn, "SELECT COUNT(*) FROM scheduled_jobs WHERE enabled=1");

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
    <p class="page-lede">
      Live views of OmniLauncher's local data. Click any card to open the full dashboard.
    </p>
    <div class="dash-grid" id="cards"></div>
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
      { key: "github",       href: "/dashboard/github",       icon: "⚙",
        title: "GitHub",
        desc: "Open issues and pull requests grouped by org and repo. Filter by org, repo, or type.",
        accent: "#8ab4f8", glow: "rgba(138,180,248,0.18)",
        metrics: d => [] },
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
