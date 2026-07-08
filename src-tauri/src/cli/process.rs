//! Process & runtime-state helpers for the `ol` CLI lifecycle commands.
//!
//! Everything that touches PID files, detached child spawning, port probing,
//! and the HTTP `/health` check lives here so the OS-specific bits are in one
//! place and can be unit-tested. State lives under `~/.omnilauncher/run/` (via
//! `path_config::data_dir()`), so `ol` works from any directory now that it is
//! on `PATH` — the old repo-local `.run/` is no longer used.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Directory holding PID files and the REPL history: `~/.omnilauncher/run/`.
pub fn run_dir() -> PathBuf {
    let dir = omnilauncher_lib::path_config::data_dir().join("run");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// PID file for the detached backend started by `ol start`.
pub fn backend_pid_file() -> PathBuf {
    run_dir().join("omnilauncher-backend.pid")
}

/// PID file for a detached GUI started by `ol gui --detached`.
pub fn gui_pid_file() -> PathBuf {
    run_dir().join("omnilauncher-gui.pid")
}

/// Persistent REPL history file: `~/.omnilauncher/repl_history`.
pub fn repl_history_file() -> PathBuf {
    omnilauncher_lib::path_config::data_dir().join("repl_history")
}

/// Write a PID to `path`.
pub fn write_pid(path: &PathBuf, pid: u32) -> std::io::Result<()> {
    std::fs::write(path, pid.to_string())
}

/// Read a PID from `path`, if present and parseable.
pub fn read_pid(path: &PathBuf) -> Option<u32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
}

/// Remove a PID file, ignoring a missing file.
pub fn clear_pid(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
}

/// Whether a process with `pid` is currently alive (cross-platform via sysinfo).
pub fn pid_alive(pid: u32) -> bool {
    use sysinfo::{Pid, System};
    let mut sys = System::new();
    let p = Pid::from_u32(pid);
    sys.refresh_process(p);
    sys.process(p).is_some()
}

/// Resident memory of `pid` in bytes, if the process exists.
pub fn pid_memory_bytes(pid: u32) -> Option<u64> {
    use sysinfo::{Pid, System};
    let mut sys = System::new();
    let p = Pid::from_u32(pid);
    sys.refresh_process(p);
    sys.process(p).map(|proc| proc.memory())
}

/// Best-effort terminate of `pid`: SIGTERM (or platform equivalent), wait up to
/// `grace`, then SIGKILL. Returns true if the process is gone afterward.
pub fn stop_pid(pid: u32, grace: Duration) -> bool {
    use sysinfo::{Pid, Signal, System};
    let mut sys = System::new();
    let p = Pid::from_u32(pid);
    sys.refresh_process(p);
    let Some(proc) = sys.process(p) else {
        return true; // already gone
    };

    // Graceful first. `kill_with(Term)` returns None if the signal is
    // unsupported (e.g. Windows) — fall back to the forceful `kill()`.
    if proc.kill_with(Signal::Term).is_none() {
        proc.kill();
    }

    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if !pid_alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // Still alive → force kill.
    let mut sys = System::new();
    sys.refresh_process(p);
    if let Some(proc) = sys.process(p) {
        proc.kill();
    }
    std::thread::sleep(Duration::from_millis(150));
    !pid_alive(pid)
}

/// Whether `host:port` currently accepts a TCP connection (i.e. something is
/// listening). Uses a short connect timeout so a dead port fails fast.
pub fn port_listening(host: &str, port: u16) -> bool {
    // "0.0.0.0" is a bind address, not connectable — probe loopback instead.
    let connect_host = if host == "0.0.0.0" { "127.0.0.1" } else { host };
    let addr_iter = match (connect_host, port).to_socket_addrs() {
        Ok(it) => it,
        Err(_) => return false,
    };
    for addr in addr_iter {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(400)).is_ok() {
            return true;
        }
    }
    false
}

/// Result of an HTTP `/health` probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    /// 2xx response received.
    Ok,
    /// Connected but the endpoint did not return success.
    Bad,
    /// Could not connect at all.
    Unreachable,
}

/// Probe `GET {base_url}/health` with a raw HTTP/1.0 request over a plain TCP
/// socket. Avoids pulling a blocking HTTP client into the CLI path; the backend
/// speaks plain HTTP on loopback. Only `http://` URLs are supported (the local
/// backend is never HTTPS).
pub fn probe_health(base_url: &str) -> Health {
    let trimmed = base_url.trim_end_matches('/');
    let without_scheme = match trimmed.strip_prefix("http://") {
        Some(rest) => rest,
        None => match trimmed.strip_prefix("https://") {
            // We can't do TLS here; report unreachable rather than lie.
            Some(_) => return Health::Unreachable,
            None => trimmed,
        },
    };
    let (host, port) = match without_scheme.split_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().unwrap_or(80)),
        None => (without_scheme, 80),
    };
    let connect_host = if host == "0.0.0.0" { "127.0.0.1" } else { host };

    let addr = match (connect_host, port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut it| it.next())
    {
        Some(a) => a,
        None => return Health::Unreachable,
    };
    let mut stream = match TcpStream::connect_timeout(&addr, Duration::from_millis(600)) {
        Ok(s) => s,
        Err(_) => return Health::Unreachable,
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));

    let req = format!("GET /health HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    if stream.write_all(req.as_bytes()).is_err() {
        return Health::Bad;
    }
    let mut buf = String::new();
    if stream.read_to_string(&mut buf).is_err() && buf.is_empty() {
        return Health::Bad;
    }
    // Parse the status line: "HTTP/1.x <code> ...".
    let status_ok = buf
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .map(|code| (200..300).contains(&code))
        .unwrap_or(false);
    if status_ok {
        Health::Ok
    } else {
        Health::Bad
    }
}

/// Poll `probe_health` until it returns `Ok` or `timeout` elapses.
pub fn wait_for_health(base_url: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if probe_health(base_url) == Health::Ok {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Path to the currently-running executable, for re-spawning `self serve`.
pub fn current_exe_path() -> std::io::Result<PathBuf> {
    std::env::current_exe()
}

/// Spawn `exe <args...>` as a detached background process in its own process
/// group so it survives the parent shell. Returns the child PID. Used by
/// `ol start` (`serve`) and `ol gui --detached` (`gui`).
pub fn spawn_detached(exe: &PathBuf, args: &[String]) -> std::io::Result<u32> {
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Put the child in a fresh process group (leader) so it is not killed
        // when the parent shell exits or receives Ctrl-C. Stable since 1.64;
        // avoids a manual setsid() FFI.
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }

    let child = cmd.spawn()?;
    Ok(child.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_round_trip_through_file() {
        let dir = std::env::temp_dir().join(format!("ol-pidtest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.pid");
        write_pid(&path, 4242).unwrap();
        assert_eq!(read_pid(&path), Some(4242));
        clear_pid(&path);
        assert_eq!(read_pid(&path), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn current_process_is_alive() {
        assert!(pid_alive(std::process::id()));
    }

    #[test]
    fn absent_port_is_not_listening() {
        // Port 1 is privileged and essentially never listening in test envs.
        assert!(!port_listening("127.0.0.1", 1));
    }

    #[test]
    fn health_probe_on_dead_port_is_unreachable() {
        assert_eq!(probe_health("http://127.0.0.1:1"), Health::Unreachable);
    }

    #[test]
    fn https_health_probe_is_unreachable_no_tls() {
        assert_eq!(probe_health("https://127.0.0.1:1422"), Health::Unreachable);
    }
}
