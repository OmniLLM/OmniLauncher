//! Self-contained lifecycle / ops commands for the `ol` CLI.
//!
//! Ports the behavior of `scripts/ops.sh` into Rust so the binary owns
//! start/stop/restart/status/logs/serve/gui/doctor and works identically on
//! Linux, macOS, and Windows (process/port control via `sysinfo` + std net,
//! no `pkill`/`lsof`/`ss`). `serve` and `gui` delegate to the binary-crate
//! entrypoints (`crate::serve_backend` / `crate::run`) so their existing
//! internals — auth token, A2A, Tauri setup — are preserved verbatim.

use crate::cli::process;
use crate::cli::render::{Output, Status};
use std::time::Duration;

/// Resolve the server port: `OMNILAUNCHER_SERVER_PORT`, else `1422`. Mirrors
/// the binding logic in `serve_backend` so status/health probes target the same
/// port the backend actually listens on.
pub fn server_port() -> u16 {
    std::env::var("OMNILAUNCHER_SERVER_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(1422)
}

/// Loopback URL used for health/status probes against the local backend.
fn backend_url() -> String {
    format!("http://127.0.0.1:{}", server_port())
}

/// `ol gui` — launch the desktop shell in the foreground (the old default
/// no-arg action). With `--detached`, spawn it in the background and record a
/// PID file instead. `debug` forwards `--debug` to a detached child.
pub fn gui(out: &Output, detached: bool, debug: bool) -> i32 {
    if !detached {
        crate::run();
        return 0;
    }
    let exe = match process::current_exe_path() {
        Ok(p) => p,
        Err(e) => {
            out.failure(&format!("cannot locate own executable: {e}"));
            return 1;
        }
    };
    let mut child_args = vec!["gui".to_string()];
    if debug {
        child_args.push("--debug".to_string());
    }
    match process::spawn_detached(&exe, &child_args) {
        Ok(pid) => {
            let _ = process::write_pid(&process::gui_pid_file(), pid);
            out.success(&format!("gui started   pid {pid}"));
            0
        }
        Err(e) => {
            out.failure(&format!("failed to launch gui: {e}"));
            1
        }
    }
}

/// `ol start` — spawn `self serve` detached, record the PID, and wait for the
/// `/health` endpoint to come up (~5s) before reporting. `debug` forwards
/// `--debug` to the detached backend so file logging is enabled.
pub fn start(out: &Output, debug: bool) -> i32 {
    // Already running?
    if let Some(pid) = process::read_pid(&process::backend_pid_file()) {
        if process::pid_alive(pid) {
            out.info(&format!(
                "{} backend already running   pid {pid}",
                out.glyph(Status::Up)
            ));
            return 0;
        }
        // Stale PID file — clear it and continue.
        process::clear_pid(&process::backend_pid_file());
    }

    let exe = match process::current_exe_path() {
        Ok(p) => p,
        Err(e) => {
            out.failure(&format!("cannot locate own executable: {e}"));
            return 1;
        }
    };

    let mut child_args = vec!["serve".to_string()];
    if debug {
        child_args.push("--debug".to_string());
    }
    let pid = match process::spawn_detached(&exe, &child_args) {
        Ok(pid) => pid,
        Err(e) => {
            out.failure(&format!("failed to spawn backend: {e}"));
            return 1;
        }
    };
    let _ = process::write_pid(&process::backend_pid_file(), pid);

    let url = backend_url();
    if process::wait_for_health(&url, Duration::from_secs(5)) {
        out.success(&format!("backend started   pid {pid}   {url}"));
        0
    } else {
        // Spawned but health never went green. Leave it running (it may still be
        // coming up) but tell the user so they can check `ol logs`.
        out.failure(&format!(
            "backend spawned (pid {pid}) but /health did not respond within 5s — check `ol logs`"
        ));
        1
    }
}

/// `ol stop` — stop the detached backend (graceful then forceful) and clear its
/// PID file.
pub fn stop(out: &Output) -> i32 {
    let pid_file = process::backend_pid_file();
    let Some(pid) = process::read_pid(&pid_file) else {
        out.failure("backend not running");
        return 1;
    };
    if !process::pid_alive(pid) {
        process::clear_pid(&pid_file);
        out.failure("backend not running");
        return 1;
    }

    let stopped = process::stop_pid(pid, Duration::from_secs(3));
    process::clear_pid(&pid_file);
    if stopped {
        out.success(&format!("backend stopped   pid {pid}"));
        0
    } else {
        out.failure(&format!("failed to stop backend   pid {pid}"));
        1
    }
}

/// `ol restart` — `stop` (best-effort) then `start`.
pub fn restart(out: &Output, debug: bool) -> i32 {
    // Best-effort stop: a "not running" stop is fine before a start.
    let _ = stop(out);
    start(out, debug)
}

/// `ol status` — rich health/process/port/binary view.
pub fn status(out: &Output) -> i32 {
    let version = env!("CARGO_PKG_VERSION");
    let port = server_port();
    let url = backend_url();

    // Backend process
    let pid = process::read_pid(&process::backend_pid_file()).filter(|&p| process::pid_alive(p));
    let backend_up = pid.is_some();
    let mem = pid.and_then(process::pid_memory_bytes);

    // Port + health
    let port_up = process::port_listening("127.0.0.1", port);
    let health = process::probe_health(&url);
    let health_up = health == process::Health::Ok;

    // GUI (detached only — a foreground GUI has no PID file)
    let gui_pid = process::read_pid(&process::gui_pid_file()).filter(|&p| process::pid_alive(p));

    if out.json {
        let payload = serde_json::json!({
            "version": version,
            "backend": { "running": backend_up, "pid": pid, "memory_bytes": mem },
            "port": { "number": port, "listening": port_up },
            "health": { "ok": health_up },
            "gui": { "running": gui_pid.is_some(), "pid": gui_pid },
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
        );
        return if backend_up { 0 } else { 1 };
    }

    println!(
        "  {}  {}",
        out.cyan("OmniLauncher"),
        out.dim(&format!("v{version}"))
    );

    let backend_line = match (backend_up, pid, mem) {
        (true, Some(p), Some(m)) => format!(
            "{} running   pid {p}   {}",
            out.glyph(Status::Up),
            human_bytes(m)
        ),
        (true, Some(p), None) => format!("{} running   pid {p}", out.glyph(Status::Up)),
        _ => format!("{} stopped", out.glyph(Status::Down)),
    };
    println!("  backend   {backend_line}");

    let port_glyph = if port_up { Status::Up } else { Status::Down };
    let port_state = if port_up { "listening" } else { "closed" };
    println!("  port      {} {port} {port_state}", out.glyph(port_glyph));

    let health_line = match health {
        process::Health::Ok => format!("{} ok", out.glyph(Status::Up)),
        process::Health::Bad => format!("{} error", out.glyph(Status::Error)),
        process::Health::Unreachable => format!("{} unreachable", out.glyph(Status::Down)),
    };
    println!("  health    {health_line}");

    let gui_line = match gui_pid {
        Some(p) => format!("{} running   pid {p}", out.glyph(Status::Up)),
        None => format!("{} stopped", out.glyph(Status::Down)),
    };
    println!("  gui       {gui_line}");

    if backend_up {
        0
    } else {
        1
    }
}

/// `ol logs` — print the log path, the last `lines` lines, and optionally follow.
pub fn logs(out: &Output, lines: usize, follow: bool) -> i32 {
    let path = omnilauncher_lib::path_config::data_dir().join("omnilauncher.log");
    if !path.exists() {
        out.failure(&format!(
            "no log file at {} — run with --debug (e.g. `ol serve --debug`) to create it",
            path.display()
        ));
        return 1;
    }
    out.info(&out.dim(&path.display().to_string()));

    // Print the last N lines.
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let all: Vec<&str> = content.lines().collect();
            let start = all.len().saturating_sub(lines);
            for line in &all[start..] {
                println!("{line}");
            }
        }
        Err(e) => {
            out.failure(&format!("cannot read log file: {e}"));
            return 1;
        }
    }

    if follow {
        return follow_log(out, &path);
    }
    0
}

/// Tail-follow a log file, printing appended bytes as they arrive. Blocks until
/// interrupted (Ctrl-C).
fn follow_log(out: &Output, path: &std::path::Path) -> i32 {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            out.failure(&format!("cannot open log for follow: {e}"));
            return 1;
        }
    };
    let mut pos = file.seek(SeekFrom::End(0)).unwrap_or(0);
    loop {
        std::thread::sleep(Duration::from_millis(400));
        let len = file.metadata().map(|m| m.len()).unwrap_or(pos);
        if len < pos {
            // File truncated/rotated — restart from the beginning.
            pos = 0;
        }
        if len > pos {
            if file.seek(SeekFrom::Start(pos)).is_err() {
                continue;
            }
            let mut buf = String::new();
            if file.read_to_string(&mut buf).is_ok() {
                print!("{buf}");
                use std::io::Write;
                let _ = std::io::stdout().flush();
                pos = len;
            }
        }
    }
}

/// `ol doctor` — diagnostics, each line OK / WARN / FAIL.
pub fn doctor(out: &Output) -> i32 {
    let mut worst = 0; // 0 ok, 1 warn, 2 fail
    let mut line = |out: &Output, ok: DoctorState, label: &str, detail: &str| {
        let (glyph, sev) = match ok {
            DoctorState::Ok => (out.glyph(Status::Up), 0),
            DoctorState::Warn => (out.yellow("!"), 1),
            DoctorState::Fail => (out.glyph(Status::Error), 2),
        };
        worst = worst.max(sev);
        if detail.is_empty() {
            println!("  {glyph} {label}");
        } else {
            println!("  {glyph} {label}   {}", out.dim(detail));
        }
    };

    // 1. settings.json exists and parses. `load_settings` masks a malformed
    // file by silently returning defaults, so we check the raw file here to
    // surface a genuine parse failure (which would otherwise look like "your
    // settings didn't take effect").
    let settings = omnilauncher_lib::load_settings();
    let settings_path = omnilauncher_lib::path_config::config_dir().join("settings.json");
    if !settings_path.exists() {
        line(
            out,
            DoctorState::Warn,
            "settings.json",
            "not found — defaults in use",
        );
    } else {
        match std::fs::read_to_string(&settings_path) {
            Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
                Ok(_) => line(
                    out,
                    DoctorState::Ok,
                    "settings.json",
                    &settings_path.display().to_string(),
                ),
                Err(e) => line(
                    out,
                    DoctorState::Fail,
                    "settings.json",
                    &format!("malformed JSON: {e}"),
                ),
            },
            Err(e) => line(
                out,
                DoctorState::Fail,
                "settings.json",
                &format!("unreadable: {e}"),
            ),
        }
    }

    // 2. auth token present
    let token = omnilauncher_lib::settings::resolve_backend_auth_token(&settings);
    if token.is_empty() {
        line(
            out,
            DoctorState::Warn,
            "auth token",
            "none resolved (backend requests unauthenticated)",
        );
    } else {
        line(
            out,
            DoctorState::Ok,
            "auth token",
            &format!("present (len {})", token.len()),
        );
    }

    // 3. AI provider URL configured + reachable
    if settings.ai_base_url.trim().is_empty() {
        line(
            out,
            DoctorState::Warn,
            "ai endpoint",
            "ai_base_url not configured",
        );
    } else {
        let reachable = ai_endpoint_reachable(&settings.ai_base_url);
        if reachable {
            line(out, DoctorState::Ok, "ai endpoint", &settings.ai_base_url);
        } else {
            line(
                out,
                DoctorState::Warn,
                "ai endpoint",
                &format!("{} (unreachable)", settings.ai_base_url),
            );
        }
    }

    // 4. bundled python (optional dep for some plugins)
    let py = omnilauncher_lib::python_installer::bundled_python_exe().is_some();
    if py {
        line(out, DoctorState::Ok, "bundled python", "");
    } else {
        line(
            out,
            DoctorState::Warn,
            "bundled python",
            "not installed (some plugins need it)",
        );
    }

    // 5. backend running?
    let running = process::read_pid(&process::backend_pid_file())
        .map(process::pid_alive)
        .unwrap_or(false);
    if running {
        line(out, DoctorState::Ok, "backend", "running");
    } else {
        line(
            out,
            DoctorState::Warn,
            "backend",
            "not running (`ol start`)",
        );
    }

    match worst {
        0 => {
            out.success("all checks passed");
            0
        }
        1 => 0, // warnings don't fail the command
        _ => {
            out.failure("one or more checks failed");
            1
        }
    }
}

enum DoctorState {
    Ok,
    Warn,
    Fail,
}

/// Best-effort reachability check for an AI provider base URL: parse host/port
/// and try a TCP connect. Doesn't validate the API, just that something answers.
fn ai_endpoint_reachable(base_url: &str) -> bool {
    let trimmed = base_url.trim_end_matches('/');
    let (scheme_default_port, rest) = if let Some(r) = trimmed.strip_prefix("https://") {
        (443u16, r)
    } else if let Some(r) = trimmed.strip_prefix("http://") {
        (80u16, r)
    } else {
        (443u16, trimmed)
    };
    // Strip any path component.
    let hostport = rest.split('/').next().unwrap_or(rest);
    let (host, port) = match hostport.split_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().unwrap_or(scheme_default_port)),
        None => (hostport, scheme_default_port),
    };
    process::port_listening(host, port)
}

/// Format a byte count as a compact human string (e.g. `48 MB`).
fn human_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{} KB", bytes / KB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_scales() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2 KB");
        assert_eq!(human_bytes(50 * 1024 * 1024), "50 MB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn default_port_is_1422() {
        // Only holds when the env override is unset; guard to avoid clobbering
        // a developer's shell.
        if std::env::var_os("OMNILAUNCHER_SERVER_PORT").is_none() {
            assert_eq!(server_port(), 1422);
        }
    }

    #[test]
    fn ai_endpoint_parse_does_not_panic() {
        // Exercise the URL parsing branches; connectivity result is irrelevant.
        let _ = ai_endpoint_reachable("https://api.openai.com/v1");
        let _ = ai_endpoint_reachable("http://localhost:11434");
        let _ = ai_endpoint_reachable("no-scheme:1234");
    }
}
