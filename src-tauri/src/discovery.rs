// This module scans folders beside the launcher and classifies Z3R projects by
// runtime readiness.
use crate::models::{AppScan, ProjectCandidate};
use crate::paths::{display_path, resolve_scan_root};
use std::fs;
use std::path::{Path, PathBuf};

// Scans sibling folders and reports whether each one looks ready to launch or build.
#[tauri::command]
pub fn scan_siblings(scan_root: Option<String>) -> Result<AppScan, String> {
    let parent = resolve_scan_root(scan_root)?;
    let mut candidates = Vec::new();

    for entry in
        fs::read_dir(&parent).map_err(|error| format!("Could not scan launcher parent: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("Could not read a sibling folder entry: {error}"))?;
        let path = entry.path();

        if path.is_dir() {
            if let Some(candidate) = inspect_candidate(&path) {
                candidates.push(candidate);
            }
        }
    }

    candidates.sort_by(|left, right| left.name.cmp(&right.name));

    Ok(AppScan {
        launcher_parent: display_path(&parent),
        candidates,
    })
}

// Builds a candidate summary when a sibling folder contains source, assets, or an executable.
fn inspect_candidate(path: &Path) -> Option<ProjectCandidate> {
    let asset_path = find_asset(path);
    let executable_path = find_executable(path);
    let has_source = path.join("Makefile").exists()
        || path.join("Zelda3.sln").exists()
        || path.join("run_with_tcc.bat").exists();

    if asset_path.is_none() && executable_path.is_none() && !has_source {
        return None;
    }

    let mut notes = Vec::new();
    let status = match (&asset_path, &executable_path) {
        (Some(asset), Some(executable)) => {
            if executable.parent() == asset.parent() {
                "ready".to_string()
            } else {
                notes.push("Executable and zelda3_assets.dat are not beside each other; use a deploy build or copy assets beside the executable.".to_string());
                "needs-deploy-copy".to_string()
            }
        }
        (Some(_), None) => "assets-ready".to_string(),
        (None, Some(_)) => "missing-assets".to_string(),
        (None, None) => "source-only".to_string(),
    };

    Some(ProjectCandidate {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| display_path(path)),
        path: display_path(path),
        asset_path: asset_path.as_deref().map(display_path),
        executable_path: executable_path.as_deref().map(display_path),
        status,
        notes,
    })
}

// Searches the project root and common deploy folders for the game asset bundle.
fn find_asset(project_path: &Path) -> Option<PathBuf> {
    let direct_candidates = [
        project_path.join("zelda3_assets.dat"),
        project_path.join("tables").join("zelda3_assets.dat"),
        project_path
            .join("bin")
            .join("x64-ReleaseDeploy")
            .join("zelda3_assets.dat"),
        project_path
            .join("bin")
            .join("Win32-ReleaseDeploy")
            .join("zelda3_assets.dat"),
    ];

    for candidate in direct_candidates {
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

// Searches common output locations for the game executable on the current platform.
fn find_executable(project_path: &Path) -> Option<PathBuf> {
    let names = if cfg!(target_os = "windows") {
        vec!["zelda3.exe"]
    } else {
        vec!["zelda3"]
    };
    let folders = [
        project_path.to_path_buf(),
        project_path.join("bin").join("x64-Release"),
        project_path.join("bin").join("x64-ReleaseDeploy"),
        project_path.join("bin").join("Win32-Release"),
        project_path.join("bin").join("Win32-ReleaseDeploy"),
    ];

    for folder in folders {
        for name in &names {
            let candidate = folder.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}
