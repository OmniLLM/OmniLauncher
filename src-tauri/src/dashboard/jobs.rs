//! Scheduled jobs dashboard.

use super::common::{count_query, now_human, open_db, render_page};
use serde_json::{json, Value};

fn aggregate() -> Value {
    let conn = match open_db() {
        Ok(c) => c,
        Err(e) => return json!({ "error": format!("{e}"), "generated_at": now_human() }),
    };

    let total = count_query(&conn, "SELECT COUNT(*) FROM scheduled_jobs");
    let enabled = count_query(
        &conn,
        "SELECT COUNT(*) FROM scheduled_jobs WHERE enabled=1",
    );
    let runs = count_query(
        &conn,
        "SELECT COALESCE(SUM(run_count),0) FROM scheduled_jobs",
    );

    let mut jobs: Vec<Value> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, label, schedule, command, enabled, COALESCE(run_count,0), \
                COALESCE(last_run,''), COALESCE(next_run,''), substr(created_at,1,10) \
         FROM scheduled_jobs ORDER BY id DESC",
    ) {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok(json!({
                "id":       r.get::<_, i64>(0)?,
                "label":    r.get::<_, String>(1)?,
                "schedule": r.get::<_, String>(2)?,
                "command":  r.get::<_, String>(3)?,
                "enabled":  r.get::<_, i64>(4)? != 0,
                "runs":     r.get::<_, i64>(5)?,
                "last_run": r.get::<_, String>(6)?,
                "next_run": r.get::<_, String>(7)?,
                "created":  r.get::<_, String>(8).unwrap_or_default(),
            }))
        }) {
            for v in rows.flatten() {
                jobs.push(v);
            }
        }
    }

    json!({
        "generated_at": now_human(),
        "total": total, "enabled": enabled, "runs": runs,
        "jobs": jobs,
    })
}

pub fn jobs_data_json() -> String {
    serde_json::to_string(&aggregate()).unwrap_or_else(|_| "{}".to_string())
}

pub fn jobs_html() -> String {
    let body = r##"
    <h1 class="page-title">Scheduler</h1>
    <div class="grid-stats" id="stats"></div>
    <div class="grid-main">
      <section class="card" style="grid-column: 1 / -1">
        <h2>All Scheduled Jobs</h2>
        <div id="jobs" class="flex flex-col gap-2.5"></div>
      </section>
    </div>
    "##;

    let script = r##"
    function statCard(label,value,sub,cls){return `<div class="stat ${cls||''}"><div class="label">${esc(label)}</div><div class="value">${esc(value)}</div>${sub?`<div class="sub">${esc(sub)}</div>`:''}</div>`;}
    function renderStats(d){
      document.getElementById('stats').innerHTML = [
        statCard('Total jobs', d.total, `${d.enabled} enabled`, 'yellow'),
        statCard('Enabled', d.enabled, d.total-d.enabled?`${d.total-d.enabled} disabled`:'all running','green'),
        statCard('Total runs', d.runs, 'lifetime executions', 'accent'),
      ].join('');
    }
    function renderJobs(d){
      const items = d.jobs||[];
      const el = document.getElementById('jobs');
      if (!items.length) { el.innerHTML='<div class="empty">No scheduled jobs.</div>'; return; }
      el.innerHTML = items.map(j => `
        <div class="job-row">
          <div class="row-head">
            <span class="id">#${j.id}</span>
            <span class="label">${esc(j.label)}</span>
            <span class="badge ${j.enabled?'done':'todo'}">${j.enabled?'ON':'OFF'}</span>
            <span class="meta">${j.runs} runs · created ${esc(j.created||'')}</span>
          </div>
          <div class="row-grid">
            <div><span class="k">Schedule:</span> <code>${esc(j.schedule)}</code></div>
            <div><span class="k">Last run:</span> ${esc(j.last_run||'—')}</div>
            <div><span class="k">Next run:</span> ${esc(j.next_run||'—')}</div>
          </div>
          <div class="cmd">${esc(j.command)}</div>
        </div>`).join('');
    }
    async function refresh(){
      try {
        const res = await fetch('/dashboard/jobs/data',{cache:'no-store'});
        if (!res.ok) return;
        const d = await res.json();
        if (d.error) { document.querySelector('main').innerHTML = `<pre style="color:#f06c6c">${esc(d.error)}</pre>`; return; }
        renderStats(d); renderJobs(d);
        stampGenerated(d);
      } catch(_){}
    }
    refresh(); setInterval(refresh, 5000);
    "##;

    render_page("Scheduler", "jobs", body, script)
}
