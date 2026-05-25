use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use rusqlite::{Connection, params};

pub struct TodoPlugin;

// ─── DB path ─────────────────────────────────────────────────────────────────

fn config_dir() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config")
        .join("omnilauncher")
}

fn db_path() -> std::path::PathBuf {
    config_dir().join("todo.sqlite")
}

fn html_path() -> std::path::PathBuf {
    config_dir().join("todos.html")
}

// ─── DB helpers ───────────────────────────────────────────────────────────────

fn open_db() -> rusqlite::Result<Connection> {
    let dir = config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let conn = Connection::open(db_path())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS todos (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            text       TEXT    NOT NULL,
            done       INTEGER NOT NULL DEFAULT 0,
            created_at TEXT    NOT NULL DEFAULT (datetime('now'))
        );",
    )?;
    // Migrate old todos.json if it exists and DB is empty
    migrate_json(&conn);
    Ok(conn)
}

fn migrate_json(conn: &Connection) {
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

#[derive(Debug)]
struct TodoItem {
    id: i64,
    text: String,
    done: bool,
    created_at: String,
}

fn load_todos() -> Vec<TodoItem> {
    let conn = match open_db() {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let mut stmt = match conn.prepare(
        "SELECT id, text, done, created_at FROM todos ORDER BY id ASC",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map([], |row| {
        Ok(TodoItem {
            id: row.get(0)?,
            text: row.get(1)?,
            done: row.get::<_, i64>(2)? != 0,
            created_at: row.get(3)?,
        })
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
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

fn set_done(id: i64, done: bool) -> String {
    match open_db() {
        Ok(conn) => {
            let val: i64 = if done { 1 } else { 0 };
            match conn.execute(
                "UPDATE todos SET done = ?1 WHERE id = ?2",
                params![val, id],
            ) {
                Ok(0) => format!("No todo with id {}", id),
                Ok(_) => {
                    let text: String = conn
                        .query_row("SELECT text FROM todos WHERE id = ?1", params![id], |r| {
                            r.get(0)
                        })
                        .unwrap_or_else(|_| id.to_string());
                    if done {
                        format!("✅ Done: {}", text)
                    } else {
                        format!("↩ Undone: {}", text)
                    }
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
            Ok(n) => format!("Cleared {} todos.", n),
            Err(e) => format!("DB error: {}", e),
        },
        Err(e) => format!("DB error: {}", e),
    }
}

// ─── HTML generation ──────────────────────────────────────────────────────────

fn generate_html(items: &[TodoItem]) -> String {
    let total = items.len();
    let done_count = items.iter().filter(|t| t.done).count();
    let pct = if total > 0 { done_count * 100 / total } else { 0 };
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // Embed data as JSON for the interactive table
    let json_rows: String = items
        .iter()
        .map(|t| {
            format!(
                r#"{{"id":{},"text":{},"done":{},"date":"{}"}}"#,
                t.id,
                serde_json::to_string(&t.text).unwrap_or_default(),
                if t.done { "true" } else { "false" },
                &t.created_at[..10],
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
.container{{max-width:860px;margin:0 auto}}
header{{display:flex;align-items:center;gap:14px;margin-bottom:28px}}
.logo{{font-size:26px}}
h1{{font-size:21px;font-weight:700;color:#cba6f7}}
.sub{{font-size:12px;color:#6c7086;margin-top:3px}}
/* Stats bar */
.stats{{background:#313244;border-radius:12px;padding:16px 22px;margin-bottom:24px;display:flex;align-items:center;gap:28px}}
.stat{{text-align:center}}
.stat .num{{font-size:24px;font-weight:700;color:#cba6f7}}
.stat .lbl{{font-size:11px;color:#6c7086;margin-top:2px}}
.progress-wrap{{flex:1}}
.progress-lbl{{font-size:11px;color:#6c7086;margin-bottom:5px}}
.progress-bar{{height:7px;background:#45475a;border-radius:4px;overflow:hidden}}
.progress-fill{{height:100%;background:#a6e3a1;border-radius:4px}}
/* Toolbar */
.toolbar{{display:flex;align-items:center;gap:10px;margin-bottom:16px;flex-wrap:wrap}}
.toolbar label{{font-size:12px;color:#6c7086}}
.toolbar select,.toolbar input{{background:#313244;border:1px solid #45475a;color:#cdd6f4;border-radius:7px;padding:5px 10px;font-size:13px;outline:none;cursor:pointer}}
.toolbar select:focus,.toolbar input:focus{{border-color:#cba6f7}}
.toolbar input{{flex:1;min-width:140px}}
.toolbar .sep{{flex:1}}
/* Table */
.card{{background:#313244;border-radius:12px;overflow:hidden}}
table{{width:100%;border-collapse:collapse}}
th{{padding:10px 12px;font-size:11px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:#6c7086;text-align:left;cursor:pointer;user-select:none;white-space:nowrap;background:#2a2a3d;border-bottom:1px solid #45475a}}
th:hover{{color:#cba6f7}}
th .sort-arrow{{margin-left:4px;opacity:.4;font-size:10px}}
th.active .sort-arrow{{opacity:1;color:#cba6f7}}
td{{padding:11px 12px;font-size:14px;vertical-align:middle;border-bottom:1px solid #2a2a3d}}
tr:last-child td{{border-bottom:none}}
tr:hover td{{background:#2e2e40}}
td.col-id{{color:#6c7086;width:52px;font-variant-numeric:tabular-nums;font-size:13px}}
td.col-text{{line-height:1.5}}
td.col-text.done-text{{color:#6c7086;text-decoration:line-through}}
td.col-status{{width:110px;white-space:nowrap}}
td.col-date{{color:#6c7086;font-size:12px;width:100px;white-space:nowrap}}
.badge{{display:inline-flex;align-items:center;gap:4px;font-size:11px;font-weight:600;padding:2px 9px;border-radius:20px}}
.badge-pending{{background:#f38ba820;color:#f38ba8;border:1px solid #f38ba840}}
.badge-done{{background:#a6e3a120;color:#a6e3a1;border:1px solid #a6e3a140}}
/* Group header rows */
tr.group-header td{{background:#252535;color:#6c7086;font-size:11px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;padding:7px 12px;border-bottom:1px solid #45475a}}
/* Empty state */
.empty{{text-align:center;color:#6c7086;padding:32px;font-size:14px}}
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
      <option value="date">Date</option>
    </select>
  </div>

  <div class="card">
    <table id="todo-table">
      <thead>
        <tr>
          <th data-col="id" class="active">ID <span class="sort-arrow">▲</span></th>
          <th data-col="text">Task <span class="sort-arrow">↕</span></th>
          <th data-col="status">Status <span class="sort-arrow">↕</span></th>
          <th data-col="date">Created <span class="sort-arrow">↕</span></th>
        </tr>
      </thead>
      <tbody id="tbody"></tbody>
    </table>
    <div class="empty" id="empty" style="display:none">No todos match your filter.</div>
  </div>

  <div class="footer">Generated by OmniLauncher · {timestamp}</div>
</div>

<script>
const RAW = [{json_rows}];

let sortCol = 'id', sortDir = 1, groupBy = 'none', filterText = '';

function render() {{
  let data = RAW.filter(t => {{
    if (!filterText) return true;
    return t.text.toLowerCase().includes(filterText.toLowerCase());
  }});

  // Sort
  data.sort((a, b) => {{
    let av = a[sortCol], bv = b[sortCol];
    if (typeof av === 'string') av = av.toLowerCase();
    if (typeof bv === 'string') bv = bv.toLowerCase();
    if (av < bv) return -sortDir;
    if (av > bv) return  sortDir;
    return 0;
  }});

  // Update stats
  const vis = data.length;
  const visDone = data.filter(t => t.done).length;
  const visPending = vis - visDone;
  document.getElementById('s-total').textContent = vis;
  document.getElementById('s-pending').textContent = visPending;
  document.getElementById('s-done').textContent = visDone;
  const pct = vis > 0 ? Math.round(visDone * 100 / vis) : 0;
  document.getElementById('pct-lbl').textContent = pct + '% complete';
  document.getElementById('pct-fill').style.width = pct + '%';

  const tbody = document.getElementById('tbody');
  tbody.innerHTML = '';

  if (data.length === 0) {{
    document.getElementById('empty').style.display = 'block';
    return;
  }}
  document.getElementById('empty').style.display = 'none';

  // Group
  if (groupBy === 'none') {{
    data.forEach(t => tbody.appendChild(makeRow(t)));
  }} else {{
    const groups = new Map();
    data.forEach(t => {{
      const key = groupBy === 'status' ? (t.done ? '✅ Done' : '⬜ Pending') : t.date;
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(t);
    }});
    // Sort group keys
    const keys = [...groups.keys()].sort();
    if (groupBy === 'status') keys.reverse(); // Pending first
    keys.forEach(key => {{
      const hdr = document.createElement('tr');
      hdr.className = 'group-header';
      hdr.innerHTML = `<td colspan="4">${{key}} <span style="font-weight:400;opacity:.6">(${{groups.get(key).length}})</span></td>`;
      tbody.appendChild(hdr);
      groups.get(key).forEach(t => tbody.appendChild(makeRow(t)));
    }});
  }}
}}

function makeRow(t) {{
  const tr = document.createElement('tr');
  tr.innerHTML = `
    <td class="col-id">#${{t.id}}</td>
    <td class="col-text ${{t.done ? 'done-text' : ''}}">${{esc(t.text)}}</td>
    <td class="col-status"><span class="badge ${{t.done ? 'badge-done' : 'badge-pending'}}">${{t.done ? '✅ Done' : '⬜ Pending'}}</span></td>
    <td class="col-date">${{t.date}}</td>
  `;
  return tr;
}}

function esc(s) {{
  return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}}

// Sort click
document.querySelectorAll('th[data-col]').forEach(th => {{
  th.addEventListener('click', () => {{
    const col = th.dataset.col === 'status' ? 'done' : th.dataset.col;
    if (sortCol === col) {{ sortDir *= -1; }}
    else {{ sortCol = col; sortDir = 1; }}
    document.querySelectorAll('th').forEach(h => {{
      h.classList.remove('active');
      h.querySelector('.sort-arrow').textContent = '↕';
    }});
    th.classList.add('active');
    th.querySelector('.sort-arrow').textContent = sortDir === 1 ? '▲' : '▼';
    render();
  }});
}});

document.getElementById('group-by').addEventListener('change', e => {{
  groupBy = e.target.value; render();
}});
document.getElementById('search').addEventListener('input', e => {{
  filterText = e.target.value; render();
}});

render();
</script>
</body>
</html>"#,
        total = total,
        pending_count = total - done_count,
        done_count = done_count,
        pct = pct,
        timestamp = timestamp,
        json_rows = json_rows,
    )
}

fn write_and_open_html(items: &[TodoItem]) -> String {
    let html = generate_html(items);
    let path = html_path();
    match std::fs::write(&path, &html) {
        Err(e) => return format!("Failed to write HTML: {}", e),
        Ok(_) => {}
    }
    let url = format!("file://{}", path.display());
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", &url])
        .spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
    format!(
        "Opened todos in browser ({} items). File: {}",
        items.len(),
        path.display()
    )
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
        Some("todo ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let term = q.raw.strip_prefix("todo ").unwrap_or("").trim();

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
                .map(|item| QueryResult {
                    id: format!("todo:{}", item.id),
                    title: format!("{} {}", if item.done { "✅" } else { "☐" }, item.text),
                    subtitle: Some(format!("#{} · {}", item.id, &item.created_at[..10])),
                    icon: None,
                    score: 60,
                    action_type: "copy".to_string(),
                    action_data: item.text.clone(),
                })
                .collect();
        }

        if term == "view" || term == "show" {
            return vec![QueryResult {
                id: "todo:view".to_string(),
                title: "Open todos in browser".to_string(),
                subtitle: Some("Generates HTML and opens your default browser".to_string()),
                icon: Some("🌐".to_string()),
                score: 80,
                action_type: "todo_view".to_string(),
                action_data: String::new(),
            }];
        }

        // "done <id>" / "undone <id>" / "remove <id>" / "rm <id>"
        if let Some(rest) = term.strip_prefix("done ") {
            if let Ok(id) = rest.trim().parse::<i64>() {
                return vec![QueryResult {
                    id: format!("todo:done:{}", id),
                    title: format!("Mark #{} done", id),
                    subtitle: Some("Press Enter to confirm".to_string()),
                    icon: Some("✅".to_string()),
                    score: 80,
                    action_type: "todo_done".to_string(),
                    action_data: id.to_string(),
                }];
            }
        }
        if let Some(rest) = term.strip_prefix("undone ") {
            if let Ok(id) = rest.trim().parse::<i64>() {
                return vec![QueryResult {
                    id: format!("todo:undone:{}", id),
                    title: format!("Mark #{} undone", id),
                    subtitle: Some("Press Enter to confirm".to_string()),
                    icon: Some("↩".to_string()),
                    score: 80,
                    action_type: "todo_undone".to_string(),
                    action_data: id.to_string(),
                }];
            }
        }
        if let Some(rest) = term.strip_prefix("remove ").or_else(|| term.strip_prefix("rm ")) {
            if let Ok(id) = rest.trim().parse::<i64>() {
                return vec![QueryResult {
                    id: format!("todo:remove:{}", id),
                    title: format!("Remove #{}", id),
                    subtitle: Some("Press Enter to confirm".to_string()),
                    icon: Some("🗑".to_string()),
                    score: 80,
                    action_type: "todo_remove".to_string(),
                    action_data: id.to_string(),
                }];
            }
        }

        // Otherwise: add a new todo
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
                "description": "Manage a persistent todo list stored in SQLite. Actions: list, add, remove, done, undone, clear, view (generates HTML and opens browser), note_save, note_read.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["list", "add", "remove", "done", "undone", "clear", "view", "note_save", "note_read"],
                            "description": "Action to perform"
                        },
                        "text": {
                            "type": "string",
                            "description": "Todo text (for add), numeric id (for remove/done/undone), or note key (for note_save/note_read)"
                        },
                        "content": {
                            "type": "string",
                            "description": "Note content (for note_save only)"
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

        match action {
            "list" => {
                let items = load_todos();
                if items.is_empty() {
                    return "Todo list is empty.".to_string();
                }
                items
                    .iter()
                    .map(|t| {
                        format!(
                            "{}. [{}] {} (id: {})",
                            t.id,
                            if t.done { "x" } else { " " },
                            t.text,
                            t.id
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
                    return format!("Invalid id: '{}'. Use numeric id from list.", text);
                }
                remove_todo(id)
            }
            "done" => {
                let id: i64 = text.parse().unwrap_or(0);
                if id == 0 {
                    return format!("Invalid id: '{}'. Use numeric id from list.", text);
                }
                set_done(id, true)
            }
            "undone" => {
                let id: i64 = text.parse().unwrap_or(0);
                if id == 0 {
                    return format!("Invalid id: '{}'. Use numeric id from list.", text);
                }
                set_done(id, false)
            }
            "clear" => clear_todos(),
            "view" => {
                let items = load_todos();
                write_and_open_html(&items)
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
