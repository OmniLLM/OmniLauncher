//! AI conversation dashboard.

use super::common::{collect_kv, count_query, now_human, open_db, render_page, today};
use serde_json::{json, Value};

/// Build per-session views from the real `conversation_sessions` table,
/// newest-first.
fn build_sessions(conn: &rusqlite::Connection) -> Vec<Value> {
    struct Row {
        id: i64,
        title: String,
        created_at: String,
        last_active_at: String,
        user_count: i64,
        ai_count: i64,
    }

    let mut sessions: Vec<Row> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT s.id, COALESCE(s.title,''), \
                COALESCE(s.created_at,''), COALESCE(s.last_active_at,''), \
                (SELECT COUNT(*) FROM conversation_messages m \
                 WHERE m.session_id = s.id AND m.role='user') AS uc, \
                (SELECT COUNT(*) FROM conversation_messages m \
                 WHERE m.session_id = s.id AND m.role='assistant') AS ac \
         FROM conversation_sessions s \
         ORDER BY datetime(s.last_active_at) DESC, s.id DESC",
    ) {
        if let Ok(it) = stmt.query_map([], |r| {
            Ok(Row {
                id: r.get(0)?,
                title: r.get(1)?,
                created_at: r.get(2)?,
                last_active_at: r.get(3)?,
                user_count: r.get(4)?,
                ai_count: r.get(5)?,
            })
        }) {
            for v in it.flatten() {
                sessions.push(v);
            }
        }
    }

    // Hide empty sessions (e.g. user clicked "New conversation" then never
    // typed) except the most recent one — that's the active session.
    let head_id = sessions.first().map(|s| s.id).unwrap_or(0);
    sessions.retain(|s| s.id == head_id || s.user_count + s.ai_count > 0);

    sessions
        .into_iter()
        .enumerate()
        .map(|(idx, s)| {
            // Pull up to 40 messages per session for the inline preview.
            let mut messages: Vec<Value> = Vec::new();
            if let Ok(mut stmt) = conn.prepare(
                "SELECT id, role, COALESCE(content,''), COALESCE(created_at,'') \
                 FROM conversation_messages \
                 WHERE session_id = ?1 \
                   AND role IN ('user','assistant') \
                   AND content IS NOT NULL AND content != '' \
                 ORDER BY id ASC LIMIT 40",
            ) {
                if let Ok(it) = stmt.query_map([s.id], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                }) {
                    for (id, role, content, at) in it.flatten() {
                        messages.push(json!({
                            "id": id, "role": role, "content": content,
                            "at": at.chars().take(16).collect::<String>(),
                        }));
                    }
                }
            }

            let title = if s.title.trim().is_empty() {
                format!("Session #{}", s.id)
            } else {
                s.title.clone()
            };

            let duration_secs = match (
                chrono::NaiveDateTime::parse_from_str(&s.created_at, "%Y-%m-%d %H:%M:%S"),
                chrono::NaiveDateTime::parse_from_str(&s.last_active_at, "%Y-%m-%d %H:%M:%S"),
            ) {
                (Ok(a), Ok(b)) => (b - a).num_seconds().max(0),
                _ => 0,
            };

            json!({
                "index": idx + 1,
                "id": s.id,
                "title": title,
                "first_id": s.id,
                "started_at": s.created_at.chars().take(16).collect::<String>(),
                "ended_at":   s.last_active_at.chars().take(16).collect::<String>(),
                "duration_secs": duration_secs,
                "user_count": s.user_count,
                "ai_count": s.ai_count,
                "total": s.user_count + s.ai_count,
                "messages": messages,
            })
        })
        .collect()
}

fn aggregate() -> Value {
    let conn = match open_db() {
        Ok(c) => c,
        Err(e) => return json!({ "error": format!("{e}"), "generated_at": now_human() }),
    };
    let t = today();

    let total = count_query(&conn, "SELECT COUNT(*) FROM conversation_messages");
    let user = count_query(
        &conn,
        "SELECT COUNT(*) FROM conversation_messages WHERE role='user'",
    );
    let assistant = count_query(
        &conn,
        "SELECT COUNT(*) FROM conversation_messages WHERE role='assistant'",
    );
    let today_count = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_messages WHERE substr(created_at,1,10) = ?1",
            [&t],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);
    let avg_user_len = conn
        .query_row(
            "SELECT COALESCE(AVG(LENGTH(content)),0) FROM conversation_messages WHERE role='user'",
            [],
            |r| r.get::<_, f64>(0),
        )
        .unwrap_or(0.0) as i64;
    let avg_ai_len = conn
        .query_row(
            "SELECT COALESCE(AVG(LENGTH(content)),0) FROM conversation_messages WHERE role='assistant'",
            [],
            |r| r.get::<_, f64>(0),
        )
        .unwrap_or(0.0) as i64;

    let trend = collect_kv(
        &conn,
        "SELECT substr(created_at,1,10) AS d, COUNT(*) FROM conversation_messages \
         GROUP BY d ORDER BY d DESC LIMIT 14",
    );

    let by_hour = collect_kv(
        &conn,
        "SELECT substr(created_at,12,2) AS h, COUNT(*) FROM conversation_messages \
         GROUP BY h ORDER BY h",
    );

    let sessions = build_sessions(&conn);
    let session_count = sessions.len() as i64;

    json!({
        "generated_at": now_human(),
        "total": total, "user": user, "assistant": assistant, "today": today_count,
        "avg_user_len": avg_user_len, "avg_ai_len": avg_ai_len,
        "session_count": session_count,
        "trend": trend.into_iter()
            .map(|(d,c)| json!({"date":d,"count":c})).collect::<Vec<_>>(),
        "by_hour": by_hour.into_iter()
            .map(|(h,c)| json!({"hour":h,"count":c})).collect::<Vec<_>>(),
        "sessions": sessions,
    })
}

pub fn conversation_data_json() -> String {
    serde_json::to_string(&aggregate()).unwrap_or_else(|_| "{}".to_string())
}

pub fn conversation_html() -> String {
    let body = r##"
    <h1 class="page-title">AI Conversation</h1>
    <div class="grid-stats" id="stats"></div>
    <div class="grid-main">
      <section class="card">
        <h2>Daily Activity (last 14 days)</h2>
        <div class="chart" id="chart-daily"></div>
        <div class="chart-axis" id="axis-daily"></div>
      </section>
      <section class="card">
        <h2>Hour of Day</h2>
        <div class="chart" id="chart-hour"></div>
        <div class="chart-axis" id="axis-hour"></div>
      </section>
    </div>

    <section class="card mt-4">
      <h2 class="flex items-center gap-2.5">
        Sessions
        <span id="session-note" class="text-[11px] text-slate-500 font-normal normal-case tracking-normal"></span>
      </h2>
      <div id="sessions" class="flex flex-col gap-2.5"></div>
    </section>
    "##;

    let script = r##"
    function statCard(label,value,sub,cls){return `<div class="stat ${cls||''}"><div class="label">${esc(label)}</div><div class="value">${esc(value)}</div>${sub?`<div class="sub">${esc(sub)}</div>`:''}</div>`;}
    function renderStats(d){
      document.getElementById('stats').innerHTML = [
        statCard('Sessions', d.session_count, 'persisted chats', 'green'),
        statCard('Total messages', d.total, `${d.today} today`, 'purple'),
        statCard('Your messages', d.user, `~${d.avg_user_len} chars avg`, 'accent'),
        statCard('AI replies', d.assistant, `~${d.avg_ai_len} chars avg`, 'cyan'),
      ].join('');
      document.getElementById('session-note').textContent =
        'Persisted in conversation_sessions';
    }
    function buildChart(elId, axisId, series, labelFn, colorClass){
      const reversed = [...series].reverse();
      const max = Math.max(1, ...reversed.map(s=>s.count));
      document.getElementById(elId).innerHTML = reversed.map(s =>
        `<div class="col ${colorClass||''}" title="${esc(labelFn(s))}: ${s.count}"><span style="height:${(s.count/max)*100}%"></span></div>`
      ).join('') || '<div class="empty">No activity.</div>';
      if (axisId) {
        document.getElementById(axisId).innerHTML = reversed.map(s =>
          `<div class="col">${esc(labelFn(s))}</div>`).join('');
      }
    }
    function fmtDuration(secs){
      if (!secs) return 'instant';
      if (secs < 60) return secs + 's';
      const m = Math.floor(secs/60), s = secs%60;
      if (m < 60) return s ? `${m}m ${s}s` : `${m}m`;
      const h = Math.floor(m/60), mm = m%60;
      return mm ? `${h}h ${mm}m` : `${h}h`;
    }
    function renderSessions(d){
      const sessions = d.sessions||[];
      const el = document.getElementById('sessions');
      if (!sessions.length) {
        el.innerHTML = '<div class="empty">No conversations yet.</div>';
        return;
      }
      // Preserve open state across refreshes
      const open = new Set(
        [...document.querySelectorAll('.session.open')].map(n => n.dataset.key)
      );
      // First session expanded by default if nothing else is open
      if (!open.size && sessions[0]) open.add(String(sessions[0].first_id));

      el.innerHTML = sessions.map(s => {
        const key = String(s.first_id);
        const msgs = (s.messages||[]).map(m => {
          const role = m.role==='assistant'?'assistant':'user';
          return `<div class="msg ${role}"><div class="head"><span>${role==='user'?'You':'AI'}</span><span>${esc(m.at||'')}</span></div><div class="content">${esc(m.content||'')}</div></div>`;
        }).join('');
        const range = s.started_at === s.ended_at ? esc(s.started_at) : `${esc(s.started_at)} → ${esc((s.ended_at||'').slice(-5))}`;
        return `<div class="session ${open.has(key)?'open':''}" data-key="${key}">
          <div class="session-head" onclick="toggleSession('${key}')">
            <span class="caret">▸</span>
            <span class="num">#${s.index}</span>
            <span class="title">${esc(s.title)}</span>
            <span class="pill u">${s.user_count} you</span>
            <span class="pill a">${s.ai_count} ai</span>
            <span class="pill">${fmtDuration(s.duration_secs)}</span>
            <span class="when">${range}</span>
          </div>
          ${open.has(key) ? `<div class="session-body">${msgs}</div>` : ''}
        </div>`;
      }).join('');
    }
    function toggleSession(key){
      const node = document.querySelector(`.session[data-key="${key}"]`);
      if (!node) return;
      if (node.classList.contains('open')) {
        node.classList.remove('open');
        const body = node.querySelector('.session-body'); if (body) body.remove();
      } else {
        node.classList.add('open');
        refresh(); // re-render so messages appear
      }
    }
    async function refresh(){
      try {
        const res = await fetch('/dashboard/conversation/data',{cache:'no-store'});
        if (!res.ok) return;
        const d = await res.json();
        if (d.error) { document.querySelector('main').innerHTML = `<pre style="color:#f06c6c">${esc(d.error)}</pre>`; return; }
        renderStats(d);
        buildChart('chart-daily','axis-daily', d.trend||[], s=>(s.date||'').slice(5), '');
        buildChart('chart-hour','axis-hour',   d.by_hour||[], s=>`${s.hour||''}h`, 'b');
        renderSessions(d);
        stampGenerated(d);
      } catch(_){}
    }
    refresh(); setInterval(refresh, 5000);
    "##;

    render_page("Conversation", "conversation", body, script)
}
