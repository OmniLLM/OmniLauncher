//! Plugin runtime-dependency inventory + installation, shared by the Tauri
//! binary (`main.rs`) and the split backend (`split_server.rs`).
//!
//! This logic previously lived in `main.rs`. It was lifted into the library so
//! the split backend (which lives in the library, not the binary) can serve the
//! `/api/plugins/runtime-deps*` endpoints with the exact same behavior.

use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct PluginRuntimeDependency {
    pub id: &'static str,
    pub label: &'static str,
    pub installed: bool,
    pub installable: bool,
    pub install_command: Option<String>,
    pub detail: String,
}

/// True if `cmd` resolves to an executable on PATH (respecting PATHEXT on
/// Windows). Pure environment inspection — no process is spawned.
pub fn command_exists(cmd: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![String::new()]
    };

    for dir in std::env::split_paths(&path_var) {
        for ext in &exts {
            if dir.join(format!("{cmd}{ext}")).is_file() {
                return true;
            }
        }
    }
    false
}

pub fn runtime_label(id: &str) -> &'static str {
    match id {
        "python" => "Python",
        "node" => "Node.js/npm",
        "dotnet" => ".NET SDK",
        _ => "runtime dependency",
    }
}

/// Inventory the plugin runtime dependencies and whether each is installed /
/// installable on this platform.
pub fn list_runtime_dependencies() -> Vec<PluginRuntimeDependency> {
    let python_installed = crate::python_installer::bundled_python_exe().is_some()
        || command_exists(if cfg!(windows) { "python" } else { "python3" });
    let npm_installed = command_exists("npm");
    let dotnet_installed = command_exists("dotnet");

    let mut deps = vec![
        PluginRuntimeDependency {
            id: "python",
            label: "Python",
            installed: python_installed,
            installable: true,
            install_command: None,
            detail: if python_installed {
                "Ready for Flow Python plugins.".to_string()
            } else {
                "Required by Flow Python plugins with requirements.txt.".to_string()
            },
        },
        PluginRuntimeDependency {
            id: "node",
            label: "Node.js/npm",
            installed: npm_installed,
            installable: runtime_install_plan("node").is_ok(),
            install_command: runtime_manual_command("node"),
            detail: if npm_installed {
                "Ready for Flow JavaScript/TypeScript and Raycast plugins.".to_string()
            } else {
                "Required by Flow JavaScript/TypeScript and Raycast plugins.".to_string()
            },
        },
    ];

    if cfg!(windows) {
        deps.push(PluginRuntimeDependency {
            id: "dotnet",
            label: ".NET SDK",
            installed: dotnet_installed,
            installable: runtime_install_plan("dotnet").is_ok(),
            install_command: runtime_manual_command("dotnet"),
            detail: if dotnet_installed {
                "Ready for Flow C#/F# plugins.".to_string()
            } else {
                "Required by Flow C#/F# plugins on Windows.".to_string()
            },
        });
    }

    deps
}

/// Build the (program, args, display-command) tuple for the platform-native
/// installer of `id` ("node" / "dotnet"). Returns `Err` when no automatic
/// installer is available on this platform.
pub fn runtime_install_plan(id: &str) -> Result<(String, Vec<String>, String), String> {
    #[cfg(windows)]
    {
        if !command_exists("winget") {
            return Err("winget is not available.".to_string());
        }
        let package = match id {
            "node" => "OpenJS.NodeJS.LTS",
            "dotnet" => "Microsoft.DotNet.SDK.8",
            _ => return Err(format!("No installer for {id}.")),
        };
        let args = vec![
            "install".to_string(),
            "--id".to_string(),
            package.to_string(),
            "--exact".to_string(),
            "--accept-source-agreements".to_string(),
            "--accept-package-agreements".to_string(),
        ];
        return Ok((
            "winget".to_string(),
            args,
            format!("winget install --id {package} --exact"),
        ));
    }

    #[cfg(target_os = "macos")]
    {
        if !command_exists("brew") {
            return Err("Homebrew is not available.".to_string());
        }
        let args = match id {
            "node" => vec!["install".to_string(), "node".to_string()],
            "dotnet" => vec![
                "install".to_string(),
                "--cask".to_string(),
                "dotnet-sdk".to_string(),
            ],
            _ => return Err(format!("No installer for {id}.")),
        };
        return Ok((
            "brew".to_string(),
            args.clone(),
            format!("brew {}", args.join(" ")),
        ));
    }

    #[cfg(target_os = "linux")]
    {
        let command = runtime_manual_command(id)
            .ok_or_else(|| format!("No installer for {}.", runtime_label(id)))?;
        return Err(format!(
            "Automatic {} install is not available on Linux because it may require administrator authentication. Run: {command}",
            runtime_label(id)
        ));
    }

    #[allow(unreachable_code)]
    Err(format!(
        "No installer for {} on this platform.",
        runtime_label(id)
    ))
}

pub fn runtime_manual_command(id: &str) -> Option<String> {
    #[cfg(windows)]
    {
        return match id {
            "node" => Some("winget install --id OpenJS.NodeJS.LTS --exact".to_string()),
            "dotnet" => Some("winget install --id Microsoft.DotNet.SDK.8 --exact".to_string()),
            _ => None,
        };
    }

    #[cfg(target_os = "macos")]
    {
        return match id {
            "node" => Some("brew install node".to_string()),
            "dotnet" => Some("brew install --cask dotnet-sdk".to_string()),
            _ => None,
        };
    }

    #[cfg(target_os = "linux")]
    {
        if command_exists("apt-get") {
            return match id {
                "node" => {
                    Some("sudo apt-get update && sudo apt-get install -y nodejs npm".to_string())
                }
                "dotnet" => Some(
                    "sudo apt-get update && sudo apt-get install -y dotnet-sdk-8.0".to_string(),
                ),
                _ => None,
            };
        }
        if command_exists("dnf") {
            return match id {
                "node" => Some("sudo dnf install -y nodejs npm".to_string()),
                "dotnet" => Some("sudo dnf install -y dotnet-sdk-8.0".to_string()),
                _ => None,
            };
        }
        if command_exists("pacman") {
            return match id {
                "node" => Some("sudo pacman -S --needed nodejs npm".to_string()),
                "dotnet" => Some("sudo pacman -S --needed dotnet-sdk".to_string()),
                _ => None,
            };
        }
    }

    #[allow(unreachable_code)]
    None
}
