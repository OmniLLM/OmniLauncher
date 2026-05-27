//! Database tables dashboard — auto-discovers every user table at runtime.

use super::common::{count_query, now_human, open_db, render_page, today};
use rusqlite::Connection;
use serde_json::{json, Value};

fn discover_tables(conn: &Connection) -> Vec<Value> {
    let mut tables: Vec<String> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT name FROM sqlite_master \
         WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '\\_%' ESCAPE '\\' \
         ORDER BY name",
    ) {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for name in rows.flatten() {
                if name == "_migrations" {
                    continue;
                }
                tables.push(name);
            }
        }
    }

    let today_str = today();
    let mut out: Vec<Value> = Vec::new();
    for tname in tables {
        if !tname.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }

        let row_count = count_query(conn, &format!("SELECT COUNT(*) FROM \"{}\"", tname));

        let mut columns: Vec<String> = Vec::new();
        if let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info(\"{}\")", tname)) {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(1)) {
                for c in rows.flatten() {
                    columns.push(c);
                }
            }
        }
        let time_col = columns
            .iter()
            .find(|c| {
                let l = c.to_ascii_lowercase();
                l == "created_at" || l == "updated_at" || l == "ts" || l == "timestamp"
            })
            .cloned();

        let added_today = if let Some(col) = &time_col {
            conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM \"{}\" WHERE substr(\"{}\",1,10) = ?1",
                    tname, col
                ),
                [&today_str],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
        } else {
            0
        };

        let sample_cols: Vec<String> = columns.iter().take(5).cloned().collect();
        let mut samples: Vec<Value> = Vec::new();
        if !sample_cols.is_empty() {
            let col_list = sample_cols
                .iter()
                .map(|c| format!("\"{}\"", c))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "SELECT {} FROM \"{}\" ORDER BY rowid DESC LIMIT 6",
                col_list, tname
            );
            if let Ok(mut stmt) = conn.prepare(&sql) {
                if let Ok(rows) = stmt.query_map([], |r| {
                    let mut obj = serde_json::Map::new();
                    for (idx, cname) in sample_cols.iter().enumerate() {
                        let v: rusqlite::types::Value =
                            r.get(idx).unwrap_or(rusqlite::types::Value::Null);
                        let jv = match v {
                            rusqlite::types::Value::Null => Value::Null,
                            rusqlite::types::Value::Integer(i) => json!(i),
                            rusqlite::types::Value::Real(f) => json!(f),
                            rusqlite::types::Value::Text(s) => {
                                let t = if s.len() > 100 { format!("{}…", &s[..100]) } else { s };
                                Value::String(t)
                            }
                            rusqlite::types::Value::Blob(_) => Value::String("<blob>".into()),
                        };
                        obj.insert(cname.clone(), jv);
                    }
                    Ok(Value::Object(obj))
                }) {
                    for v in rows.flatten() {
                        samples.push(v);
                    }
                }
            }
        }

        out.push(json!({
            "name": tname,
            "rows": row_count,
            "added_today": added_today,
            "columns": columns,
            "samples": samples,
        }));
    }
    out
}

fn aggregate() -> Value {
    let conn = match open_db() {
        Ok(c) => c,
        Err(e) => return json!({ "error": format!("{e}"), "generated_at": now_human() }),
    };
    let tables = discover_tables(&conn);
    let total_rows: i64 = tables.iter().map(|t| t["rows"].as_i64().unwrap_or(0)).sum();
    let added_today: i64 = tables
        .iter()
        .map(|t| t["added_today"].as_i64().unwrap_or(0))
        .sum();
    json!({
        "generated_at": now_human(),
        "table_count": tables.len(),
        "total_rows": total_rows,
        "added_today": added_today,
        "tables": tables,
    })
}

pub fn tables_data_json() -> String {
    serde_json::to_string(&aggregate()).unwrap_or_else(|_| "{}".to_string())
}

pub fn tables_html() -> String {
    let body = r##"
    <h1 class="page-title">Database</h1>
    <p class="page-lede">
      Auto-discovered SQLite tables. New tables added by future migrations appear here automatically.
    </p>
    <div class="grid-stats" id="stats"></div>
    <div id="tables-grid" class="tbl-grid"></div>
    "##;

    let script = r##"
    function statCard(label,value,sub,cls){return `<div class="stat ${cls||''}"><div class="label">${esc(label)}</div><div class="value">${esc(value)}</div>${sub?`<div class="sub">${esc(sub)}</div>`:''}</div>`;}
    function renderStats(d){
      document.getElementById('stats').innerHTML = [
        statCard('Tables', d.table_count, 'in omnilauncher.sqlite', 'cyan'),
        statCard('Total rows', d.total_rows, 'across all tables', 'accent'),
        statCard('Added today', d.added_today, 'where time column exists', 'green'),
      ].join('');
    }
    function renderTables(d){
      const tables = d.tables || [];
      const el = document.getElementById('tables-grid');
      if (!tables.length) { el.innerHTML = '<div class="empty">No tables found.</div>'; return; }
      el.innerHTML = tables.map(t => {
        const samples = (t.samples||[]).map(row => {
          const cells = Object.entries(row).map(([k,v]) =>
            `<div class="kv"><span class="k">${esc(k)}</span><span class="v">${esc(v===null?'—':v)}</span></div>`
          ).join('');
          return `<div class="sample">${cells}</div>`;
        }).join('') || '<div class="empty">empty</div>';
        const cols = (t.columns||[]).join(', ');
        return `<div class="tbl-card">
          <div class="head">
            <div class="name">${esc(t.name)}</div>
            <div class="rows">${t.rows} rows${t.added_today?` · +${t.added_today} today`:''}</div>
          </div>
          <div class="cols" title="${esc(cols)}">${esc(cols)}</div>
          <div class="samples">${samples}</div>
        </div>`;
      }).join('');
    }
    async function refresh(){
      try {
        const res = await fetch('/dashboard/tables/data',{cache:'no-store'});
        if (!res.ok) return;
        const d = await res.json();
        if (d.error) { document.querySelector('main').innerHTML = `<pre style="color:#f06c6c">${esc(d.error)}</pre>`; return; }
        renderStats(d); renderTables(d);
        stampGenerated(d);
      } catch(_){}
    }
    refresh(); setInterval(refresh, 5000);
    "##;

    render_page("Database", "tables", body, script)
}
