// Runtime packaging facts shared by clone-path validation and the frontend. Packaged
// Flatpak and macOS app-bundle installs should not clone into their executable folder.
use crate::models::AppRuntimeInfo;
use crate::paths::{display_path, resolve_scan_root};
use std::path::Path;

#[tauri::command]
pub fn app_runtime_info() -> Result<AppRuntimeInfo, String> {
    let default_scan_root = resolve_scan_root(None)?;
    let flatpak = is_flatpak_runtime();
    let packaged_macos = is_packaged_macos();
    let default_clone_requires_scan_path = default_clone_requires_scan_path();

    Ok(AppRuntimeInfo {
        os: std::env::consts::OS.to_string(),
        default_scan_root: display_path(&default_scan_root),
        flatpak,
        packaged_macos,
        default_clone_requires_scan_path,
        default_clone_warning: default_clone_warning(default_clone_requires_scan_path),
    })
}

#[tauri::command]
pub fn launcher_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub(crate) fn ensure_clone_scan_root(scan_root: &Option<String>) -> Result<(), String> {
    if scan_root.is_none() && default_clone_requires_scan_path() {
        return Err(default_clone_warning(true).unwrap_or_else(|| {
            "Choose a repo scan path before cloning from this packaged launcher.".to_string()
        }));
    }

    Ok(())
}

fn default_clone_requires_scan_path() -> bool {
    is_flatpak_runtime() || is_packaged_macos()
}

fn default_clone_warning(required: bool) -> Option<String> {
    required.then(|| {
        "Flatpak and macOS DMG/app-bundle releases cannot clone into the default app location. \
Add a repo scan path, select it as the clone destination, then clone."
            .to_string()
    })
}

#[cfg(target_os = "linux")]
pub(crate) fn is_flatpak_runtime() -> bool {
    Path::new("/.flatpak-info").is_file()
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn is_flatpak_runtime() -> bool {
    false
}

#[cfg(target_os = "macos")]
fn is_packaged_macos() -> bool {
    !cfg!(debug_assertions)
}

#[cfg(not(target_os = "macos"))]
fn is_packaged_macos() -> bool {
    false
}
