// This module scans folders beside the launcher and classifies Z3R projects by
// runtime readiness.
use crate::makefile_patches::{
    has_snesrev_makefile_patch, has_snesrev_solution_patch, is_snesrev_zelda3_project,
};
use crate::models::{AppScan, ProjectCandidate, ProjectScanGroup};
use crate::paths::{display_path, resolve_scan_root};
use std::fs;
use std::path::{Path, PathBuf};

// Scans the default root plus any user-added roots and groups results by source path.
// The flat candidates list is retained for existing selected-project logic, while
// groups drive the home-screen section display in the same order the frontend sends.
#[tauri::command]
pub fn scan_siblings(scan_roots: Option<Vec<String>>) -> Result<AppScan, String> {
    let default_root = resolve_scan_root(None)?;
    let roots = ordered_scan_roots(&default_root, scan_roots.unwrap_or_default())?;
    let mut groups = Vec::new();
    let mut candidates = Vec::new();

    for (index, root) in roots.iter().enumerate() {
        let group_candidates = scan_root(root)?;
        candidates.extend(group_candidates.iter().cloned());
        groups.push(ProjectScanGroup {
            label: scan_root_label(root),
            path: display_path(root),
            is_default: index == 0,
            candidates: group_candidates,
        });
    }

    Ok(AppScan {
        launcher_parent: display_path(&default_root),
        candidates,
        groups,
    })
}

// Builds the scan root list with the launcher default first and user-added paths after it.
fn ordered_scan_roots(
    default_root: &Path,
    added_roots: Vec<String>,
) -> Result<Vec<PathBuf>, String> {
    let mut roots = vec![default_root.to_path_buf()];

    for root in added_roots {
        let path = PathBuf::from(root);

        if !roots.iter().any(|existing| existing == &path) {
            roots.push(path);
        }
    }

    Ok(roots)
}

// Uses only the direct folder name as the group label, with a path fallback for root paths.
fn scan_root_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| display_path(path))
}

// Scans one root and reports whether each child looks ready to launch or build.
// Direct children are inspected first; any direct child that is not itself a project is
// descended into exactly one level to support the nested {parent}/{owner}/{repo} layout
// produced by clone_custom_project for multi-fork installs.
fn scan_root(parent: &Path) -> Result<Vec<ProjectCandidate>, String> {
    let mut candidates = Vec::new();

    // Missing pasted paths should not break the whole launcher; they simply scan empty.
    if !parent.is_dir() {
        return Ok(candidates);
    }

    for entry in fs::read_dir(parent)
        .map_err(|error| format!("Could not scan {}: {error}", display_path(parent)))?
    {
        let entry =
            entry.map_err(|error| format!("Could not read a sibling folder entry: {error}"))?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        // First try the top-level layout. The canonical Z3R clone and any hand-placed
        // flat folder end here with owner = None.
        if let Some(candidate) = inspect_candidate(&path, None) {
            candidates.push(candidate);
            continue;
        }

        // Direct child is not a project on its own. Treat it as a candidate owner folder
        // and look one level deeper for the nested {owner}/{repo} layout. Recursion is
        // bounded to this one extra level so pointing the scan root at a deep tree cannot
        // explode the scan.
        scan_owner_folder(&path, &mut candidates);
    }

    candidates.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(candidates)
}

// Treats a non-project direct child of the scan root as a potential owner folder and
// inspects its immediate subfolders for the {owner}/{repo} custom-clone layout.
// Errors reading the folder are swallowed silently because a missing or permission-denied
// folder simply means there is nothing here to discover, not a top-level scan failure.
fn scan_owner_folder(owner_path: &Path, candidates: &mut Vec<crate::models::ProjectCandidate>) {
    // Skip dotfile owners like `.git`, `.idea`, `.cache`, etc. so internal tool folders
    // never claim to be GitHub owners and litter the cards screen.
    let owner_name = match owner_path.file_name().and_then(|name| name.to_str()) {
        Some(name) if !name.starts_with('.') => name.to_string(),
        _ => return,
    };

    let Ok(entries) = fs::read_dir(owner_path) else {
        return;
    };

    for nested_entry in entries.flatten() {
        let nested_path = nested_entry.path();
        if !nested_path.is_dir() {
            continue;
        }

        if let Some(candidate) = inspect_candidate(&nested_path, Some(owner_name.clone())) {
            candidates.push(candidate);
        }
    }
}

// Builds a candidate summary when a folder contains source, assets, or an executable.
// owner is supplied for nested {owner}/{repo} discoveries and left None for top-level
// folders so the frontend only renders the author line on actual nested clones.
fn inspect_candidate(path: &Path, owner: Option<String>) -> Option<ProjectCandidate> {
    let asset_path = find_asset(path);
    let executable_path = find_executable(path);
    let has_makefile = path.join("Makefile").exists();
    let has_solution = path.join("Zelda3.sln").exists();
    let has_source = has_makefile || has_solution || path.join("run_with_tcc.bat").exists();
    let git_repo = path.join(".git").exists();

    if asset_path.is_none() && executable_path.is_none() && !has_source {
        return None;
    }

    let mut notes = Vec::new();
    let status = match (&asset_path, &executable_path) {
        (Some(asset), Some(executable)) => {
            if executable.parent() == asset.parent() || is_windows_runtime_output(executable) {
                "ready".to_string()
            } else {
                notes.push(
                    concat!(
                        "Executable and zelda3_assets.dat are not beside each other; ",
                        "use a deploy build or copy assets beside the executable."
                    )
                    .to_string(),
                );
                "needs-deploy-copy".to_string()
            }
        }
        (Some(_), None) => "assets-ready".to_string(),
        (None, Some(_)) => "missing-assets".to_string(),
        (None, None) => "source-only".to_string(),
    };

    let is_snesrev_zelda3 = is_discovered_snesrev_zelda3(path, owner.as_deref());
    let snesrev_makefile_patch_applied = is_snesrev_zelda3 && has_snesrev_makefile_patch(path);
    let snesrev_solution_patch_applied =
        is_snesrev_zelda3 && has_solution && has_snesrev_solution_patch(path);
    let source_patch_needed = source_patch_for_platform(
        is_snesrev_zelda3,
        has_solution,
        snesrev_makefile_patch_applied,
        snesrev_solution_patch_applied,
    );

    Some(ProjectCandidate {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| display_path(path)),
        owner,
        path: display_path(path),
        asset_path: asset_path.as_deref().map(display_path),
        executable_path: executable_path.as_deref().map(display_path),
        git_repo,
        snesrev_makefile_patch_applied,
        snesrev_solution_patch_applied,
        source_patch_needed,
        status,
        notes,
    })
}

// Keeps non-Windows Makefile patch visibility on the existing nested-owner signal,
// while Windows can also recognize a direct scan of the snesrev folder for SLN builds.
fn is_discovered_snesrev_zelda3(path: &Path, owner: Option<&str>) -> bool {
    if cfg!(target_os = "windows") {
        return is_snesrev_zelda3_project(path, owner);
    }

    owner.is_some_and(|owner| owner.eq_ignore_ascii_case("snesrev"))
        && path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("zelda3"))
}

// Chooses the one source-file patch that matters on the current launcher platform.
fn source_patch_for_platform(
    is_snesrev_zelda3: bool,
    has_solution: bool,
    makefile_patch_applied: bool,
    solution_patch_applied: bool,
) -> Option<String> {
    if cfg!(target_os = "windows") {
        return (is_snesrev_zelda3 && has_solution && !solution_patch_applied)
            .then(|| "solution".to_string());
    }

    (is_snesrev_zelda3 && !makefile_patch_applied).then(|| "makefile".to_string())
}

// Searches the project root and common deploy folders for the game asset bundle.
fn find_asset(project_path: &Path) -> Option<PathBuf> {
    let direct_candidates = [
        project_path.join("zelda3_assets.dat"),
        project_path.join("tables").join("zelda3_assets.dat"),
        project_path
            .join("bin")
            .join("x64-Release")
            .join("zelda3_assets.dat"),
        project_path
            .join("bin")
            .join("x64-ReleaseDeploy")
            .join("zelda3_assets.dat"),
        project_path
            .join("bin")
            .join("Win32-Release")
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

// Windows MSBuild emits zelda3.exe and SDL2.dll together in bin/*-Release while
// zelda3_assets.dat may remain in the project root or tables folder.
fn is_windows_runtime_output(executable: &Path) -> bool {
    if !cfg!(target_os = "windows") {
        return false;
    }

    executable
        .parent()
        .is_some_and(|folder| folder.join("SDL2.dll").is_file())
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
