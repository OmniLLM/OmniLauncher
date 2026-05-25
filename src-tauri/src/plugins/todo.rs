use crate::db;
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use rusqlite::{params, Connection};

pub struct TodoPlugin;

// ─── DB path ─────────────────────────────────────────────────────────────────

fn config_dir() -> std::path::PathBuf {
    // Allow tests to override the config directory via an env var so that
    // parallel test runs don't share the same SQLite database.
    if let Ok(base) = std::env::var("OMNILAUNCHER_CONFIG_DIR") {
        return std::path::PathBuf::from(base);
    }
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config")
        .join("omnilauncher")
}

fn db_path() -> std::path::PathBuf {
    config_dir().join("todo.sqlite")
}

// ─── DB helpers ───────────────────────────────────────────────────────────────

fn open_db() -> rusqlite::Result<Connection> {
    let dir = config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let conn = Connection::open(db_path())?;
    db::run_migrations(&conn)?;
    migrate_json(&conn);
    Ok(conn)
}

fn migrate_json(conn: &Connection) {
    // Skip legacy migration when running under a test-specific config dir override —
    // the override dir is always empty so there is nothing to migrate, and reading
    // from `dirs::home_dir()` would contaminate the test DB with real user todos.
    if std::env::var("OMNILAUNCHER_CONFIG_DIR").is_ok() {
        return;
    }

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM todos", [], |r| r.get(0))
        .unwrap_or(0);
    if count > 0 {
        return;
    }
    let legacy = dirs::home_dir()
        .unwrap_or_default()
        .join(".omnilauncher")
        .join("todos.json");
    if let Ok(content) = std::fs::read_to_string(&legacy) {
        if let Ok(items) = serde_json::from_str::<Vec<String>>(&content) {
            for item in items {
                let _ = conn.execute("INSERT INTO todos (text) VALUES (?1)", params![item]);
            }
        }
    }
}

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct Comment {
    body: String,
    created_at: String,
}

/// priority: 1=urgent 2=high 3=normal 4=low
#[derive(Debug)]
struct TodoItem {
    id: i64,
    text: String,
    status: String,
    done: bool,
    priority: i64,
    due_date: String,
    tags: String,
    created_at: String,
    completed_at: String,
    description: String,
    comments: Vec<Comment>,
}

fn status_label(status: &str) -> &'static str {
    match status {
        "todo" => "⬜ Todo",
        "in_progress" => "🟦 In Progress",
        "blocked" => "⛔ Blocked",
        "done" => "✅ Done",
        _ => "⬜ Todo",
    }
}

fn priority_label(p: i64) -> &'static str {
    match p {
        1 => "🔴 Urgent",
        2 => "🟠 High",
        3 => "🟡 Normal",
        4 => "🔵 Low",
        _ => "🟡 Normal",
    }
}

fn load_todos() -> Vec<TodoItem> {
    let conn = match open_db() {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let mut stmt = match conn.prepare(
        "SELECT id, text, COALESCE(status, CASE WHEN done = 1 THEN 'done' ELSE 'todo' END), done, priority, due_date, tags, created_at, completed_at, description
         FROM todos ORDER BY priority ASC, id ASC",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let mut items: Vec<TodoItem> = stmt
        .query_map([], |row| {
            Ok(TodoItem {
                id: row.get(0)?,
                text: row.get(1)?,
                status: row.get(2)?,
                done: row.get::<_, i64>(3)? != 0,
                priority: row.get::<_, Option<i64>>(4)?.unwrap_or(3),
                due_date: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                tags: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
                created_at: row.get(7)?,
                completed_at: row.get::<_, Option<String>>(8)?.unwrap_or_default(),
                description: row.get::<_, Option<String>>(9)?.unwrap_or_default(),
                comments: vec![],
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    for item in &mut items {
        if let Ok(mut cstmt) = conn.prepare(
            "SELECT body, created_at FROM todo_comments WHERE todo_id = ?1 ORDER BY id ASC",
        ) {
            item.comments = cstmt
                .query_map(params![item.id], |row| {
                    Ok(Comment {
                        body: row.get(0)?,
                        created_at: row.get(1)?,
                    })
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default();
        }
    }
    items
}

fn add_todo(text: &str) -> String {
    match open_db() {
        Ok(conn) => match conn.execute("INSERT INTO todos (text) VALUES (?1)", params![text]) {
            Ok(_) => format!("Added: {}", text),
            Err(e) => format!("DB error: {}", e),
        },
        Err(e) => format!("DB error: {}", e),
    }
}

fn remove_todo(id: i64) -> String {
    match open_db() {
        Ok(conn) => {
            let text: Option<String> = conn
                .query_row("SELECT text FROM todos WHERE id = ?1", params![id], |r| {
                    r.get(0)
                })
                .ok();
            match conn.execute("DELETE FROM todos WHERE id = ?1", params![id]) {
                Ok(0) => format!("No todo with id {}", id),
                Ok(_) => format!("Removed: {}", text.unwrap_or_else(|| id.to_string())),
                Err(e) => format!("DB error: {}", e),
            }
        }
        Err(e) => format!("DB error: {}", e),
    }
}

fn set_status(id: i64, status: &str) -> String {
    let allowed = ["todo", "in_progress", "blocked", "done"];
    if !allowed.contains(&status) {
        return format!("Invalid status: {}", status);
    }

    match open_db() {
        Ok(conn) => {
            let done = status == "done";
            let val: i64 = if done { 1 } else { 0 };
            let completed = if done {
                chrono::Local::now().format("%Y-%m-%d").to_string()
            } else {
                String::new()
            };
            match conn.execute(
                "UPDATE todos SET status = ?1, done = ?2, completed_at = ?3 WHERE id = ?4",
                params![status, val, completed, id],
            ) {
                Ok(0) => format!("No todo with id {}", id),
                Ok(_) => {
                    let text: String = conn
                        .query_row("SELECT text FROM todos WHERE id = ?1", params![id], |r| {
                            r.get(0)
                        })
                        .unwrap_or_else(|_| id.to_string());
                    format!("{}: {}", status_label(status), text)
                }
                Err(e) => format!("DB error: {}", e),
            }
        }
        Err(e) => format!("DB error: {}", e),
    }
}

fn clear_todos() -> String {
    match open_db() {
        Ok(conn) => match conn.execute("DELETE FROM todos", []) {
            Ok(n) => {
                // Reset the autoincrement counter so the next inserted row starts at id=1.
                let _ = conn.execute("DELETE FROM sqlite_sequence WHERE name = 'todos'", []);
                format!("Cleared {} todos.", n)
            }
            Err(e) => format!("DB error: {}", e),
        },
        Err(e) => format!("DB error: {}", e),
    }
}

fn set_field(id: i64, field: &str, value: &str) -> String {
    let allowed = ["priority", "due_date", "tags", "description"];
    if !allowed.contains(&field) {
        return format!("Unknown field: {}", field);
    }
    match open_db() {
        Ok(conn) => {
            let sql = format!("UPDATE todos SET {} = ?1 WHERE id = ?2", field);
            match conn.execute(&sql, params![value, id]) {
                Ok(0) => format!("No todo with id {}", id),
                Ok(_) => format!("Updated {} for todo #{}", field, id),
                Err(e) => format!("DB error: {}", e),
            }
        }
        Err(e) => format!("DB error: {}", e),
    }
}

// ─── HTML generation ──────────────────────────────────────────────────────────

fn generate_html(items: &[TodoItem]) -> String {
    let total = items.len();
    let done_count = items.iter().filter(|t| t.status == "done").count();
    let pct = if total > 0 {
        done_count * 100 / total
    } else {
        0
    };
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let json_rows: String = items
        .iter()
        .map(|t| {
            let comments_json: String = t
                .comments
                .iter()
                .map(|c| format!(
                    r#"{{"body":{},"at":"{}"}}"#,
                    serde_json::to_string(&c.body).unwrap_or_default(),
                    &c.created_at[..10],
                ))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                r#"{{"id":{},"text":{},"desc":{},"done":{},"status":{},"priority":{},"due":{},"tags":{},"date":"{}","completed":{},"comments":[{}]}}"#,
                t.id,
                serde_json::to_string(&t.text).unwrap_or_default(),
                serde_json::to_string(&t.description).unwrap_or_default(),
                if t.done { "true" } else { "false" },
                serde_json::to_string(&t.status).unwrap_or_default(),
                t.priority,
                serde_json::to_string(&t.due_date).unwrap_or_default(),
                serde_json::to_string(&t.tags).unwrap_or_default(),
                &t.created_at[..10],
                serde_json::to_string(&t.completed_at).unwrap_or_default(),
                comments_json,
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>OmniLauncher Todos</title>
<style>
*{{box-sizing:border-box;margin:0;padding:0}}
body{{background:#1e1e2e;color:#cdd6f4;font-family:'Inter','Segoe UI',system-ui,sans-serif;min-height:100vh;padding:36px 20px}}
.container{{max-width:1200px;margin:0 auto}}
header{{display:flex;align-items:center;gap:14px;margin-bottom:28px}}
.logo{{font-size:26px}}
h1{{font-size:21px;font-weight:700;color:#cba6f7}}
.sub{{font-size:12px;color:#6c7086;margin-top:3px}}
.stats{{background:#313244;border-radius:12px;padding:16px 22px;margin-bottom:24px;display:flex;align-items:center;gap:28px;flex-wrap:wrap}}
.stat{{text-align:center}}
.stat .num{{font-size:24px;font-weight:700;color:#cba6f7}}
.stat .lbl{{font-size:11px;color:#6c7086;margin-top:2px}}
.progress-wrap{{flex:1;min-width:120px}}
.progress-lbl{{font-size:11px;color:#6c7086;margin-bottom:5px}}
.progress-bar{{height:7px;background:#45475a;border-radius:4px;overflow:hidden}}
.progress-fill{{height:100%;background:#a6e3a1;border-radius:4px}}
.toolbar{{display:flex;align-items:center;gap:10px;margin-bottom:16px;flex-wrap:wrap}}
.toolbar label{{font-size:12px;color:#6c7086}}
.toolbar select,.toolbar input{{background:#313244;border:1px solid #45475a;color:#cdd6f4;border-radius:7px;padding:5px 10px;font-size:13px;outline:none;cursor:pointer}}
.toolbar select:focus,.toolbar input:focus{{border-color:#cba6f7}}
.toolbar input{{flex:1;min-width:140px}}
.card{{background:#313244;border-radius:12px;overflow:hidden}}
table{{width:100%;border-collapse:collapse}}
th{{position:relative;padding:10px 12px;font-size:11px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:#6c7086;text-align:left;cursor:pointer;user-select:none;white-space:nowrap;background:#2a2a3d;border-bottom:1px solid #45475a}}
th:hover{{color:#cba6f7}}
th .arr{{margin-left:4px;opacity:.4;font-size:10px}}
th.active .arr{{opacity:1;color:#cba6f7}}
th .col-filter-badge{{display:inline-block;margin-left:5px;background:#cba6f7;color:#1e1e2e;border-radius:10px;font-size:9px;font-weight:700;padding:1px 6px;vertical-align:middle;text-transform:none;letter-spacing:0}}
.col-dropdown{{position:absolute;top:100%;left:0;z-index:200;min-width:160px;background:#2a2a3d;border:1px solid #45475a;border-radius:8px;box-shadow:0 8px 24px #00000080;overflow:hidden;display:none}}
.col-dropdown.open{{display:block}}
.col-dropdown-item{{padding:8px 14px;font-size:12px;color:#cdd6f4;cursor:pointer;white-space:nowrap;text-transform:none;letter-spacing:0;font-weight:400}}
.col-dropdown-item:hover{{background:#313244;color:#cba6f7}}
.col-dropdown-item.selected{{color:#a6e3a1;font-weight:600}}
.col-dropdown-item.clear-item{{color:#f38ba8;border-top:1px solid #45475a;margin-top:2px}}
tr.data-row{{cursor:pointer;transition:background 120ms}}
tr.data-row:hover td{{background:#2e2e40}}
tr.data-row.expanded td{{background:#2a2a3c;border-bottom:none}}
tr.data-row.overdue td.col-due{{color:#f38ba8;font-weight:600}}
td{{padding:11px 12px;font-size:14px;vertical-align:middle;border-bottom:1px solid #2a2a3d}}
td.col-expand{{width:24px;color:#6c7086;font-size:12px;padding-right:0}}
td.col-id{{color:#6c7086;width:44px;font-size:12px;font-variant-numeric:tabular-nums}}
td.col-text{{line-height:1.5;min-width:180px}}
td.col-text .desc-preview{{font-size:11px;color:#6c7086;margin-top:2px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;max-width:320px}}
td.col-text.done-text{{color:#6c7086;text-decoration:line-through}}
td.col-pri{{width:100px;white-space:nowrap}}
td.col-status{{width:100px;white-space:nowrap}}
td.col-due{{width:98px;font-size:12px;color:#6c7086;white-space:nowrap}}
td.col-tags{{width:130px;font-size:12px}}
td.col-date{{color:#6c7086;font-size:12px;width:94px;white-space:nowrap}}
td.col-completed{{color:#a6e3a1;font-size:12px;width:94px;white-space:nowrap}}
td.col-comments{{width:64px;font-size:12px;color:#6c7086;text-align:center}}
.badge{{display:inline-flex;align-items:center;gap:4px;font-size:11px;font-weight:600;padding:2px 8px;border-radius:20px;white-space:nowrap}}
.badge-todo{{background:#f38ba820;color:#f38ba8;border:1px solid #f38ba840}}
.badge-in-progress{{background:#89b4fa20;color:#89b4fa;border:1px solid #89b4fa40}}
.badge-blocked{{background:#fab38720;color:#fab387;border:1px solid #fab38740}}
.badge-done{{background:#a6e3a120;color:#a6e3a1;border:1px solid #a6e3a140}}
.pri-1{{color:#f38ba8}}.pri-2{{color:#fab387}}.pri-3{{color:#f9e2af}}.pri-4{{color:#89b4fa}}
.tag{{display:inline-block;font-size:10px;background:#45475a;color:#cdd6f4;border-radius:4px;padding:1px 6px;margin:1px 2px 1px 0}}
tr.detail-row td{{padding:0;border-bottom:1px solid #45475a}}
.detail-panel{{padding:16px 18px 20px 48px;background:#25253a;display:flex;flex-direction:column;gap:14px}}
.detail-grid{{display:grid;grid-template-columns:repeat(auto-fill,minmax(180px,1fr));gap:10px;margin-bottom:4px}}
.detail-field{{background:#313244;border-radius:8px;padding:10px 14px}}
.detail-field-lbl{{font-size:10px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:#6c7086;margin-bottom:4px}}
.detail-field-val{{font-size:13px;color:#cdd6f4}}
.detail-section-lbl{{font-size:11px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:#6c7086;margin-bottom:6px}}
.detail-desc{{font-size:14px;line-height:1.7;color:#cdd6f4;white-space:pre-wrap}}
.detail-desc.empty,.no-comments{{font-size:13px;color:#45475a;font-style:italic}}
.comments-list{{display:flex;flex-direction:column;gap:8px}}
.comment{{background:#313244;border-radius:8px;padding:10px 14px;border-left:3px solid #cba6f755}}
.comment-body{{font-size:14px;line-height:1.6;white-space:pre-wrap}}
.comment-meta{{font-size:11px;color:#6c7086;margin-top:4px}}
tr.group-header td{{background:#252535;color:#6c7086;font-size:11px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;padding:7px 12px;border-bottom:1px solid #45475a}}
.empty-state{{text-align:center;color:#6c7086;padding:32px;font-size:14px}}
.footer{{text-align:center;font-size:11px;color:#45475a;margin-top:32px}}
</style>
</head>
<body>
<div class="container">
  <header>
    <div class="logo">✦</div>
    <div>
      <h1>OmniLauncher Todos</h1>
      <div class="sub">~/.config/omnilauncher/todo.sqlite</div>
    </div>
  </header>

  <div class="stats">
    <div class="stat"><div class="num" id="s-total">{total}</div><div class="lbl">Total</div></div>
    <div class="stat"><div class="num" id="s-pending">{pending_count}</div><div class="lbl">Pending</div></div>
    <div class="stat"><div class="num" id="s-done">{done_count}</div><div class="lbl">Done</div></div>
    <div class="stat"><div class="num" id="s-overdue" style="color:#f38ba8">0</div><div class="lbl">Overdue</div></div>
    <div class="progress-wrap">
      <div class="progress-lbl" id="pct-lbl">{pct}% complete</div>
      <div class="progress-bar"><div class="progress-fill" id="pct-fill" style="width:{pct}%"></div></div>
    </div>
  </div>

  <div class="toolbar">
    <input id="search" type="text" placeholder="🔍  Filter todos…">
    <label>Group by</label>
    <select id="group-by">
      <option value="none">None</option>
      <option value="status">Status</option>
      <option value="priority">Priority</option>
      <option value="tags">Tags</option>
      <option value="date">Created date</option>
      <option value="due">Due date</option>
    </select>
  </div>

  <div class="card">
    <table id="todo-table">
      <thead>
        <tr>
          <th style="width:24px;padding-right:0"></th>
          <th data-col="id" class="active">ID <span class="arr">▲</span></th>
          <th data-col="text">Task <span class="arr">↕</span></th>
          <th data-col="priority" data-filterable="priority">Priority <span class="arr">↕</span><div class="col-dropdown" id="dd-priority"></div></th>
          <th data-col="status" data-filterable="status">Status <span class="arr">↕</span><div class="col-dropdown" id="dd-status"></div></th>
          <th data-col="due" data-filterable="due">Due date <span class="arr">↕</span><div class="col-dropdown" id="dd-due"></div></th>
          <th data-col="tags" data-filterable="tags">Tags <span class="arr">↕</span><div class="col-dropdown" id="dd-tags"></div></th>
          <th data-col="date" data-filterable="date">Created <span class="arr">↕</span><div class="col-dropdown" id="dd-date"></div></th>
          <th data-col="completed" data-filterable="completed">Completed <span class="arr">↕</span><div class="col-dropdown" id="dd-completed"></div></th>
          <th data-col="comments">💬 <span class="arr">↕</span></th>
        </tr>
      </thead>
      <tbody id="tbody"></tbody>
    </table>
    <div class="empty-state" id="empty" style="display:none">No todos match your filter.</div>
  </div>

  <div class="footer">Generated by OmniLauncher · {timestamp}</div>
</div>

<script>
let RAW = [{json_rows}];
const TODAY = '{today}';
const DATA_URL = '/todo/data';
const PRI_LABEL = {{1:'🔴 Urgent',2:'🟠 High',3:'🟡 Normal',4:'🔵 Low'}};
const PRI_CLASS = {{1:'pri-1',2:'pri-2',3:'pri-3',4:'pri-4'}};
const STATUS_LABEL = {{todo:'⬜ Todo',in_progress:'🟦 In Progress',blocked:'⛔ Blocked',done:'✅ Done'}};
const STATUS_CLASS = {{todo:'badge-todo',in_progress:'badge-in-progress',blocked:'badge-blocked',done:'badge-done'}};
const STATUS_ORDER = {{todo:1,in_progress:2,blocked:3,done:4}};
function statusLabel(status) {{ return STATUS_LABEL[status] || STATUS_LABEL.todo; }}
function statusClass(status) {{ return STATUS_CLASS[status] || STATUS_CLASS.todo; }}
function statusSortKey(status) {{ return STATUS_ORDER[status] || STATUS_ORDER.todo; }}
function statusFromLabel(label) {{ return Object.keys(STATUS_LABEL).find(key => STATUS_LABEL[key] === label) || 'todo'; }}
let sortCol='id', sortDir=1, groupBy='none', filterText='', expanded=new Set();
// columnFilters: map of filterable col key → selected value string (or null)
const columnFilters = {{}};

// ─── helpers ──────────────────────────────────────────────────────────────────

function isOverdue(t) {{
  return t.status !== 'done' && t.due && t.due < TODAY;
}}

function colValues(col) {{
  const set = new Set();
  RAW.forEach(t => {{
    if (col === 'status') {{ set.add(statusLabel(t.status)); }}
    else if (col === 'priority') {{ set.add(PRI_LABEL[t.priority] || 'Normal'); }}
    else if (col === 'tags') {{
      const tags = t.tags ? t.tags.split(',').map(s=>s.trim()).filter(Boolean) : [];
      if (tags.length === 0) set.add('(no tags)');
      else tags.forEach(tag => set.add(tag));
    }}
    else if (col === 'due') {{ set.add(t.due || '—'); }}
    else if (col === 'date') {{ set.add(t.date); }}
    else if (col === 'completed') {{ set.add(t.completed || '—'); }}
  }});
  return [...set].sort();
}}

function colMatchesFilter(t, col, selected) {{
  if (col === 'status') return statusLabel(t.status) === selected;
  if (col === 'priority') return (PRI_LABEL[t.priority] || 'Normal') === selected;
  if (col === 'tags') {{
    const tags = t.tags ? t.tags.split(',').map(s=>s.trim()).filter(Boolean) : [];
    return selected === '(no tags)' ? tags.length === 0 : tags.includes(selected);
  }}
  if (col === 'due') return (t.due || '—') === selected;
  if (col === 'date') return t.date === selected;
  if (col === 'completed') return (t.completed || '—') === selected;
  return true;
}}

// ─── dropdown ─────────────────────────────────────────────────────────────────

let openDropdownCol = null;

function openDropdown(col, th) {{
  closeDropdown();
  openDropdownCol = col;
  const dd = document.getElementById('dd-' + col);
  if (!dd) return;
  dd.innerHTML = '';

  const values = colValues(col);
  values.forEach(val => {{
    const item = document.createElement('div');
    item.className = 'col-dropdown-item' + (columnFilters[col] === val ? ' selected' : '');
    item.textContent = val;
    item.addEventListener('mousedown', e => {{
      e.stopPropagation();
      columnFilters[col] = (columnFilters[col] === val) ? null : val;
      closeDropdown();
      updateFilterBadges();
      render();
    }});
    dd.appendChild(item);
  }});

  if (columnFilters[col]) {{
    const clear = document.createElement('div');
    clear.className = 'col-dropdown-item clear-item';
    clear.textContent = '✕ Clear filter';
    clear.addEventListener('mousedown', e => {{
      e.stopPropagation();
      columnFilters[col] = null;
      closeDropdown();
      updateFilterBadges();
      render();
    }});
    dd.appendChild(clear);
  }}

  dd.classList.add('open');
}}

function closeDropdown() {{
  if (openDropdownCol) {{
    const dd = document.getElementById('dd-' + openDropdownCol);
    if (dd) dd.classList.remove('open');
    openDropdownCol = null;
  }}
}}

function updateFilterBadges() {{
  document.querySelectorAll('th[data-filterable]').forEach(th => {{
    const col = th.dataset.filterable;
    const existing = th.querySelector('.col-filter-badge');
    if (columnFilters[col]) {{
      if (!existing) {{
        const badge = document.createElement('span');
        badge.className = 'col-filter-badge';
        badge.textContent = columnFilters[col];
        th.appendChild(badge);
      }} else {{
        existing.textContent = columnFilters[col];
      }}
    }} else if (existing) {{
      existing.remove();
    }}
  }});
}}

// ─── render ───────────────────────────────────────────────────────────────────

function render() {{
  let data = RAW.filter(t => {{
    if (filterText && !t.text.toLowerCase().includes(filterText.toLowerCase()) &&
        !t.tags.toLowerCase().includes(filterText.toLowerCase())) return false;
    for (const [col, val] of Object.entries(columnFilters)) {{
      if (val && !colMatchesFilter(t, col, val)) return false;
    }}
    return true;
  }});

  data.sort((a,b) => {{
    let av, bv;
    if (sortCol==='status') {{ av=statusSortKey(a.status); bv=statusSortKey(b.status); }}
    else if (sortCol==='priority') {{ av=a.priority; bv=b.priority; }}
    else if (sortCol==='due') {{ av=a.due||'9999'; bv=b.due||'9999'; }}
    else if (sortCol==='date') {{ av=a.date; bv=b.date; }}
    else {{ av=a[sortCol]; bv=b[sortCol]; }}
    if (typeof av==='string') av=av.toLowerCase();
    if (typeof bv==='string') bv=bv.toLowerCase();
    return av<bv ? -sortDir : av>bv ? sortDir : 0;
  }});

  const vis=data.length, visDone=data.filter(t=>t.status==='done').length;
  const visOverdue=data.filter(isOverdue).length;
  document.getElementById('s-total').textContent=vis;
  document.getElementById('s-pending').textContent=vis-visDone;
  document.getElementById('s-done').textContent=visDone;
  document.getElementById('s-overdue').textContent=visOverdue;
  const pct=vis>0?Math.round(visDone*100/vis):0;
  document.getElementById('pct-lbl').textContent=pct+'% complete';
  document.getElementById('pct-fill').style.width=pct+'%';

  const tbody=document.getElementById('tbody');
  tbody.innerHTML='';
  document.getElementById('empty').style.display=data.length?'none':'block';

  const insert=(t)=>{{ tbody.appendChild(makeDataRow(t)); if(expanded.has(t.id)) tbody.appendChild(makeDetailRow(t)); }};

  if (groupBy==='none') {{
    data.forEach(insert);
  }} else {{
    const groups=new Map();
    data.forEach(t=>{{
      let key;
      if (groupBy==='status') key=statusLabel(t.status);
      else if (groupBy==='priority') key=PRI_LABEL[t.priority]||'Normal';
      else if (groupBy==='tags') {{ (t.tags?t.tags.split(',').map(s=>s.trim()).filter(Boolean):['(no tags)']).forEach(tag=>{{ if(!groups.has(tag)) groups.set(tag,[]); groups.get(tag).push(t); }}); return; }}
      else if (groupBy==='due') key=t.due||(t.status==='done'?'✅ Done':'No due date');
      else key=t.date;
      if(!groups.has(key)) groups.set(key,[]);
      groups.get(key).push(t);
    }});
    const keys=[...groups.keys()].sort();
    if(groupBy==='status') keys.sort((a,b)=>statusSortKey(statusFromLabel(a))-statusSortKey(statusFromLabel(b)));
    if(groupBy==='priority') keys.sort((a,b)=>Object.values(PRI_LABEL).indexOf(a)-Object.values(PRI_LABEL).indexOf(b));
    keys.forEach(key=>{{
      const hdr=document.createElement('tr'); hdr.className='group-header';
      hdr.innerHTML=`<td colspan="10">${{key}} <span style="font-weight:400;opacity:.6">(${{groups.get(key).length}})</span></td>`;
      tbody.appendChild(hdr);
      groups.get(key).forEach(insert);
    }});
  }}
}}

function makeDataRow(t) {{
  const tr=document.createElement('tr');
  const over=isOverdue(t);
  tr.className='data-row'+(expanded.has(t.id)?' expanded':'')+(over?' overdue':'');
  tr.dataset.id=t.id;
  const tagHtml=t.tags?t.tags.split(',').map(s=>s.trim()).filter(Boolean).map(s=>`<span class="tag">${{esc(s)}}</span>`).join(''):'';
  const dueDisplay=t.due?(over?`⚠ ${{t.due}}`:t.due):'—';
  const descPreview=t.desc?`<div class="desc-preview">${{esc(t.desc)}}</div>`:'';
  const completedDisplay=t.completed||'—';
  const commentCount=t.comments.length>0?`<span style="color:#cba6f7;font-weight:600">${{t.comments.length}}</span>`:'—';
  tr.innerHTML=`
    <td class="col-expand">${{expanded.has(t.id)?'▾':'▸'}}</td>
    <td class="col-id">#${{t.id}}</td>
    <td class="col-text ${{t.status==='done'?'done-text':''}}">${{esc(t.text)}}${{descPreview}}</td>
    <td class="col-pri"><span class="${{PRI_CLASS[t.priority]||'pri-3'}}">${{PRI_LABEL[t.priority]||'Normal'}}</span></td>
    <td class="col-status"><span class="badge ${{statusClass(t.status)}}">${{statusLabel(t.status)}}</span></td>
    <td class="col-due">${{dueDisplay}}</td>
    <td class="col-tags">${{tagHtml||'<span style="color:#45475a">—</span>'}}</td>
    <td class="col-date">${{t.date}}</td>
    <td class="col-completed">${{completedDisplay}}</td>
    <td class="col-comments">${{commentCount}}</td>
  `;
  tr.addEventListener('click',()=>{{ if(expanded.has(t.id)) expanded.delete(t.id); else expanded.add(t.id); render(); }});
  return tr;
}}

function makeDetailRow(t) {{
  const tr=document.createElement('tr'); tr.className='detail-row';
  const commentsHtml=t.comments.length===0
    ?'<div class="no-comments">No comments yet.</div>'
    :t.comments.map(c=>`<div class="comment"><div class="comment-body">${{esc(c.body)}}</div><div class="comment-meta">${{c.at}}</div></div>`).join('');
  tr.innerHTML=`<td colspan="10"><div class="detail-panel">
    <div class="detail-grid">
      <div class="detail-field"><div class="detail-field-lbl">Priority</div><div class="detail-field-val ${{PRI_CLASS[t.priority]||'pri-3'}}">${{PRI_LABEL[t.priority]||'Normal'}}</div></div>
      <div class="detail-field"><div class="detail-field-lbl">Due date</div><div class="detail-field-val">${{t.due||'—'}}</div></div>
      <div class="detail-field"><div class="detail-field-lbl">Tags</div><div class="detail-field-val">${{t.tags||'—'}}</div></div>
      <div class="detail-field"><div class="detail-field-lbl">Created</div><div class="detail-field-val">${{t.date}}</div></div>
      <div class="detail-field"><div class="detail-field-lbl">Completed</div><div class="detail-field-val">${{t.completed||'—'}}</div></div>
    </div>
    <div><div class="detail-section-lbl">Description</div>
      <div class="detail-desc ${{t.desc?'':'empty'}}">${{t.desc?esc(t.desc):'No description.'}}</div></div>
    <div><div class="detail-section-lbl">Comments (${{t.comments.length}})</div>
      <div class="comments-list">${{commentsHtml}}</div></div>
  </div></td>`;
  return tr;
}}

function esc(s) {{ return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;'); }}

// ─── header click: sort + filterable columns open dropdown ────────────────────

document.querySelectorAll('th[data-col]').forEach(th=>{{
  th.addEventListener('click', e=>{{
    // If click was inside dropdown, skip sort toggle
    if (e.target.closest('.col-dropdown')) return;

    const col = th.dataset.col;
    const filterable = th.dataset.filterable;

    if (filterable) {{
      // If same filterable header clicked again, toggle dropdown; otherwise open
      if (openDropdownCol === filterable) {{
        closeDropdown();
      }} else {{
        openDropdown(filterable, th);
      }}
      // Still toggle sort on second click (after dropdown open):
      // sort toggle is secondary; open dropdown is primary action.
    }} else {{
      if (sortCol===col) sortDir*=-1; else {{sortCol=col;sortDir=1;}}
      document.querySelectorAll('th').forEach(h=>{{h.classList.remove('active');h.querySelector('.arr')&&(h.querySelector('.arr').textContent='↕');}});
      th.classList.add('active'); th.querySelector('.arr').textContent=sortDir===1?'▲':'▼';
      render();
    }}
  }});
}});

// Close dropdown when clicking outside
document.addEventListener('click', e=>{{
  if (!e.target.closest('th[data-filterable]')) closeDropdown();
}});

document.getElementById('group-by').addEventListener('change',e=>{{groupBy=e.target.value;render();}});
document.getElementById('search').addEventListener('input',e=>{{filterText=e.target.value;render();}});
async function refreshData() {{
  try {{
    const res = await fetch(DATA_URL, {{ cache: 'no-store' }});
    if (!res.ok) return;
    const next = await res.json();
    RAW = next;
    render();
  }} catch (_) {{}}
}}

setInterval(refreshData, 1500);
render();
</script>
</body>
</html>"#,
        total = total,
        pending_count = total - done_count,
        done_count = done_count,
        pct = pct,
        today = today,
        timestamp = timestamp,
        json_rows = json_rows,
    )
}

fn todo_live_url() -> &'static str {
    "http://127.0.0.1:1421/todo"
}

pub fn todo_live_data_json() -> String {
    let rows = load_todos()
        .into_iter()
        .map(|t| {
            let comments = t
                .comments
                .into_iter()
                .map(|c| {
                    serde_json::json!({
                        "body": c.body,
                        "at": c.created_at.chars().take(10).collect::<String>(),
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "id": t.id,
                "text": t.text,
                "desc": t.description,
                "done": t.done,
                "status": t.status,
                "priority": t.priority,
                "due": t.due_date,
                "tags": t.tags,
                "date": t.created_at.chars().take(10).collect::<String>(),
                "completed": t.completed_at,
                "comments": comments,
            })
        })
        .collect::<Vec<_>>();

    serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string())
}

pub fn todo_live_html() -> String {
    generate_html(&load_todos())
}

// ─── Plugin impl ──────────────────────────────────────────────────────────────

#[async_trait]
impl Plugin for TodoPlugin {
    fn name(&self) -> &str {
        "todo_memory"
    }
    fn description(&self) -> &str {
        "Manage a persistent todo list (SQLite) with browser view"
    }
    fn keyword(&self) -> Option<&str> {
        None
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let term = if let Some(term) = q.raw.strip_prefix("todo ") {
            term.trim()
        } else if let Some(term) = q.raw.strip_prefix("/todo ") {
            term.trim()
        } else if let Some(term) = q.raw.strip_prefix("/t ") {
            term.trim()
        } else if q.raw == "todo" || q.raw == "/todo" || q.raw == "/t" {
            ""
        } else {
            return vec![];
        };

        if term.is_empty() || term == "list" {
            let items = load_todos();
            if items.is_empty() {
                return vec![QueryResult {
                    id: "todo:empty".to_string(),
                    title: "No todos".to_string(),
                    subtitle: Some("Use 'todo <text>' to add one".to_string()),
                    icon: Some("📝".to_string()),
                    score: 50,
                    action_type: "copy".to_string(),
                    action_data: String::new(),
                }];
            }
            return items
                .iter()
                .map(|item| {
                    let due_str = if !item.due_date.is_empty() {
                        format!(" · due {}", item.due_date)
                    } else {
                        String::new()
                    };
                    QueryResult {
                        id: format!("todo:{}", item.id),
                        title: format!("{} {}", status_label(&item.status), item.text),
                        subtitle: Some(format!(
                            "#{} · {} · {} · p{}{}",
                            item.id,
                            &item.created_at[..10],
                            status_label(&item.status),
                            item.priority,
                            due_str
                        )),
                        icon: None,
                        score: 60,
                        action_type: "copy".to_string(),
                        action_data: item.text.clone(),
                    }
                })
                .collect();
        }

        if term == "view" || term == "show" {
            return vec![QueryResult {
                id: "todo:view".to_string(),
                title: "Open todos in browser".to_string(),
                subtitle: Some("Opens the live todo page in your default browser".to_string()),
                icon: Some("🌐".to_string()),
                score: 80,
                action_type: "open_url".to_string(),
                action_data: "/todo".to_string(),
            }];
        }

        macro_rules! parse_id_action {
            ($prefix:expr, $action_type:expr, $icon:expr, $label:expr) => {
                if let Some(rest) = term.strip_prefix($prefix) {
                    if let Ok(id) = rest.trim().parse::<i64>() {
                        return vec![QueryResult {
                            id: format!("todo:{}:{}", $action_type, id),
                            title: format!("{} #{}", $label, id),
                            subtitle: Some("Press Enter to confirm".to_string()),
                            icon: Some($icon.to_string()),
                            score: 80,
                            action_type: $action_type.to_string(),
                            action_data: id.to_string(),
                        }];
                    }
                }
            };
        }
        parse_id_action!("done ", "todo_done", "✅", "Mark done");
        parse_id_action!("undone ", "todo_undone", "↩", "Mark undone");
        parse_id_action!("status todo ", "todo_status_todo", "⬜", "Mark todo");
        parse_id_action!(
            "status in_progress ",
            "todo_status_in_progress",
            "🟦",
            "Mark in progress"
        );
        parse_id_action!(
            "status blocked ",
            "todo_status_blocked",
            "⛔",
            "Mark blocked"
        );
        parse_id_action!("status done ", "todo_status_done", "✅", "Mark done");
        parse_id_action!("remove ", "todo_remove", "🗑", "Remove");
        parse_id_action!("rm ", "todo_remove", "🗑", "Remove");

        vec![QueryResult {
            id: "todo:add".to_string(),
            title: format!("Add todo: {}", term),
            subtitle: Some("Press Enter to add".to_string()),
            icon: Some("📝".to_string()),
            score: 70,
            action_type: "todo_add".to_string(),
            action_data: term.to_string(),
        }]
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "todo_memory",
                "description": "Manage a persistent todo list stored in SQLite. Fields: text, status (todo/in_progress/blocked/done), description, priority (1=urgent/2=high/3=normal/4=low), due_date (YYYY-MM-DD), tags (comma-separated), completed_at (auto-set on done). Actions: list, add, remove, done, undone, clear, view (live browser view), set_status, set_field (update priority/due_date/tags/description), comment_add, note_save, note_read.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["list","add","remove","done","undone","clear","view","set_status","set_field","comment_add","description_set","note_save","note_read"],
                            "description": "Action to perform"
                        },
                        "text": {
                            "type": "string",
                            "description": "Todo text (add), numeric id (remove/done/undone/set_field/comment_add/description_set), or note key (note_save/note_read)"
                        },
                        "field": {
                            "type": "string",
                            "enum": ["priority","due_date","tags","description"],
                            "description": "Field to update (for set_field action)"
                        },
                        "content": {
                            "type": "string",
                            "description": "New value (set_field), comment body (comment_add), description (description_set), or note content (note_save)"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["todo","in_progress","blocked","done"],
                            "description": "New status for set_status action"
                        }
                    },
                    "required": ["action"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let action = args["action"].as_str().unwrap_or("list");
        let text = args["text"].as_str().unwrap_or("");
        let content = args["content"].as_str().unwrap_or("");
        let field = args["field"].as_str().unwrap_or("");
        let status = args["status"].as_str().unwrap_or("");

        match action {
            "list" => {
                let items = load_todos();
                if items.is_empty() {
                    return "Todo list is empty.".to_string();
                }
                items
                    .iter()
                    .map(|t| {
                        let due = if t.due_date.is_empty() {
                            String::new()
                        } else {
                            format!(" due:{}", t.due_date)
                        };
                        let tags = if t.tags.is_empty() {
                            String::new()
                        } else {
                            format!(" [{}]", t.tags)
                        };
                        format!(
                            "{}. [{}] {} (id:{} status:{} p:{}{}{})",
                            t.id,
                            if t.status == "done" { "x" } else { " " },
                            t.text,
                            t.id,
                            status_label(&t.status),
                            priority_label(t.priority),
                            due,
                            tags
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            "add" => {
                if text.is_empty() {
                    return "Error: no text provided".to_string();
                }
                add_todo(text)
            }
            "remove" => {
                let id: i64 = text.parse().unwrap_or(0);
                if id == 0 {
                    return format!("Invalid id: '{}'", text);
                }
                remove_todo(id)
            }
            "done" => {
                let id: i64 = text.parse().unwrap_or(0);
                if id == 0 {
                    return format!("Invalid id: '{}'", text);
                }
                set_status(id, "done")
            }
            "undone" => {
                let id: i64 = text.parse().unwrap_or(0);
                if id == 0 {
                    return format!("Invalid id: '{}'", text);
                }
                set_status(id, "todo")
            }
            "clear" => clear_todos(),
            "view" => format!("Open {} in your browser.", todo_live_url()),
            "set_status" => {
                let id: i64 = text.parse().unwrap_or(0);
                if id == 0 {
                    return format!("Invalid id: '{}'", text);
                }
                if status.is_empty() {
                    return "Error: status required (todo/in_progress/blocked/done)".to_string();
                }
                set_status(id, status)
            }
            "set_field" => {
                let id: i64 = text.parse().unwrap_or(0);
                if id == 0 {
                    return format!("Invalid id: '{}'", text);
                }
                if field.is_empty() {
                    return "Error: field required (priority/due_date/tags/description)"
                        .to_string();
                }
                set_field(id, field, content)
            }
            "comment_add" => {
                let id: i64 = text.parse().unwrap_or(0);
                if id == 0 {
                    return format!("Invalid id: '{}'", text);
                }
                if content.is_empty() {
                    return "Error: content required".to_string();
                }
                match open_db() {
                    Ok(conn) => match conn.execute(
                        "INSERT INTO todo_comments (todo_id, body) VALUES (?1, ?2)",
                        params![id, content],
                    ) {
                        Ok(_) => format!("Comment added to todo #{}", id),
                        Err(e) => format!("DB error: {}", e),
                    },
                    Err(e) => format!("DB error: {}", e),
                }
            }
            "description_set" => {
                let id: i64 = text.parse().unwrap_or(0);
                if id == 0 {
                    return format!("Invalid id: '{}'", text);
                }
                set_field(id, "description", content)
            }
            "note_save" => {
                if text.is_empty() || content.is_empty() {
                    return "Error: need text (key) and content".to_string();
                }
                let notes_dir = config_dir().join("notes");
                let _ = std::fs::create_dir_all(&notes_dir);
                let path = notes_dir.join(format!("{}.md", text));
                match std::fs::write(&path, content) {
                    Ok(_) => format!("Note '{}' saved.", text),
                    Err(e) => format!("Error saving note: {}", e),
                }
            }
            "note_read" => {
                if text.is_empty() {
                    return "Error: need note key".to_string();
                }
                let path = config_dir().join("notes").join(format!("{}.md", text));
                match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => format!("Note '{}' not found.", text),
                }
            }
            _ => format!("Unknown action: {}", action),
        }
    }
}
