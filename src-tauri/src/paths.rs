// This module centralizes path handling so packaged app layout differences do not
// leak into scanning, setup, or launch code.
use std::path::{Path, PathBuf};

pub const Z3R_REPO_URL: &str = "https://github.com/xander-haj/Z3R";
pub const Z3R_BETA_REPO_URL: &str = "https://github.com/xander-haj/Z3R-Beta";

// Converts filesystem paths into display-safe strings for the JavaScript frontend.
pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

// Resolves either a user-chosen scan root or the launcher-adjacent default root.
pub fn resolve_scan_root(scan_root: Option<String>) -> Result<PathBuf, String> {
    if let Some(scan_root) = scan_root {
        let path = PathBuf::from(scan_root);

        if path.is_dir() {
            return Ok(path);
        }

        return Err(format!(
            "Selected scan folder does not exist: {}",
            display_path(&path)
        ));
    }

    default_scan_root()
}

// Finds the default directory that should be scanned for Z3R folders.
fn default_scan_root() -> Result<PathBuf, String> {
    if cfg!(debug_assertions) {
        return dev_scan_root();
    }

    packaged_scan_root()
}

// Resolves `Launcher/src-tauri` back to the folder containing `Launcher` during `cargo tauri dev`.
fn dev_scan_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let launcher_dir = manifest_dir
        .parent()
        .ok_or_else(|| "Cargo manifest directory does not have a launcher parent.".to_string())?;
    let launcher_parent = launcher_dir
        .parent()
        .ok_or_else(|| "Launcher folder does not have a parent directory.".to_string())?;

    Ok(launcher_parent.to_path_buf())
}

// Finds the real packaged app parent directory for release builds.
fn packaged_scan_root() -> Result<PathBuf, String> {
    let exe_path = std::env::current_exe()
        .map_err(|error| format!("Could not locate launcher executable: {error}"))?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| "Launcher executable does not have a parent directory.".to_string())?;

    if cfg!(target_os = "macos") {
        if let Some(bundle_parent) = macos_bundle_parent(exe_dir) {
            return Ok(bundle_parent);
        }
    }

    Ok(exe_dir.to_path_buf())
}

// Resolves `Launcher.app/Contents/MacOS` back to the folder that contains `Launcher.app`.
fn macos_bundle_parent(exe_dir: &Path) -> Option<PathBuf> {
    let contents_dir = exe_dir.parent()?;
    let app_dir = contents_dir.parent()?;
    let app_parent = app_dir.parent()?;
    let is_macos_dir = exe_dir.file_name().is_some_and(|name| name == "MacOS");
    let is_contents_dir = contents_dir
        .file_name()
        .is_some_and(|name| name == "Contents");
    let is_app_bundle = app_dir
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("app"));

    if is_macos_dir && is_contents_dir && is_app_bundle {
        Some(app_parent.to_path_buf())
    } else {
        None
    }
}

// Resolves the Python executable inside a virtual environment on the current platform.
pub fn venv_python(venv_path: &Path) -> Option<PathBuf> {
    let python = if cfg!(target_os = "windows") {
        venv_path.join("Scripts").join("python.exe")
    } else {
        venv_path.join("bin").join("python")
    };

    python.is_file().then_some(python)
}
