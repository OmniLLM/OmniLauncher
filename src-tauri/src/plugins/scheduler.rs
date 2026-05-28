use crate::db;
use crate::path_config;
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use chrono::{Datelike, Local, TimeZone, Timelike};
use rusqlite::{params, Connection};
use std::path::PathBuf;

pub struct SchedulerPlugin;

// ─── DB helpers ──────────────────────────────────────────────────────────────

fn db_path() -> PathBuf {
    path_config::data_dir().join("omnilauncher.sqlite")
}

fn open_db() -> rusqlite::Result<Connection> {
    let dir = path_config::data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| {
        rusqlite::Error::InvalidPath(format!("Failed to create data dir {:?}: {}", dir, e).into())
    })?;
    let conn = Connection::open(db_path())?;
    // Allow concurrent readers (dashboard polls every 5s) and let writers wait
    // up to 5s instead of failing immediately with SQLITE_BUSY.
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    let _ = conn.pragma_update(None, "busy_timeout", 5000);
    db::run_migrations(&conn)?;
    Ok(conn)
}

// ─── Schedule types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Schedule {
    /// Interval: every N seconds
    Interval(u64),
    /// Cron: "min hour dom month dow"
    Cron(String),
}

impl Schedule {
    /// Parse from stored string.
    /// Format: "every:<seconds>" or "cron:<5-fields>"
    pub fn from_stored(s: &str) -> Option<Self> {
        if let Some(rest) = s.strip_prefix("every:") {
            rest.parse::<u64>().ok().map(Schedule::Interval)
        } else {
            s.strip_prefix("cron:")
                .map(|rest| Schedule::Cron(rest.to_string()))
        }
    }

    pub fn to_stored(&self) -> String {
        match self {
            Schedule::Interval(secs) => format!("every:{}", secs),
            Schedule::Cron(expr) => format!("cron:{}", expr),
        }
    }

    /// Human-readable label
    pub fn display(&self) -> String {
        match self {
            Schedule::Interval(secs) => {
                if *secs < 60 {
                    format!("every {}s", secs)
                } else if *secs < 3600 {
                    format!("every {}m", secs / 60)
                } else {
                    format!("every {}h", secs / 3600)
                }
            }
            Schedule::Cron(expr) => format!("cron({})", expr),
        }
    }

    /// Compute next run time (Unix timestamp seconds) from now.
    pub fn next_from_now(&self) -> i64 {
        let now = now_unix();
        match self {
            Schedule::Interval(secs) => now + *secs as i64,
            Schedule::Cron(expr) => next_cron_time(expr, now).unwrap_or(now + 60),
        }
    }
}

/// Current Unix timestamp (seconds).
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Minimal cron parser: "min hour dom month dow"
/// Returns next fire time as Unix seconds, or None on parse error.
/// Cron fields are interpreted in the user's **local** time zone.
fn next_cron_time(expr: &str, from_secs: i64) -> Option<i64> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return None;
    }
    let (min_f, hour_f, dom_f, mon_f, dow_f) =
        (fields[0], fields[1], fields[2], fields[3], fields[4]);

    // Start at the next whole minute in local time, at least 60s from now.
    let start = Local.timestamp_opt(from_secs + 60, 0).single()?;
    let mut t = start
        .with_second(0)?
        .with_nanosecond(0)?;

    for _ in 0..(366 * 24 * 60) {
        let min = t.minute();
        let hour = t.hour();
        let dom = t.day();
        // chrono: Mon=1..Sun=7; cron: Sun=0..Sat=6 (also accepts 7 for Sun)
        let dow = t.weekday().num_days_from_sunday();
        let mon = t.month();
        if field_matches(min_f, min)
            && field_matches(hour_f, hour)
            && field_matches(dom_f, dom)
            && field_matches(mon_f, mon)
            && (field_matches(dow_f, dow) || (dow == 0 && field_matches(dow_f, 7)))
        {
            return Some(t.timestamp());
        }
        t += chrono::Duration::minutes(1);
    }
    None
}

fn field_matches(field: &str, val: u32) -> bool {
    if field == "*" {
        return true;
    }
    if let Some(step) = field.strip_prefix("*/") {
        if let Ok(n) = step.parse::<u32>() {
            return n > 0 && val.is_multiple_of(n);
        }
    }
    if let Ok(n) = field.parse::<u32>() {
        return n == val;
    }
    // Range "N-M"
    if let Some((a, b)) = field.split_once('-') {
        if let (Ok(lo), Ok(hi)) = (a.parse::<u32>(), b.parse::<u32>()) {
            return val >= lo && val <= hi;
        }
    }
    // List "N,M,..."
    for part in field.split(',') {
        if let Ok(n) = part.trim().parse::<u32>() {
            if n == val {
                return true;
            }
        }
    }
    false
}

// ─── DB operations ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Job {
    pub id: i64,
    pub label: String,
    pub schedule: String,
    pub command: String,
    pub enabled: bool,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
    pub run_count: i64,
}

pub fn list_jobs() -> Vec<Job> {
    let Ok(conn) = open_db() else { return vec![] };
    let Ok(mut stmt) = conn.prepare(
        "SELECT id, label, schedule, command, enabled, last_run, next_run, run_count FROM scheduled_jobs ORDER BY id",
    ) else { return vec![] };
    stmt.query_map([], |row| {
        Ok(Job {
            id: row.get(0)?,
            label: row.get(1)?,
            schedule: row.get(2)?,
            command: row.get(3)?,
            enabled: row.get::<_, i64>(4)? != 0,
            last_run: row.get(5)?,
            next_run: row.get(6)?,
            run_count: row.get(7)?,
        })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

pub fn add_job(label: &str, schedule: &Schedule, command: &str) -> rusqlite::Result<i64> {
    let conn = open_db()?;
    let sched_str = schedule.to_stored();
    let next = unix_to_iso(schedule.next_from_now());
    conn.execute(
        "INSERT INTO scheduled_jobs (label, schedule, command, enabled, next_run) VALUES (?1, ?2, ?3, 1, ?4)",
        params![label, sched_str, command, next],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_job(id: i64) -> bool {
    let Ok(conn) = open_db() else { return false };
    conn.execute("DELETE FROM scheduled_jobs WHERE id = ?1", params![id])
        .map(|n| n > 0)
        .unwrap_or(false)
}

pub fn toggle_job(id: i64, enabled: bool) -> bool {
    let Ok(conn) = open_db() else { return false };
    conn.execute(
        "UPDATE scheduled_jobs SET enabled = ?1 WHERE id = ?2",
        params![if enabled { 1i64 } else { 0i64 }, id],
    )
    .map(|n| n > 0)
    .unwrap_or(false)
}

/// Called by the scheduler background task after executing a job.
pub fn record_run(id: i64, schedule: &Schedule) {
    let conn = match open_db() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[scheduler] record_run open_db failed for job #{}: {}", id, e);
            return;
        }
    };
    let now = unix_to_iso(now_unix());
    let next = unix_to_iso(schedule.next_from_now());
    if let Err(e) = conn.execute(
        "UPDATE scheduled_jobs SET last_run = ?1, next_run = ?2, run_count = run_count + 1 WHERE id = ?3",
        params![now, next, id],
    ) {
        eprintln!("[scheduler] record_run UPDATE failed for job #{}: {}", id, e);
    }
}

fn unix_to_iso(t: i64) -> String {
    // Simple ISO8601 UTC — no chrono dep needed
    // We store as seconds-since-epoch string for easy comparison
    format!("{}", t)
}

// ─── Background scheduler task ────────────────────────────────────────────────

/// Launch a background tokio task that polls every 30s and fires due jobs.
/// Call once at app startup from main.rs.
pub fn start_scheduler() {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            tick_scheduler();
        }
    });
}

fn tick_scheduler() {
    let now = now_unix();
    let jobs = list_jobs();
    for job in jobs {
        if !job.enabled {
            continue;
        }
        let Some(sched) = Schedule::from_stored(&job.schedule) else {
            continue;
        };

        // Sanity-check: if the schedule can't compute a real next-fire time
        // (e.g. legacy malformed cron like "11:33"), disable it instead of
        // letting it fall back to "fire every 60s".
        if let Schedule::Cron(ref expr) = sched {
            if next_cron_time(expr, now).is_none() {
                eprintln!(
                    "[scheduler] job #{} has invalid cron '{}' — disabling",
                    job.id, expr
                );
                let _ = toggle_job(job.id, false);
                continue;
            }
        }

        // Parse next_run as unix seconds
        let next: i64 = job
            .next_run
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        if next <= now {
            // Record the run BEFORE spawning the command. This guarantees the
            // run is counted even if the spawned process hangs or panics, and
            // immediately advances `next_run` so the next tick (30s later)
            // doesn't re-fire the same job while it's still executing.
            record_run(job.id, &sched);
            run_job(job.id, &job.label, &job.command, &sched);
        }
    }
}

fn run_job(id: i64, label: &str, command: &str, schedule: &Schedule) {
    let cmd = command.to_string();
    let label = label.to_string();
    let _ = schedule; // reserved for future use (e.g. timeout based on schedule)

    tokio::spawn(async move {
        #[cfg(windows)]
        let output = {
            // Write the command to a temp .cmd file and run it via cmd /C.
            // This avoids fragile nested-quote handling when the command itself
            // contains things like `powershell -Command "..."`, and lets the
            // user's command be interpreted as a normal Windows batch line.
            let mut path = std::env::temp_dir();
            path.push(format!(
                "omnilauncher_job_{}_{}.cmd",
                id,
                now_unix()
            ));
            let write_res = std::fs::write(&path, format!("@echo off\r\n{}\r\n", cmd));
            let result = match write_res {
                Ok(()) => {
                    tokio::process::Command::new("cmd")
                        .args(["/C", path.to_str().unwrap_or("")])
                        .output()
                        .await
                }
                Err(e) => Err(e),
            };
            let _ = std::fs::remove_file(&path);
            result
        };
        #[cfg(not(windows))]
        let output = tokio::process::Command::new("sh")
            .args(["-c", &cmd])
            .output()
            .await;

        let result_msg = match output {
            Ok(out) => {
                let stdout = strip_clixml(&String::from_utf8_lossy(&out.stdout));
                let stderr = strip_clixml(&String::from_utf8_lossy(&out.stderr));
                if out.status.success() {
                    format!("✅ {}: {}", label, stdout.trim())
                } else {
                    format!("❌ {}: {}", label, stderr.trim())
                }
            }
            Err(e) => format!("❌ {}: failed to run: {}", label, e),
        };

        // System notification (run is already recorded — see tick_scheduler).
        send_notification(&label, &result_msg);
    });
}

// Strip PowerShell CLIXML banner/payload from captured stdout/stderr.
// When PS is invoked from cmd, it emits "#< CLIXML\n<Objs ...>...</Objs>" on stderr.
fn strip_clixml(s: &str) -> String {
    s.lines()
        .filter(|line| {
            let t = line.trim();
            !t.starts_with("#< CLIXML") && !t.starts_with("<Objs ") && !t.starts_with("<Obj ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn send_notification(title: &str, body: &str) {
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("notify-send")
            .args([title, body])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            body.replace('"', "\\\""),
            title.replace('"', "\\\"")
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .spawn();
    }
    #[cfg(target_os = "windows")]
    {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        // Build a PowerShell script and pass it via -EncodedCommand (UTF-16LE base64)
        // so we don't have to escape quotes / `<` / `=` / `$` in the title or body.
        let script = "Add-Type -AssemblyName System.Windows.Forms | Out-Null; \
             [System.Windows.Forms.MessageBox]::Show($env:OL_NOTIFY_BODY, $env:OL_NOTIFY_TITLE) | Out-Null";
        let utf16: Vec<u16> = script.encode_utf16().collect();
        let mut bytes = Vec::with_capacity(utf16.len() * 2);
        for u in utf16 {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let encoded = STANDARD.encode(&bytes);
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-EncodedCommand", &encoded])
            .env("OL_NOTIFY_TITLE", format!("OmniLauncher: {}", title))
            .env("OL_NOTIFY_BODY", body)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

// ─── Parse user input ─────────────────────────────────────────────────────────

/// Parse duration string like "5m", "1h", "30s" into seconds
fn parse_interval(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix('s') {
        return n.parse().ok();
    }
    if let Some(n) = s.strip_suffix('m') {
        return n.parse::<u64>().ok().map(|v| v * 60);
    }
    if let Some(n) = s.strip_suffix('h') {
        return n.parse::<u64>().ok().map(|v| v * 3600);
    }
    if let Some(n) = s.strip_suffix("min") {
        return n.parse::<u64>().ok().map(|v| v * 60);
    }
    if let Some(n) = s.strip_suffix("sec") {
        return n.parse().ok();
    }
    None
}

/// Normalize a user-supplied schedule string into a validated `Schedule`.
///
/// Accepts:
///   - intervals: "5m", "1h", "30s", "10min", "45sec"
///   - daily time shorthand: "HH:MM" → cron "MM HH * * *"
///   - explicit cron prefix: "cron:M H dom mon dow"
///   - bare 5-field cron: "M H dom mon dow"
///
/// Returns `Err(msg)` when the string can't be parsed into a valid recurring
/// schedule, so the caller can refuse to persist a job that would mis-fire.
fn normalize_schedule(input: &str) -> Result<Schedule, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("empty schedule".to_string());
    }

    // Interval forms (5m, 1h, ...).
    if let Some(secs) = parse_interval(s) {
        if secs == 0 {
            return Err("interval must be > 0 seconds".to_string());
        }
        return Ok(Schedule::Interval(secs));
    }

    // Strip optional "cron:" prefix.
    let cron_body = s.strip_prefix("cron:").map(str::trim).unwrap_or(s);

    // "HH:MM" daily shorthand.
    if let Some((hh, mm)) = cron_body.split_once(':') {
        if !hh.contains(' ') && !mm.contains(' ') {
            if let (Ok(h), Ok(m)) = (hh.parse::<u32>(), mm.parse::<u32>()) {
                if h < 24 && m < 60 {
                    let expr = format!("{} {} * * *", m, h);
                    if next_cron_time(&expr, now_unix()).is_some() {
                        return Ok(Schedule::Cron(expr));
                    }
                }
                return Err(format!(
                    "invalid time '{}' — expected HH:MM (00-23:00-59)",
                    cron_body
                ));
            }
        }
    }

    // Full 5-field cron.
    let fields: Vec<&str> = cron_body.split_whitespace().collect();
    if fields.len() == 5 {
        let expr = fields.join(" ");
        if next_cron_time(&expr, now_unix()).is_some() {
            return Ok(Schedule::Cron(expr));
        }
        return Err(format!("invalid cron expression '{}'", expr));
    }

    Err(format!(
        "could not parse schedule '{}' — use '5m', '1h', 'HH:MM', or 'M H dom mon dow'",
        input
    ))
}

// ─── Plugin impl ──────────────────────────────────────────────────────────────

#[async_trait]
impl Plugin for SchedulerPlugin {
    fn name(&self) -> &str {
        "scheduler"
    }

    fn description(&self) -> &str {
        "Manage scheduled recurring tasks (sched list/add/del/on/off)"
    }

    fn keyword(&self) -> Option<&str> {
        Some("sched")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = q.raw.trim();

        // bare "sched" → list
        if raw == "sched" || raw == "sched list" {
            return list_results();
        }

        // sched del <id>
        if let Some(rest) = raw
            .strip_prefix("sched del ")
            .or_else(|| raw.strip_prefix("sched delete "))
        {
            let rest = rest.trim();
            if let Ok(id) = rest.parse::<i64>() {
                return vec![QueryResult {
                    id: format!("sched:del:{}", id),
                    title: format!("🗑️ Delete job #{}", id),
                    subtitle: Some("Press Enter to confirm deletion".to_string()),
                    icon: Some("🗑️".to_string()),
                    score: 100,
                    action_type: "sched_del".to_string(),
                    action_data: id.to_string(),
                }];
            }
        }

        // sched off/on <id>
        if let Some(rest) = raw
            .strip_prefix("sched off ")
            .or_else(|| raw.strip_prefix("sched disable "))
        {
            if let Ok(id) = rest.trim().parse::<i64>() {
                return vec![QueryResult {
                    id: format!("sched:off:{}", id),
                    title: format!("⏸ Pause job #{}", id),
                    subtitle: Some("Press Enter to pause".to_string()),
                    icon: Some("⏸".to_string()),
                    score: 100,
                    action_type: "sched_off".to_string(),
                    action_data: id.to_string(),
                }];
            }
        }

        if let Some(rest) = raw
            .strip_prefix("sched on ")
            .or_else(|| raw.strip_prefix("sched enable "))
        {
            if let Ok(id) = rest.trim().parse::<i64>() {
                return vec![QueryResult {
                    id: format!("sched:on:{}", id),
                    title: format!("▶️ Resume job #{}", id),
                    subtitle: Some("Press Enter to resume".to_string()),
                    icon: Some("▶️".to_string()),
                    score: 100,
                    action_type: "sched_on".to_string(),
                    action_data: id.to_string(),
                }];
            }
        }

        // sched add "label" every 5m <command>
        // sched add "label" * * * * * <command>
        if let Some(rest) = raw.strip_prefix("sched add ") {
            return parse_add_preview(rest);
        }

        // Hint when typing "sched "
        if raw.starts_with("sched ") {
            return hint_results();
        }

        vec![]
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "scheduler",
                "description": "Manage scheduled recurring tasks: list, add, delete, enable or disable jobs",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "description": "'list', 'add', 'delete', 'enable', or 'disable'" },
                        "id": { "type": "integer", "description": "Job ID (required for delete/enable/disable)" },
                        "label": { "type": "string", "description": "Job label (required for add)" },
                        "schedule": { "type": "string", "description": "Schedule for add. One of: interval ('5m','1h','30s','45sec','10min'); daily time 'HH:MM' (e.g. '09:30' = every day at 9:30); 5-field cron '*/5 * * * *'. Plain values like '11:33' MUST be in HH:MM 24h form." },
                        "command": { "type": "string", "description": "Shell command to run (required for add)" }
                    },
                    "required": ["action"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let action = args["action"].as_str().unwrap_or("").trim();
        match action {
            "list" => {
                let jobs = list_jobs();
                if jobs.is_empty() {
                    return "No scheduled jobs".to_string();
                }
                jobs.iter().map(|j| {
                    let sched_display = Schedule::from_stored(&j.schedule)
                        .map(|s| s.display())
                        .unwrap_or_else(|| j.schedule.clone());
                    let status = if j.enabled { "enabled" } else { "disabled" };
                    format!("#{} [{}] {} — {} | cmd: {} | runs: {}", j.id, status, j.label, sched_display, j.command, j.run_count)
                }).collect::<Vec<_>>().join("\n")
            }
            "add" => {
                let label = match args["label"].as_str() {
                    Some(l) if !l.is_empty() => l,
                    _ => return "Error: 'label' is required for add".to_string(),
                };
                let sched_str = match args["schedule"].as_str() {
                    Some(s) if !s.is_empty() => s,
                    _ => return "Error: 'schedule' is required for add (e.g. '5m' or '*/5 * * * *')".to_string(),
                };
                let cmd = match args["command"].as_str() {
                    Some(c) if !c.is_empty() => c,
                    _ => return "Error: 'command' is required for add".to_string(),
                };
                let schedule = match normalize_schedule(sched_str) {
                    Ok(s) => s,
                    Err(e) => return format!("Error: {}", e),
                };
                match add_job(label, &schedule, cmd) {
                    Ok(id) => format!("Job #{} added: {} — {} | cmd: {}", id, label, schedule.display(), cmd),
                    Err(e) => format!("Error adding job: {}", e),
                }
            }
            "delete" => {
                let id = match args["id"].as_i64() {
                    Some(i) => i,
                    None => return "Error: 'id' is required for delete".to_string(),
                };
                if delete_job(id) {
                    format!("Job #{} deleted", id)
                } else {
                    format!("Job #{} not found", id)
                }
            }
            "enable" => {
                let id = match args["id"].as_i64() {
                    Some(i) => i,
                    None => return "Error: 'id' is required for enable".to_string(),
                };
                if toggle_job(id, true) { format!("Job #{} enabled", id) } else { format!("Job #{} not found", id) }
            }
            "disable" => {
                let id = match args["id"].as_i64() {
                    Some(i) => i,
                    None => return "Error: 'id' is required for disable".to_string(),
                };
                if toggle_job(id, false) { format!("Job #{} disabled", id) } else { format!("Job #{} not found", id) }
            }
            _ => format!("Unknown action: '{}'. Use: list, add, delete, enable, disable", action),
        }
    }
}

fn list_results() -> Vec<QueryResult> {
    let jobs = list_jobs();
    if jobs.is_empty() {
        return vec![QueryResult {
            id: "sched:empty".to_string(),
            title: "No scheduled jobs yet".to_string(),
            subtitle: Some("Type: sched add \"My Job\" every 5m echo hello".to_string()),
            icon: Some("📅".to_string()),
            score: 50,
            action_type: "none".to_string(),
            action_data: String::new(),
        }];
    }

    jobs.iter()
        .map(|j| {
            let sched_display = Schedule::from_stored(&j.schedule)
                .map(|s| s.display())
                .unwrap_or_else(|| j.schedule.clone());
            let status = if j.enabled { "✅" } else { "⏸" };
            let last = j.last_run.as_deref().unwrap_or("never");
            QueryResult {
                id: format!("sched:job:{}", j.id),
                title: format!("{} #{} {} — {}", status, j.id, j.label, sched_display),
                subtitle: Some(format!(
                    "cmd: {} | last: {} | runs: {}",
                    j.command, last, j.run_count
                )),
                icon: Some("📅".to_string()),
                score: 90,
                action_type: "none".to_string(),
                action_data: String::new(),
            }
        })
        .collect()
}

fn hint_results() -> Vec<QueryResult> {
    vec![
        QueryResult {
            id: "sched:hint:list".to_string(),
            title: "sched list".to_string(),
            subtitle: Some("Show all scheduled jobs".to_string()),
            icon: Some("📅".to_string()),
            score: 80,
            action_type: "none".to_string(),
            action_data: String::new(),
        },
        QueryResult {
            id: "sched:hint:add".to_string(),
            title: "sched add \"Label\" every 5m <command>".to_string(),
            subtitle: Some("Add a recurring job (every 5m/1h/30s · at 09:30 · or 5-field cron)".to_string()),
            icon: Some("➕".to_string()),
            score: 75,
            action_type: "none".to_string(),
            action_data: String::new(),
        },
        QueryResult {
            id: "sched:hint:del".to_string(),
            title: "sched del <id>".to_string(),
            subtitle: Some("Delete a job by ID".to_string()),
            icon: Some("🗑️".to_string()),
            score: 70,
            action_type: "none".to_string(),
            action_data: String::new(),
        },
        QueryResult {
            id: "sched:hint:toggle".to_string(),
            title: "sched off/on <id>".to_string(),
            subtitle: Some("Pause or resume a job".to_string()),
            icon: Some("⏸".to_string()),
            score: 65,
            action_type: "none".to_string(),
            action_data: String::new(),
        },
    ]
}

/// Parse "add" sub-command and return a preview QueryResult.
/// Formats:
///   "Label" every 5m echo hello
///   "Label" 0 9 * * * echo hello
fn parse_add_preview(rest: &str) -> Vec<QueryResult> {
    let rest = rest.trim();

    // Extract quoted label
    let (label, after_label) = if let Some(stripped) = rest.strip_prefix('"') {
        if let Some(end) = stripped.find('"') {
            (&stripped[..end], stripped[end + 1..].trim())
        } else {
            return vec![];
        }
    } else if let Some(stripped) = rest.strip_prefix('\'') {
        if let Some(end) = stripped.find('\'') {
            (&stripped[..end], stripped[end + 1..].trim())
        } else {
            return vec![];
        }
    } else {
        // No quotes: first word is label
        if let Some((lbl, rest2)) = rest.split_once(' ') {
            (lbl, rest2.trim())
        } else {
            return vec![];
        }
    };

    if after_label.is_empty() {
        return vec![];
    }

    // Try "every <interval> <command>"
    if let Some(ev_rest) = after_label.strip_prefix("every ") {
        let parts: Vec<&str> = ev_rest.splitn(2, ' ').collect();
        if parts.len() == 2 {
            let interval_str = parts[0];
            let cmd = parts[1];
            if let Some(secs) = parse_interval(interval_str) {
                let sched = Schedule::Interval(secs);
                let stored = sched.to_stored();
                return vec![QueryResult {
                    id: "sched:add:preview".to_string(),
                    title: format!("📅 Add job \"{}\" — {}", label, sched.display()),
                    subtitle: Some(format!("cmd: {}", cmd)),
                    icon: Some("➕".to_string()),
                    score: 100,
                    action_type: "sched_add".to_string(),
                    action_data: format!("{}|||{}|||{}", label, stored, cmd),
                }];
            }
        }
    }

    // Try "at HH:MM <command>" — daily-at-time shorthand.
    if let Some(at_rest) = after_label.strip_prefix("at ") {
        if let Some((time, cmd)) = at_rest.split_once(' ') {
            if let Ok(sched) = normalize_schedule(time) {
                let stored = sched.to_stored();
                return vec![QueryResult {
                    id: "sched:add:preview".to_string(),
                    title: format!("📅 Add job \"{}\" — daily at {}", label, time),
                    subtitle: Some(format!("cmd: {}", cmd)),
                    icon: Some("➕".to_string()),
                    score: 100,
                    action_type: "sched_add".to_string(),
                    action_data: format!("{}|||{}|||{}", label, stored, cmd),
                }];
            }
        }
    }

    // Try cron "M H dom mon dow <command>" (5 fields then command)
    let words: Vec<&str> = after_label.splitn(6, ' ').collect();
    if words.len() == 6 {
        let cron_expr = format!(
            "{} {} {} {} {}",
            words[0], words[1], words[2], words[3], words[4]
        );
        let cmd = words[5];
        if let Ok(sched) = normalize_schedule(&cron_expr) {
            let stored = sched.to_stored();
            return vec![QueryResult {
                id: "sched:add:preview".to_string(),
                title: format!("📅 Add job \"{}\" — cron({})", label, cron_expr),
                subtitle: Some(format!("cmd: {}", cmd)),
                icon: Some("➕".to_string()),
                score: 100,
                action_type: "sched_add".to_string(),
                action_data: format!("{}|||{}|||{}", label, stored, cmd),
            }];
        }
    }

    vec![]
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_scheduler_does_not_require_current_tokio_runtime() {
        start_scheduler();
    }

    #[test]
    fn test_parse_interval() {
        assert_eq!(parse_interval("5m"), Some(300));
        assert_eq!(parse_interval("1h"), Some(3600));
        assert_eq!(parse_interval("30s"), Some(30));
        assert_eq!(parse_interval("90min"), Some(5400));
        assert_eq!(parse_interval("badval"), None);
    }

    #[test]
    fn test_schedule_stored_roundtrip() {
        let s = Schedule::Interval(300);
        assert_eq!(
            Schedule::from_stored(&s.to_stored()).unwrap().to_stored(),
            s.to_stored()
        );

        let c = Schedule::Cron("0 9 * * *".to_string());
        assert_eq!(
            Schedule::from_stored(&c.to_stored()).unwrap().to_stored(),
            c.to_stored()
        );
    }

    #[test]
    fn test_field_matches() {
        assert!(field_matches("*", 0));
        assert!(field_matches("*", 59));
        assert!(field_matches("*/5", 0));
        assert!(field_matches("*/5", 15));
        assert!(!field_matches("*/5", 7));
        assert!(field_matches("9", 9));
        assert!(!field_matches("9", 10));
        assert!(field_matches("1-5", 3));
        assert!(!field_matches("1-5", 6));
        assert!(field_matches("0,30", 30));
        assert!(!field_matches("0,30", 15));
    }

    #[test]
    fn test_next_cron_time() {
        // "* * * * *" — every minute, should fire in ≤60s
        let now = now_unix();
        let next = next_cron_time("* * * * *", now).unwrap();
        assert!(next > now && next <= now + 120);
    }

    #[test]
    fn test_db_add_list_delete() {
        let dir = tempfile::tempdir().expect("tmpdir");
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", dir.path().to_str().unwrap());

        let sched = Schedule::Interval(300);
        let id = add_job("Test Job", &sched, "echo hello").unwrap();
        assert!(id > 0);

        let jobs = list_jobs();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].label, "Test Job");

        assert!(delete_job(id));
        assert_eq!(list_jobs().len(), 0);
    }

    #[tokio::test]
    async fn test_plugin_query_list_empty() {
        let dir = tempfile::tempdir().expect("tmpdir");
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", dir.path().to_str().unwrap());

        let plugin = SchedulerPlugin;
        let q = crate::plugins::Query {
            raw: "sched".to_string(),
            terms: vec![],
        };
        let results = plugin.query(&q).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].title.contains("No scheduled"));
    }

    #[tokio::test]
    async fn test_plugin_query_add_preview() {
        let plugin = SchedulerPlugin;
        let q = crate::plugins::Query {
            raw: "sched add \"Daily Report\" every 1h echo report".to_string(),
            terms: vec![],
        };
        let results = plugin.query(&q).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action_type, "sched_add");
        assert!(results[0].action_data.contains("Daily Report"));
        assert!(results[0].action_data.contains("every:3600"));
        assert!(results[0].action_data.contains("echo report"));
    }

    #[tokio::test]
    async fn test_plugin_query_cron_preview() {
        let plugin = SchedulerPlugin;
        let q = crate::plugins::Query {
            raw: "sched add \"Morning\" 0 9 * * * echo morning".to_string(),
            terms: vec![],
        };
        let results = plugin.query(&q).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action_type, "sched_add");
        assert!(results[0].action_data.contains("cron:0 9 * * *"));
    }

    #[tokio::test]
    async fn test_tick_runs_due_job_and_records() {
        let dir = tempfile::tempdir().expect("tmpdir");
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", dir.path().to_str().unwrap());

        // Use a cmd-compatible echo on Windows, sh echo elsewhere.
        #[cfg(windows)]
        let cmd = "echo hello";
        #[cfg(not(windows))]
        let cmd = "echo hello";

        let id = add_job("Smoke Test", &Schedule::Interval(1), cmd).unwrap();

        // Backdate next_run so the tick considers the job due immediately.
        {
            let conn = open_db().unwrap();
            let past = unix_to_iso(now_unix() - 5);
            conn.execute(
                "UPDATE scheduled_jobs SET next_run = ?1 WHERE id = ?2",
                params![past, id],
            )
            .unwrap();
        }

        tick_scheduler();

        // run_job spawns the command on tokio; wait for completion + record_run.
        for _ in 0..50 {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            let jobs = list_jobs();
            if jobs.iter().any(|j| j.id == id && j.run_count > 0) {
                return; // success
            }
        }
        panic!("scheduled job never recorded a run within 10s");
    }
}
