//! Todos dashboard.

use super::common::{collect_kv, count_query, now_human, open_db, render_page, today};
use serde_json::{json, Value};

fn aggregate() -> Value {
    let conn = match open_db() {
        Ok(c) => c,
        Err(e) => return json!({ "error": format!("{e}"), "generated_at": now_human() }),
    };
    let t = today();

    let total = count_query(&conn, "SELECT COUNT(*) FROM todos");
    let done = count_query(&conn, "SELECT COUNT(*) FROM todos WHERE status='done'");
    let in_progress = count_query(&conn, "SELECT COUNT(*) FROM todos WHERE status='in_progress'");
    let blocked = count_query(&conn, "SELECT COUNT(*) FROM todos WHERE status='blocked'");
    let pending = count_query(
        &conn,
        "SELECT COUNT(*) FROM todos WHERE status IN ('todo','') OR status IS NULL",
    );
    let overdue = conn
        .query_row(
            "SELECT COUNT(*) FROM todos WHERE due_date IS NOT NULL AND due_date != '' \
             AND due_date < ?1 AND status != 'done'",
            [&t],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);
    let done_today = conn
        .query_row(
            "SELECT COUNT(*) FROM todos WHERE substr(COALESCE(completed_at,''),1,10) = ?1",
            [&t],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);
    let added_today = conn
        .query_row(
            "SELECT COUNT(*) FROM todos WHERE substr(created_at,1,10) = ?1",
            [&t],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);

    let priority = collect_kv(
        &conn,
        "SELECT CAST(COALESCE(priority,3) AS TEXT), COUNT(*) \
         FROM todos GROUP BY priority ORDER BY priority",
    );

    let mut recent: Vec<Value> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, text, status, COALESCE(priority,3), substr(created_at,1,10) \
         FROM todos ORDER BY id DESC LIMIT 8",
    ) {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok(json!({
                "id":       r.get::<_, i64>(0)?,
                "text":     r.get::<_, String>(1)?,
                "status":   r.get::<_, String>(2).unwrap_or_default(),
                "priority": r.get::<_, i64>(3)?,
                "date":     r.get::<_, String>(4).unwrap_or_default(),
            }))
        }) {
            for v in rows.flatten() {
                recent.push(v);
            }
        }
    }

    let trend_completed = collect_kv(
        &conn,
        "SELECT substr(completed_at,1,10) AS d, COUNT(*) FROM todos \
         WHERE completed_at IS NOT NULL AND completed_at != '' \
         GROUP BY d ORDER BY d DESC LIMIT 14",
    );
    let trend_created = collect_kv(
        &conn,
        "SELECT substr(created_at,1,10) AS d, COUNT(*) FROM todos \
         GROUP BY d ORDER BY d DESC LIMIT 14",
    );

    let mut tag_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT tags FROM todos WHERE tags IS NOT NULL AND tags != ''")
    {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for s in rows.flatten() {
                for tag in s.split(',') {
                    let tag = tag.trim();
                    if !tag.is_empty() {
                        *tag_counts.entry(tag.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    let mut top_tags: Vec<(String, i64)> = tag_counts.into_iter().collect();
    top_tags.sort_by(|a, b| b.1.cmp(&a.1));
    top_tags.truncate(10);

    json!({
        "generated_at": now_human(),
        "total": total, "done": done, "pending": pending,
        "in_progress": in_progress, "blocked": blocked,
        "overdue": overdue, "done_today": done_today, "added_today": added_today,
        "priority": priority.into_iter()
            .map(|(k,v)| json!({"priority":k,"count":v})).collect::<Vec<_>>(),
        "recent": recent,
        "trend_created": trend_created.into_iter()
            .map(|(d,c)| json!({"date":d,"count":c})).collect::<Vec<_>>(),
        "trend_completed": trend_completed.into_iter()
            .map(|(d,c)| json!({"date":d,"count":c})).collect::<Vec<_>>(),
        "top_tags": top_tags.into_iter()
            .map(|(t,c)| json!({"tag":t,"count":c})).collect::<Vec<_>>(),
    })
}

pub fn todos_data_json() -> String {
    serde_json::to_string(&aggregate()).unwrap_or_else(|_| "{}".to_string())
}

pub fn todos_html() -> String {
    let body = r##"
    <h1 class="page-title">Todos</h1>
    <div class="grid-stats" id="stats"></div>
    <div class="grid-main">
      <section class="card">
        <h2>Completion</h2>
        <div class="donut-wrap">
          <svg class="donut" viewBox="0 0 120 120">
            <circle class="track" cx="60" cy="60" r="50"></circle>
            <circle id="donut-arc" cx="60" cy="60" r="50"
              stroke="var(--green)" stroke-dasharray="0 314" stroke-linecap="round"></circle>
          </svg>
          <div class="donut-center">
            <div class="pct" id="donut-pct">0%</div>
            <div class="lbl">completion</div>
            <div class="legend" style="margin-top:10px">
              <div class="item"><span class="dot" style="background:var(--green)"></span><span id="leg-done">0 done</span></div>
              <div class="item"><span class="dot" style="background:#6b7287"></span><span id="leg-open">0 open</span></div>
            </div>
          </div>
        </div>
        <div style="margin-top:14px" id="status-bars"></div>
      </section>

      <section class="card">
        <h2>Activity (last 14 days)</h2>
        <div style="font-size:11px;color:var(--sub);margin-bottom:6px">
          <span style="color:var(--accent)">■</span> Created &nbsp;
          <span style="color:var(--green)">■</span> Completed
        </div>
        <div class="chart" id="chart-created"></div>
        <div class="chart-axis" id="chart-axis"></div>
        <div class="chart" id="chart-completed" style="margin-top:8px"></div>
      </section>

      <section class="card">
        <h2>Priority Distribution</h2>
        <div id="priority-bars"></div>
        <h2 style="margin-top:18px">Top Tags</h2>
        <div class="tag-cloud" id="tag-cloud"></div>
      </section>

      <section class="card">
        <h2>Recent Todos</h2>
        <div class="list" id="recent"></div>
      </section>
    </div>
    "##;

    let script = r##"
    const PRI_LABEL = {1:'Critical',2:'High',3:'Normal',4:'Low',5:'Minimal'};
    function statusLabel(s){return ({todo:'Todo',in_progress:'In progress',blocked:'Blocked',done:'Done'}[s])||(s||'Todo');}
    function statCard(label,value,sub,cls){return `<div class="stat ${cls||''}"><div class="label">${esc(label)}</div><div class="value">${esc(value)}</div>${sub?`<div class="sub">${esc(sub)}</div>`:''}</div>`;}

    function renderStats(d){
      const pct = d.total>0 ? Math.round((d.done/d.total)*100) : 0;
      document.getElementById('stats').innerHTML = [
        statCard('Total', d.total, `${d.added_today} added today`, 'accent'),
        statCard('Completed', d.done, `${d.done_today} today · ${pct}%`, 'green'),
        statCard('In progress', d.in_progress, `${d.pending} pending`, 'cyan'),
        statCard('Blocked', d.blocked, d.overdue?`${d.overdue} overdue`:'on track', d.blocked||d.overdue?'red':''),
      ].join('');
    }
    function renderDonut(d){
      const pct = d.total>0 ? (d.done/d.total) : 0;
      const circ = 2*Math.PI*50;
      document.getElementById('donut-arc').setAttribute('stroke-dasharray',`${(pct*circ).toFixed(2)} ${circ}`);
      document.getElementById('donut-pct').textContent = Math.round(pct*100)+'%';
      document.getElementById('leg-done').textContent = `${d.done} done`;
      document.getElementById('leg-open').textContent = `${d.pending + d.in_progress + d.blocked} open`;
    }
    function renderStatusBars(d){
      const max = Math.max(1, d.total);
      const row = (k,l,c) => `<div class="bar-row"><div class="label">${l}</div><div class="bar ${k}"><span style="width:${(c/max)*100}%"></span></div><div class="count">${c}</div></div>`;
      document.getElementById('status-bars').innerHTML = [
        row('todo','Todo',d.pending),
        row('in_progress','In progress',d.in_progress),
        row('blocked','Blocked',d.blocked),
        row('done','Done',d.done),
      ].join('');
    }
    function renderPriority(d){
      const rows = d.priority||[];
      const max = Math.max(1, ...rows.map(r=>r.count));
      document.getElementById('priority-bars').innerHTML = rows.map(r=>{
        const p = parseInt(r.priority,10)||3;
        return `<div class="bar-row"><div class="label"><span class="pri pri-${p}">${PRI_LABEL[p]||'Normal'}</span></div><div class="bar in_progress"><span style="width:${(r.count/max)*100}%"></span></div><div class="count">${r.count}</div></div>`;
      }).join('') || '<div class="empty">No todos yet.</div>';
    }
    function renderTags(d){
      const tags = d.top_tags||[];
      const el = document.getElementById('tag-cloud');
      el.innerHTML = tags.length
        ? tags.map(t=>`<span class="tag">${esc(t.tag)} <span class="c">${t.count}</span></span>`).join('')
        : '<div class="empty">No tags.</div>';
    }
    function renderRecent(d){
      const items = d.recent||[];
      const el = document.getElementById('recent');
      el.innerHTML = items.length
        ? items.map(it=>{
            const s = it.status||'todo';
            return `<div class="item"><div class="id">#${it.id}</div><div class="body">${esc(it.text)}</div><span class="pri pri-${it.priority||3}">P${it.priority||3}</span><span class="badge ${s}">${statusLabel(s)}</span><div class="meta">${esc(it.date)}</div></div>`;
          }).join('')
        : '<div class="empty">No todos yet.</div>';
    }
    function buildChart(elId, series, colorClass){
      const reversed = [...series].reverse();
      const max = Math.max(1, ...reversed.map(s=>s.count));
      document.getElementById(elId).innerHTML = reversed.map(s=>
        `<div class="col ${colorClass||''}" title="${esc(s.date)}: ${s.count}"><span style="height:${(s.count/max)*100}%"></span></div>`
      ).join('') || '<div class="empty">No activity.</div>';
      return reversed;
    }
    function renderTrends(d){
      const created   = buildChart('chart-created',   d.trend_created||[],   '');
      const completed = buildChart('chart-completed', d.trend_completed||[], 'b');
      const base = created.length >= completed.length ? created : completed;
      document.getElementById('chart-axis').innerHTML = base.map(s =>
        `<div class="col">${esc((s.date||'').slice(5))}</div>`).join('');
    }
    async function refresh(){
      try {
        const res = await fetch('/dashboard/todos/data', { cache:'no-store' });
        if (!res.ok) return;
        const d = await res.json();
        if (d.error) { document.querySelector('main').innerHTML = `<pre style="color:#f06c6c">${esc(d.error)}</pre>`; return; }
        renderStats(d); renderDonut(d); renderStatusBars(d);
        renderPriority(d); renderTags(d); renderRecent(d); renderTrends(d);
        stampGenerated(d);
      } catch(_){}
    }
    refresh(); setInterval(refresh, 5000);
    "##;

    render_page("Todos", "todos", body, script)
}
