// Git update preview and selected-file application for cloned repos. The launcher fetches
// upstream first, shows the changed file list, then applies only the files the user kept checked.
use crate::bundled_tools::git_program;
use crate::command_env::platform_command_in_dir;
use crate::models::{ActionResult, RepoChange, RepoUpdatePreview};
use crate::paths::display_path;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

#[tauri::command]
pub fn preview_repo_update(
    app: tauri::AppHandle,
    project_path: String,
) -> Result<RepoUpdatePreview, String> {
    let project = repo_project_path(project_path)?;
    ensure_git_repo(&project)?;
    git_output(&app, &project, ["fetch", "--prune"])?;

    let upstream = upstream_ref(&app, &project)?;
    let behind_count = behind_count(&app, &project, &upstream)?;
    let changes = upstream_changes(&app, &project, &upstream)?;
    let dirty_files = dirty_files(&app, &project)?;
    let warnings = update_warnings(&changes, &dirty_files);

    Ok(RepoUpdatePreview {
        project_path: display_path(&project),
        upstream: Some(upstream),
        behind_count,
        can_apply: !changes.is_empty(),
        changes,
        warnings,
        dirty_files,
    })
}

#[tauri::command]
pub fn apply_repo_update(
    app: tauri::AppHandle,
    project_path: String,
    selected_files: Vec<String>,
) -> Result<ActionResult, String> {
    let project = repo_project_path(project_path)?;
    ensure_git_repo(&project)?;
    git_output(&app, &project, ["fetch", "--prune"])?;

    let upstream = upstream_ref(&app, &project)?;
    let changes = upstream_changes(&app, &project, &upstream)?;
    let changes_by_path = changes
        .iter()
        .map(|change| (change.path.clone(), change.clone()))
        .collect::<HashMap<_, _>>();
    let selected = selected_files
        .into_iter()
        .filter(|path| !path.trim().is_empty())
        .collect::<Vec<_>>();

    if selected.is_empty() {
        return Ok(ActionResult {
            ok: false,
            message: "No repo update files were selected.".to_string(),
            stdout: String::new(),
            stderr: String::new(),
        });
    }

    for path in &selected {
        if !is_safe_repo_path(path) {
            return Err(format!("Unsafe repo update path was rejected: {path}"));
        }

        if !changes_by_path.contains_key(path) {
            return Err(format!("Selected file is not in the update preview: {path}"));
        }
    }

    let dirty = dirty_files(&app, &project)?.into_iter().collect::<HashSet<_>>();
    let mut conflicting = Vec::new();

    for path in &selected {
        let Some(change) = changes_by_path.get(path) else {
            continue;
        };

        if dirty.contains(path) {
            conflicting.push(path.clone());
        }

        if let Some(old_path) = &change.old_path {
            if dirty.contains(old_path) {
                conflicting.push(old_path.clone());
            }
        }
    }

    conflicting.sort();
    conflicting.dedup();

    if !conflicting.is_empty() {
        return Ok(ActionResult {
            ok: false,
            message: "Selected files have local edits. Back them up or uncheck them before updating."
                .to_string(),
            stdout: conflicting.join("\n"),
            stderr: String::new(),
        });
    }

    let mut applied = Vec::new();

    for path in &selected {
        let change = changes_by_path
            .get(path)
            .ok_or_else(|| format!("Selected file disappeared from update preview: {path}"))?;
        apply_change(&app, &project, &upstream, change)?;
        applied.push(path.clone());
    }

    Ok(ActionResult {
        ok: true,
        message: "Selected repo changes applied.".to_string(),
        stdout: applied.join("\n"),
        stderr: String::new(),
    })
}

fn repo_project_path(project_path: String) -> Result<PathBuf, String> {
    let project = PathBuf::from(project_path);

    if project.is_dir() {
        Ok(project)
    } else {
        Err(format!(
            "Project folder does not exist: {}",
            display_path(&project)
        ))
    }
}

fn ensure_git_repo(project: &Path) -> Result<(), String> {
    if project.join(".git").exists() {
        Ok(())
    } else {
        Err("This project is not a Git repo clone.".to_string())
    }
}

fn upstream_ref(app: &tauri::AppHandle, project: &Path) -> Result<String, String> {
    if let Ok(upstream) = git_output(app, project, ["rev-parse", "--abbrev-ref", "@{upstream}"]) {
        let upstream = upstream.trim();

        if !upstream.is_empty() {
            return Ok(upstream.to_string());
        }
    }

    let branch = git_output(app, project, ["branch", "--show-current"])
        .unwrap_or_default()
        .trim()
        .to_string();
    let mut candidates = Vec::new();

    if !branch.is_empty() {
        candidates.push(format!("origin/{branch}"));
    }

    candidates.push("origin/main".to_string());
    candidates.push("origin/master".to_string());

    for candidate in candidates {
        if git_success(
            app,
            project,
            ["rev-parse", "--verify", "--quiet", candidate.as_str()],
        ) {
            return Ok(candidate);
        }
    }

    Err("No upstream branch was found for this repo.".to_string())
}

fn behind_count(app: &tauri::AppHandle, project: &Path, upstream: &str) -> Result<usize, String> {
    let range = format!("HEAD..{upstream}");
    let count = git_output(app, project, ["rev-list", "--count", range.as_str()])?;

    count
        .trim()
        .parse::<usize>()
        .map_err(|error| format!("Could not read repo update count: {error}"))
}

fn upstream_changes(
    app: &tauri::AppHandle,
    project: &Path,
    upstream: &str,
) -> Result<Vec<RepoChange>, String> {
    let range = format!("HEAD..{upstream}");
    let output = git_output(app, project, ["diff", "--name-status", range.as_str(), "--"])?;

    Ok(output
        .lines()
        .filter_map(parse_name_status_line)
        .filter(|change| !change_matches_upstream(app, project, upstream, change))
        .collect())
}

fn parse_name_status_line(line: &str) -> Option<RepoChange> {
    let parts = line.split('\t').collect::<Vec<_>>();
    let status = parts.first()?.trim();

    if status.is_empty() {
        return None;
    }

    if status.starts_with('R') || status.starts_with('C') {
        let old_path = parts.get(1)?.trim();
        let path = parts.get(2)?.trim();

        return Some(RepoChange {
            path: path.to_string(),
            old_path: Some(old_path.to_string()),
            status: status.to_string(),
            label: change_label(status),
        });
    }

    let path = parts.get(1)?.trim();

    Some(RepoChange {
        path: path.to_string(),
        old_path: None,
        status: status.to_string(),
        label: change_label(status),
    })
}

fn change_label(status: &str) -> String {
    let label = match status.chars().next().unwrap_or('M') {
        'A' => "Added",
        'C' => "Copied",
        'D' => "Deleted",
        'M' => "Modified",
        'R' => "Renamed",
        'T' => "Type changed",
        _ => "Changed",
    };

    label.to_string()
}

fn dirty_files(app: &tauri::AppHandle, project: &Path) -> Result<Vec<String>, String> {
    let output = git_output(app, project, ["status", "--porcelain"])?;
    let mut files = Vec::new();

    for line in output.lines() {
        if line.len() < 4 {
            continue;
        }

        let path = line[3..].trim();

        if let Some((old_path, new_path)) = path.split_once(" -> ") {
            files.push(old_path.to_string());
            files.push(new_path.to_string());
        } else {
            files.push(path.trim_matches('"').to_string());
        }
    }

    files.sort();
    files.dedup();
    Ok(files)
}

fn update_warnings(changes: &[RepoChange], dirty_files: &[String]) -> Vec<String> {
    let mut warnings = Vec::new();
    let paths = changes
        .iter()
        .flat_map(|change| {
            change
                .old_path
                .as_deref()
                .into_iter()
                .chain(std::iter::once(change.path.as_str()))
        })
        .collect::<Vec<_>>();

    if paths.iter().any(|path| is_zelda_ini_path(*path)) {
        warnings.push("zelda3.ini changes are included. Back up your ini file before updating."
            .to_string());
    }

    if paths.iter().any(|path| repo_path_in_folder(*path, "assets")) {
        warnings.push(
            "Assets changed. Build a fresh zelda3_assets.dat after applying this update."
                .to_string(),
        );
    }

    if paths
        .iter()
        .any(|path| repo_path_in_folder(*path, "src") || repo_path_in_folder(*path, "snes"))
    {
        warnings.push("Source changed. Rebuild the game after applying this update.".to_string());
    }

    if !dirty_files.is_empty() {
        warnings.push(
            "Local repo edits exist. Files with local edits are blocked from update until backed up or unchecked."
                .to_string(),
        );
    }

    warnings
}

fn apply_change(
    app: &tauri::AppHandle,
    project: &Path,
    upstream: &str,
    change: &RepoChange,
) -> Result<(), String> {
    if change.status.starts_with('D') {
        git_output(app, project, ["rm", "--quiet", "--ignore-unmatch", "--", change.path.as_str()])?;
        return Ok(());
    }

    if change.status.starts_with('R') {
        if let Some(old_path) = &change.old_path {
            if old_path != &change.path {
                git_output(
                    app,
                    project,
                    ["rm", "--quiet", "--ignore-unmatch", "--", old_path.as_str()],
                )?;
            }
        }
    }

    git_output(
        app,
        project,
        ["checkout", upstream, "--", change.path.as_str()],
    )?;

    Ok(())
}

fn change_matches_upstream(
    app: &tauri::AppHandle,
    project: &Path,
    upstream: &str,
    change: &RepoChange,
) -> bool {
    let new_path_matches = git_success(
        app,
        project,
        ["diff", "--quiet", upstream, "--", change.path.as_str()],
    );

    if !new_path_matches {
        return false;
    }

    match &change.old_path {
        Some(old_path) => git_success(
            app,
            project,
            ["diff", "--quiet", upstream, "--", old_path.as_str()],
        ),
        None => true,
    }
}

fn is_zelda_ini_path(path: &str) -> bool {
    path == "zelda3.ini" || path.ends_with("/zelda3.ini")
}

fn repo_path_in_folder(path: &str, folder: &str) -> bool {
    path == folder || path.starts_with(&format!("{folder}/"))
}

fn is_safe_repo_path(path: &str) -> bool {
    if path.is_empty() || path.contains('\0') || path.contains('\\') {
        return false;
    }

    !Path::new(path).components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) | Component::CurDir
        )
    })
}

fn git_success<I, S>(app: &tauri::AppHandle, project: &Path, args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let program = git_program(app);

    platform_command_in_dir(&program, project)
        .args(args)
        .output()
        .is_ok_and(|output| output.status.success())
}

fn git_output<I, S>(app: &tauri::AppHandle, project: &Path, args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let program = git_program(app);
    let output = platform_command_in_dir(&program, project)
        .args(args)
        .output()
        .map_err(|error| format!("Could not run git in {}: {error}", display_path(project)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        return Err(if stderr.is_empty() {
            format!("git exited with status {}", output.status)
        } else {
            stderr
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
