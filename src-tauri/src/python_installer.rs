//! Self-contained Python installer for OmniLauncher.
//!
//! Downloads a python-build-standalone release from GitHub and extracts it
//! to `~/.omnilauncher/python/`. No external tools (uv, pip, brew, etc.)
//! are required — only an internet connection.

use std::path::{Path, PathBuf};
use std::io::Write;

/// Return the path of the bundled Python executable if it exists.
pub fn bundled_python_exe() -> Option<PathBuf> {
    let p = bundled_python_dir().join(python_bin_rel());
    if p.exists() { Some(p) } else { None }
}

/// `~/.omnilauncher/python`
fn bundled_python_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".omnilauncher")
        .join("python")
}

/// Relative path of the python3 binary inside the install dir.
fn python_bin_rel() -> &'static str {
    if cfg!(windows) { "python.exe" } else { "bin/python3" }
}

/// Download & extract python-build-standalone into `~/.omnilauncher/python/`.
/// Returns the path to the python binary on success.
pub async fn install_bundled_python() -> Result<PathBuf, String> {
    let dest = bundled_python_dir();
    let exe = dest.join(python_bin_rel());
    if exe.exists() {
        return Ok(exe);
    }

    let url = resolve_download_url().await?;
    let archive = download_to_temp(&url).await?;

    std::fs::create_dir_all(&dest)
        .map_err(|e| format!("mkdir ~/.omnilauncher/python: {e}"))?;

    if url.ends_with(".zip") {
        extract_zip(&archive, &dest)?;
    } else {
        extract_tar_zst_or_gz(&archive, &dest)?;
    }

    // python-build-standalone extracts into a sub-dir like `python/`; flatten.
    flatten_single_subdir(&dest)?;

    // Make binary executable on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if exe.exists() {
            let mut perms = std::fs::metadata(&exe)
                .map_err(|e| e.to_string())?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&exe, perms).map_err(|e| e.to_string())?;
        }
    }

    if exe.exists() {
        Ok(exe)
    } else {
        Err(format!("extraction finished but {} not found", exe.display()))
    }
}

/// Pick the right asset URL from the latest python-build-standalone release.
async fn resolve_download_url() -> Result<String, String> {
    // python-build-standalone release tag and known-good asset pattern.
    // We pin a recent tag; bump periodically for security updates.
    const RELEASE_TAG: &str = "20250317";
    const PYTHON_VER: &str = "3.12.9";

    let asset_name = asset_name_for_platform(PYTHON_VER);
    Ok(format!(
        "https://github.com/indygreg/python-build-standalone/releases/download/{RELEASE_TAG}/{asset_name}"
    ))
}

/// Construct the asset filename for the current platform.
fn asset_name_for_platform(py_ver: &str) -> String {
    // python-build-standalone naming:
    // cpython-{ver}+{tag}-{arch}-{os}-{abi}-install_only.tar.gz  (Linux/mac)
    // cpython-{ver}+{tag}-{arch}-pc-windows-msvc-install_only.zip (Windows)
    const TAG: &str = "20250317";

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return format!("cpython-{py_ver}+{TAG}-x86_64-unknown-linux-gnu-install_only.tar.gz");

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return format!("cpython-{py_ver}+{TAG}-aarch64-unknown-linux-gnu-install_only.tar.gz");

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return format!("cpython-{py_ver}+{TAG}-x86_64-apple-darwin-install_only.tar.gz");

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return format!("cpython-{py_ver}+{TAG}-aarch64-apple-darwin-install_only.tar.gz");

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return format!("cpython-{py_ver}+{TAG}-x86_64-pc-windows-msvc-install_only.zip");

    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    return format!("cpython-{py_ver}+{TAG}-aarch64-pc-windows-msvc-install_only.zip");

    // Fallback (shouldn't reach here for supported platforms)
    #[allow(unreachable_code)]
    {
        format!("cpython-{py_ver}+{TAG}-x86_64-unknown-linux-gnu-install_only.tar.gz")
    }
}

/// Download URL to a temp file, return path.
async fn download_to_temp(url: &str) -> Result<PathBuf, String> {
    let resp = reqwest::get(url)
        .await
        .map_err(|e| format!("download failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("download HTTP {}: {url}", resp.status()));
    }

    let ext = if url.ends_with(".zip") { "zip" } else { "tar.gz" };
    let tmp = std::env::temp_dir().join(format!("omnilauncher_python.{ext}"));
    let bytes = resp.bytes().await.map_err(|e| format!("read body: {e}"))?;
    let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    f.write_all(&bytes).map_err(|e| e.to_string())?;
    Ok(tmp)
}

fn extract_tar_zst_or_gz(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);
    tar.unpack(dest).map_err(|e| format!("tar unpack: {e}"))?;
    Ok(())
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("zip open: {e}"))?;
    zip.extract(dest).map_err(|e| format!("zip extract: {e}"))?;
    Ok(())
}

/// python-build-standalone "install_only" tarballs unpack to `python/` subdir.
/// Move contents up one level so `dest/bin/python3` works directly.
fn flatten_single_subdir(dest: &Path) -> Result<(), String> {
    let subdir = dest.join("python");
    if !subdir.exists() || !subdir.is_dir() {
        return Ok(()); // already flat or different layout
    }
    // Move all children of subdir/* → dest/*
    for entry in std::fs::read_dir(&subdir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = dest.join(entry.file_name());
        if !target.exists() {
            std::fs::rename(entry.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    let _ = std::fs::remove_dir(&subdir); // ignore if not empty (shouldn't happen)
    Ok(())
}

/// Tauri command: install bundled Python and return status string.
#[tauri::command]
pub async fn install_python_command() -> String {
    match install_bundled_python().await {
        Ok(exe) => format!("✅ Python installed: {}", exe.display()),
        Err(e) => format!("❌ Install failed: {e}"),
    }
}

/// Tauri command: check if bundled Python is installed.
#[tauri::command]
pub fn check_bundled_python() -> serde_json::Value {
    match bundled_python_exe() {
        Some(p) => serde_json::json!({
            "installed": true,
            "path": p.to_string_lossy()
        }),
        None => serde_json::json!({
            "installed": false,
            "path": null
        }),
    }
}
