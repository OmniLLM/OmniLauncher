import io, sys
p = r"C:\Users\jzhu\repos\OmniLauncher\src-tauri\src\plugins\scheduler.rs"
s = open(p, 'r', encoding='utf-8').read()
start_marker = "/// Execute either a stored job script (when "
i = s.index(start_marker)
j = s.index("\n/// Dry-run a scheduled-job command", i)
# keep up to start of line containing marker - find line start
line_start = s.rfind("\n", 0, i) + 1
prefix = s[:line_start]
suffix = s[j+1:]  # from "/// Dry-run..."
replacement = '''/// Execute either a stored job script (when `cmd` is a `job_<id>.<ext>`
/// filename living under `~/.omnilauncher/scheduler/scripts/`) or an
/// ad-hoc body (used by validation). Returns the captured
/// `std::process::Output`.
///
/// Filename references are resolved against the scripts dir directly so the
/// scheduler never re-writes the on-disk script at run time — the file is
/// the source of truth and the user can hand-edit it. Inline bodies are
/// materialised as `validate_<ts>.<ext>` and deleted after the run.
async fn spawn_command(cmd: &str, tag: i64) -> std::io::Result<std::process::Output> {
    let dir = scripts_dir();

    let (path, is_ephemeral, executor) = if looks_like_script_filename(cmd) {
        let path = dir.join(cmd.trim());
        if !path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("script file missing: {}", path.display()),
            ));
        }
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        let executor = detect_explicit_executor(&body)
            .or_else(|| {
                path.extension().and_then(|e| e.to_str()).and_then(|e| match e {
                    "py"  => Some(Executor::Python),
                    "ps1" => Some(Executor::PowerShell),
                    "sh"  => Some(Executor::Sh),
                    _ => None,
                })
            })
            .unwrap_or_else(pick_executor);
        (path, false, executor)
    } else {
        let executor = executor_for(cmd);
        let (ext, file_body) = match executor {
            Executor::Python     => ("py",  cmd.to_string()),
            Executor::PowerShell => ("ps1", cmd.to_string()),
            Executor::Sh         => ("sh",  format!("#!/bin/sh\\n{}\\n", cmd)),
        };
        let p = if tag > 0 {
            dir.join(format!("job_{}.{}", tag, ext))
        } else {
            dir.join(format!("validate_{}.{}", now_unix(), ext))
        };
        std::fs::write(&p, file_body)?;
        (p, tag <= 0, executor)
    };

    let path_str = path.to_string_lossy().to_string();
    let result = match executor {
        Executor::Python => {
            let prefix = python_bin().clone().unwrap_or_else(|| "python".into());
            let mut parts = prefix.split_whitespace();
            let bin = parts.next().unwrap_or("python");
            let mut command = tokio::process::Command::new(bin);
            for arg in parts {
                command.arg(arg);
            }
            command.arg(&path_str).output().await
        }
        Executor::PowerShell => {
            tokio::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    &path_str,
                ])
                .output()
                .await
        }
        Executor::Sh => {
            tokio::process::Command::new("sh")
                .arg(&path_str)
                .output()
                .await
        }
    };

    if is_ephemeral {
        let _ = std::fs::remove_file(&path);
    }
    result
}

'''
out = prefix + replacement + suffix
open(p, 'w', encoding='utf-8', newline='\n').write(out)
lines = out.split('\n')
print("LINE_COUNT", len(lines))
for n in range(554, min(650, len(lines))):
    print(f"{n+1}: {lines[n]}")
