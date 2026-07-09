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

/// Reconciled truth about the backend, derived from BOTH the PID file (what
/// `ol` started) AND reality (who listens on the port + `/health`). The PID
/// file alone is not authoritative: a backend launched via `ol serve` directly,
/// or one inherited from a previous session, serves the port without `ol`
/// having tracked it. Every lifecycle command routes through `backend_state`
/// so `status`, `start`, `restart`, and `doctor` can never disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendState {
    /// Not listening and no live tracked PID — genuinely down.
    Stopped,
    /// Serving on the port, and it is the process in our PID file.
    /// `pid` is the tracked PID; `healthy` is the `/health` result.
    Tracked { pid: u32, healthy: bool },
    /// Serving on the port, but NOT the process we track (PID file missing,
    /// stale, or pointing elsewhere). `pid` is the real owner if we could
    /// resolve it from the port (Linux), else `None`. This is the case that
    /// previously rendered as a contradictory "stopped + listening + ok".
    Untracked { pid: Option<u32>, healthy: bool },
}

impl BackendState {
    /// Whether the backend is serving at all (tracked or not). This — not "is
    /// there a live PID file" — is the real "is it up?" signal.
    pub fn is_running(&self) -> bool {
        !matches!(self, BackendState::Stopped)
    }

    /// The effective PID of the running backend, tracked or resolved from the
    /// port; `None` when stopped or when the owner couldn't be resolved.
    pub fn pid(&self) -> Option<u32> {
        match self {
            BackendState::Stopped => None,
            BackendState::Tracked { pid, .. } => Some(*pid),
            BackendState::Untracked { pid, .. } => *pid,
        }
    }
}

/// Resolve the reconciled `BackendState` from the PID file and live probes.
///
/// Precedence:
///   1. If our tracked PID is alive AND the port is listening → `Tracked`.
///   2. Else if the port is listening (someone else is serving) → `Untracked`,
///      resolving the real owner PID from the port when possible, and clearing
///      the stale PID file so we don't keep reporting a corpse.
///   3. Else → `Stopped` (clearing any dead PID file).
pub fn backend_state() -> BackendState {
    let port = server_port();
    let url = backend_url();
    let tracked = process::read_pid(&process::backend_pid_file()).filter(|&p| process::pid_alive(p));
    let listening = process::port_listening("127.0.0.1", port);

    match (tracked, listening) {
        (Some(pid), true) => {
            let healthy = process::probe_health(&url) == process::Health::Ok;
            BackendState::Tracked { pid, healthy }
        }
        (_, true) => {
            // Someone is serving but it isn't our tracked process. Drop any
            // stale PID file so subsequent commands don't trust a dead PID.
            if tracked.is_none() {
                process::clear_pid(&process::backend_pid_file());
            }
            let healthy = process::probe_health(&url) == process::Health::Ok;
            let owner = process::port_pid(port);
            BackendState::Untracked { pid: owner, healthy }
        }
        (Some(_pid), false) => {
            // Tracked PID is alive but the port is closed — it is not serving
            // (crashed listener, wrong port, still starting elsewhere). Treat as
            // stopped for lifecycle purposes; leave the PID file for `stop`.
            BackendState::Stopped
        }
        (None, false) => {
            process::clear_pid(&process::backend_pid_file());
            BackendState::Stopped
        }
    }
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
    // Already serving? Reconcile against reality, not just the PID file — a
    // backend started via `ol serve` (or inherited) holds the port without a
    // tracked PID. Spawning a second one would only lose the EADDRINUSE race
    // and leave a corpse, so report the running one honestly instead.
    match backend_state() {
        BackendState::Tracked { pid, .. } => {
            out.info(&format!(
                "{} backend already running   pid {pid}",
                out.glyph(Status::Up)
            ));
            return 0;
        }
        BackendState::Untracked { pid, .. } => {
            let who = pid
                .map(|p| format!("pid {p}"))
                .unwrap_or_else(|| "unknown pid".to_string());
            out.info(&format!(
                "{} backend already running   {who}   {} (not started by ol; use `ol restart` to take over)",
                out.glyph(Status::Up),
                out.dim("untracked")
            ));
            return 0;
        }
        BackendState::Stopped => {}
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
    // Health can go green off a *different* backend, so a green probe alone is
    // not proof OUR child came up. Require both: health responds AND the pid we
    // spawned is still alive. If the child died (e.g. lost an EADDRINUSE race)
    // we must not claim success or leave its dead pid on file.
    if process::wait_for_health(&url, Duration::from_secs(5)) && process::pid_alive(pid) {
        out.success(&format!("backend started   pid {pid}   {url}"));
        0
    } else if process::pid_alive(pid) {
        // Spawned, still alive, but health never went green — leave it running
        // (may still be coming up) but tell the user to check logs.
        out.failure(&format!(
            "backend spawned (pid {pid}) but /health did not respond within 5s — check `ol logs`"
        ));
        1
    } else {
        // Child exited before serving. Clear the dead pid so `status`/`stop`
        // don't trust it, and surface the real reason.
        process::clear_pid(&process::backend_pid_file());
        out.failure(
            "backend failed to start — the process exited (port already in use? check `ol logs`)",
        );
        1
    }
}

/// `ol stop` — stop the backend and clear its PID file. Reconciles against
/// reality: if the backend we track isn't the one serving the port (untracked
/// backend from `ol serve` or a prior session), reclaim the port by stopping
/// its real owner too, so `stop` actually frees the port the user sees.
pub fn stop(out: &Output) -> i32 {
    let pid_file = process::backend_pid_file();
    match backend_state() {
        BackendState::Tracked { pid, .. } => {
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
        BackendState::Untracked { pid: Some(pid), .. } => {
            // Serving but not tracked — reclaim the port by stopping the real
            // owner we resolved from the port table.
            let stopped = process::stop_pid(pid, Duration::from_secs(3));
            process::clear_pid(&pid_file);
            if stopped {
                out.success(&format!("backend stopped   pid {pid}   {}", out.dim("(untracked)")));
                0
            } else {
                out.failure(&format!("failed to stop backend   pid {pid}"));
                1
            }
        }
        BackendState::Untracked { pid: None, .. } => {
            // Something serves the port but we can't identify the owner (e.g.
            // non-Linux where port_pid returns None). Don't guess-kill.
            process::clear_pid(&pid_file);
            out.failure(&format!(
                "backend is serving on port {} but its process could not be identified to stop it",
                server_port()
            ));
            1
        }
        BackendState::Stopped => {
            process::clear_pid(&pid_file);
            out.failure("backend not running");
            1
        }
    }
}

/// `ol restart` — reclaim the port (stop whatever serves it, tracked or not)
/// then start a fresh, tracked backend. Because `stop` now reconciles against
/// reality, a restart run against an untracked backend takes it over cleanly
/// instead of spawning a doomed duplicate.
pub fn restart(out: &Output, debug: bool) -> i32 {
    // Best-effort stop: a "not running" stop is fine before a start. When a
    // backend was serving, wait for the port to actually free so the fresh
    // `serve` child doesn't lose an EADDRINUSE race against the one we just
    // signalled.
    let was_running = backend_state().is_running();
    let _ = stop(out);
    if was_running {
        wait_for_port_free("127.0.0.1", server_port(), Duration::from_secs(3));
    }
    start(out, debug)
}

/// Poll until `host:port` stops accepting connections or `timeout` elapses.
/// Returns true if the port is free. Used by `restart` to avoid racing the
/// just-stopped listener's socket teardown.
fn wait_for_port_free(host: &str, port: u16, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if !process::port_listening(host, port) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// `ol stop --gui` — stop a detached GUI shell (started via `ol gui --detached`)
/// by its PID file, and clear the file. This ports the last capability that only
/// lived in the shell wrappers (`scripts/ops.*` stop-frontend) into the binary.
pub fn stop_gui(out: &Output) -> i32 {
    let pid_file = process::gui_pid_file();
    let Some(pid) = process::read_pid(&pid_file) else {
        out.failure("gui not running (no detached shell tracked)");
        return 1;
    };
    if !process::pid_alive(pid) {
        process::clear_pid(&pid_file);
        out.failure("gui not running");
        return 1;
    }

    let stopped = process::stop_pid(pid, Duration::from_secs(3));
    process::clear_pid(&pid_file);
    if stopped {
        out.success(&format!("gui stopped   pid {pid}"));
        0
    } else {
        out.failure(&format!("failed to stop gui   pid {pid}"));
        1
    }
}

/// `ol stop --all` — stop both the detached GUI shell and the backend. Reports
/// success if at least one was running and stopped; a fully-idle system is not
/// an error (nothing to do).
pub fn stop_all(out: &Output) -> i32 {
    // Run both regardless of individual outcome so one failure doesn't skip the
    // other. `stop`/`stop_gui` return 1 when their target wasn't running, which
    // is fine here — "stop everything" on an idle system is a no-op, not a
    // failure.
    let gui_running = process::read_pid(&process::gui_pid_file())
        .map(process::pid_alive)
        .unwrap_or(false);
    let backend_running = backend_state().is_running();

    if !gui_running && !backend_running {
        out.info("nothing running");
        return 0;
    }

    let mut ok = true;
    if gui_running {
        ok &= stop_gui(out) == 0;
    }
    if backend_running {
        ok &= stop(out) == 0;
    }
    if ok {
        0
    } else {
        1
    }
}

/// `ol health` — probe the backend `/health` endpoint at the configured port and
/// exit 0 if healthy, 1 otherwise. Unlike `status` (which reports on the managed
/// PID file), this checks actual HTTP health, so it also works for backends we
/// didn't spawn (wsl / remote). Ports the scripts' `test-backend` into the binary.
pub fn health(out: &Output) -> i32 {
    let url = backend_url();
    match process::probe_health(&url) {
        process::Health::Ok => {
            out.success(&format!("healthy   {url}"));
            0
        }
        process::Health::Bad => {
            out.failure(&format!("unhealthy — endpoint returned an error   {url}"));
            1
        }
        process::Health::Unreachable => {
            out.failure(&format!("unreachable   {url}"));
            1
        }
    }
}

/// `ol status` — rich health/process/port/binary view.
pub fn status(out: &Output) -> i32 {
    let version = env!("CARGO_PKG_VERSION");
    let port = server_port();
    let url = backend_url();

    // Reconciled backend truth (PID file + port + health), so the backend line
    // can never contradict the port/health lines below.
    let state = backend_state();
    let backend_up = state.is_running();
    let mem = state.pid().and_then(process::pid_memory_bytes);

    // Port + health (probed independently for the dedicated lines).
    let port_up = process::port_listening("127.0.0.1", port);
    let health = process::probe_health(&url);
    let health_up = health == process::Health::Ok;

    // GUI (detached only — a foreground GUI has no PID file)
    let gui_pid = process::read_pid(&process::gui_pid_file()).filter(|&p| process::pid_alive(p));

    if out.json {
        let tracked = matches!(state, BackendState::Tracked { .. });
        let payload = serde_json::json!({
            "version": version,
            "backend": {
                "running": backend_up,
                "tracked": tracked,
                "pid": state.pid(),
                "memory_bytes": mem,
            },
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

    let backend_line = match &state {
        BackendState::Tracked { pid, .. } => match mem {
            Some(m) => format!(
                "{} running   pid {pid}   {}",
                out.glyph(Status::Up),
                human_bytes(m)
            ),
            None => format!("{} running   pid {pid}", out.glyph(Status::Up)),
        },
        BackendState::Untracked { pid, .. } => {
            let who = pid
                .map(|p| format!("pid {p}"))
                .unwrap_or_else(|| "pid unknown".to_string());
            let mem_str = mem.map(|m| format!("   {}", human_bytes(m))).unwrap_or_default();
            format!(
                "{} running   {who}{mem_str}   {}",
                out.glyph(Status::Up),
                out.dim("untracked")
            )
        }
        BackendState::Stopped => format!("{} stopped", out.glyph(Status::Down)),
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

    // 5. backend running? Reconciled against reality so `doctor` agrees with
    // `status` — an untracked-but-serving backend counts as running.
    match backend_state() {
        BackendState::Tracked { .. } => line(out, DoctorState::Ok, "backend", "running"),
        BackendState::Untracked { .. } => line(
            out,
            DoctorState::Ok,
            "backend",
            "running (untracked — not started by ol)",
        ),
        BackendState::Stopped => line(
            out,
            DoctorState::Warn,
            "backend",
            "not running (`ol start`)",
        ),
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

    #[test]
    fn backend_state_accessors_reflect_variant() {
        // Stopped: not running, no pid.
        let s = BackendState::Stopped;
        assert!(!s.is_running());
        assert_eq!(s.pid(), None);

        // Tracked: running, pid is the tracked pid.
        let t = BackendState::Tracked { pid: 4242, healthy: true };
        assert!(t.is_running());
        assert_eq!(t.pid(), Some(4242));

        // Untracked with a resolved owner: running, pid is the owner.
        let u = BackendState::Untracked { pid: Some(99), healthy: false };
        assert!(u.is_running());
        assert_eq!(u.pid(), Some(99));

        // Untracked with an unresolved owner (e.g. non-Linux): still running,
        // but pid is unknown — callers must NOT treat None here as "stopped".
        let u_unknown = BackendState::Untracked { pid: None, healthy: true };
        assert!(u_unknown.is_running());
        assert_eq!(u_unknown.pid(), None);
    }

    #[test]
    fn backend_state_is_stopped_when_nothing_listens() {
        // Hermetic reconciliation check: with the PID file redirected to a fresh
        // temp dir (so no real user state is touched) and the port pointed at a
        // dead port, `backend_state` must resolve to `Stopped`. This is the exact
        // inverse of the reported bug ("stopped" shown while something served).
        //
        // Both env vars are process-global, so serialize via a binary-crate-local
        // lock. (The lib's CONFIG_DIR_ENV_LOCK is `#[cfg(test)]` and thus not
        // visible to the binary crate's test build.)
        use std::sync::Mutex;
        static ENV_LOCK: Mutex<()> = Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let prev_dir = std::env::var_os("OMNILAUNCHER_CONFIG_DIR");
        let prev_port = std::env::var_os("OMNILAUNCHER_SERVER_PORT");
        let tmp = std::env::temp_dir().join(format!("ol-statetest-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();

        // SAFETY: guarded by ENV_LOCK; restored below before returning.
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", &tmp);
        std::env::set_var("OMNILAUNCHER_SERVER_PORT", "1"); // privileged, never served

        let state = backend_state();

        // Restore env before asserting so a panic can't leak overrides.
        match prev_dir {
            Some(v) => std::env::set_var("OMNILAUNCHER_CONFIG_DIR", v),
            None => std::env::remove_var("OMNILAUNCHER_CONFIG_DIR"),
        }
        match prev_port {
            Some(v) => std::env::set_var("OMNILAUNCHER_SERVER_PORT", v),
            None => std::env::remove_var("OMNILAUNCHER_SERVER_PORT"),
        }
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(
            state,
            BackendState::Stopped,
            "a port with no listener must reconcile to Stopped"
        );
    }
}
