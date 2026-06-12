// This module performs user-triggered actions with fixed commands and arguments.
use crate::bundled_tools::{git_program, python_program};
use crate::command_env::{open_path, platform_command_in_dir};
use crate::models::ActionResult;
use crate::paths::{
    display_path, resolve_scan_root, venv_python, Z3R_BETA_REPO_URL, Z3R_REPO_URL,
};
use crate::runtime_info::ensure_clone_scan_root;
use crate::rom_storage::copy_stored_rom_to_project;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

// Launches the selected game executable with its own folder as the working directory.
#[tauri::command]
pub fn launch_game(executable_path: String) -> Result<ActionResult, String> {
    let executable = PathBuf::from(executable_path);
    let executable_dir = executable
        .parent()
        .ok_or_else(|| "The executable path has no parent folder.".to_string())?;
    let working_dir = launch_working_dir(&executable, executable_dir);

    platform_command_in_dir(&display_path(&executable), working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not launch game: {error}"))?;

    Ok(ActionResult {
        ok: true,
        message: "Game launched.".to_string(),
        stdout: String::new(),
        stderr: String::new(),
    })
}

// Visual Studio outputs live under bin/{Platform-Configuration}; use the project root
// as cwd so assets in zelda3_assets.dat or tables/ remain discoverable at runtime.
fn launch_working_dir<'a>(executable: &'a Path, executable_dir: &'a Path) -> &'a Path {
    if !cfg!(target_os = "windows") {
        return executable_dir;
    }

    let Some(bin_dir) = executable_dir.parent() else {
        return executable_dir;
    };
    let Some(project_dir) = bin_dir.parent() else {
        return executable_dir;
    };
    let is_visual_studio_output = bin_dir
        .file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("bin"));
    let has_windows_runtime = executable
        .file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("zelda3.exe"))
        && executable_dir.join("SDL2.dll").is_file();

    if is_visual_studio_output && has_windows_runtime {
        project_dir
    } else {
        executable_dir
    }
}

// Clones Xander's Z3R repository into the active scan root when the user requests it.
#[tauri::command]
pub fn clone_project(
    app: tauri::AppHandle,
    scan_root: Option<String>,
    beta: Option<bool>,
) -> Result<ActionResult, String> {
    ensure_clone_scan_root(&scan_root)?;
    let parent = resolve_scan_root(scan_root)?;
    let use_beta = beta.unwrap_or(false);
    let repo_name = if use_beta { "Z3R-Beta" } else { "Z3R" };
    let repo_url = if use_beta {
        Z3R_BETA_REPO_URL
    } else {
        Z3R_REPO_URL
    };
    let target = parent.join(repo_name);

    if target.exists() {
        return Err(format!(
            "Target folder already exists: {}",
            display_path(&target)
        ));
    }

    let mut result = run_command(
        &git_program(&app),
        &["clone", "--recursive", repo_url, repo_name],
        &parent,
        "Clone complete.",
    )?;

    attach_rom_copy_message(&app, &target, &mut result)?;
    Ok(result)
}

// Clones a user-provided GitHub repository URL into a nested {scan_root}/{owner}/{repo}
// layout so multiple forks that share a repo name (e.g. john/zelda3 and steve/zelda3) can
// coexist beside the launcher without colliding. The canonical Z3R clone stays flat at
// {scan_root}/Z3R — only the custom clone path nests under an owner segment.
#[tauri::command]
pub fn clone_custom_project(
    app: tauri::AppHandle,
    repo_url: String,
    scan_root: Option<String>,
) -> Result<ActionResult, String> {
    ensure_clone_scan_root(&scan_root)?;
    let parent = resolve_scan_root(scan_root)?;
    let normalized_url = normalize_github_url(&repo_url)?;
    let (owner, repo) = github_repo_owner_and_name(&normalized_url)?;
    let owner_dir = parent.join(&owner);
    let target = owner_dir.join(&repo);

    if target.exists() {
        return Err(format!(
            "Target folder already exists: {}",
            display_path(&target)
        ));
    }

    // Pre-create the owner folder so git can write into a clean leaf. create_dir_all is a
    // no-op when the owner folder already exists from a previous fork clone under the
    // same owner, which is exactly the multi-fork case this feature is designed for.
    fs::create_dir_all(&owner_dir).map_err(|error| {
        format!(
            "Could not create owner folder {}: {error}",
            display_path(&owner_dir)
        )
    })?;

    // Pass the relative "{owner}/{repo}" target to git so the cwd stays at the scan root.
    // Matches how clone_project keeps its working directory at the parent.
    let relative_target = format!("{owner}/{repo}");

    let mut result = run_command(
        &git_program(&app),
        &["clone", "--recursive", &normalized_url, &relative_target],
        &parent,
        "Custom clone complete.",
    )?;

    attach_rom_copy_message(&app, &target, &mut result)?;
    Ok(result)
}

// Opens a detected project folder in the platform file manager.
#[tauri::command]
pub fn open_project_folder(project_path: String) -> Result<ActionResult, String> {
    let project = PathBuf::from(project_path);

    if !project.is_dir() {
        return Err(format!(
            "Project folder does not exist: {}",
            display_path(&project)
        ));
    }

    open_path(&project, "project folder")?;

    Ok(ActionResult {
        ok: true,
        message: format!("Opened project folder: {}", display_path(&project)),
        stdout: String::new(),
        stderr: String::new(),
    })
}

// Creates a project-local Python virtual environment without installing packages.
#[tauri::command]
pub fn create_venv(app: tauri::AppHandle, project_path: String) -> Result<ActionResult, String> {
    let project = PathBuf::from(project_path);
    let program = python_program(&app);

    let mut result = run_command(
        &program,
        &["-m", "venv", ".venv"],
        &project,
        "Virtual environment created.",
    )?;

    if !result.ok {
        result = add_venv_creation_guidance(result, &program, &project);
    }

    Ok(result)
}

// Rewrites Debian/Ubuntu's missing ensurepip failure into an installable package hint.
fn add_venv_creation_guidance(
    mut result: ActionResult,
    program: &str,
    project: &Path,
) -> ActionResult {
    let output = format!("{}\n{}", result.stdout, result.stderr);

    if !is_missing_ensurepip_error(&output) {
        return result;
    }

    result.message = if cfg!(target_os = "linux") {
        linux_venv_support_message(&python_version_venv_package(program, project))
    } else {
        "Python could not create .venv because ensurepip is missing. Install Python venv support, \
then press Create venv again."
            .to_string()
    };

    result
}

// Detects the common Python venv failure shown by Debian and Ubuntu when python*-venv is missing.
fn is_missing_ensurepip_error(output: &str) -> bool {
    output.contains("ensurepip is not available")
        || output.contains("No module named ensurepip")
        || output.contains("python3-venv")
        || (output.contains("python3.") && output.contains("-venv"))
}

// Asks the selected Python for its major/minor version so Ubuntu users see the right venv package.
fn python_version_venv_package(program: &str, cwd: &Path) -> String {
    let output = platform_command_in_dir(program, cwd)
        .args([
            "-c",
            "import sys; print(f'python{sys.version_info.major}.{sys.version_info.minor}-venv')",
        ])
        .output();

    let Ok(output) = output else {
        return "python3-venv".to_string();
    };

    let package = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if output.status.success() && !package.is_empty() {
        package
    } else {
        "python3-venv".to_string()
    }
}

// Keeps the Ubuntu guidance specific without hiding the generic package fallback used by some distros.
fn linux_venv_support_message(version_package: &str) -> String {
    if version_package == "python3-venv" {
        return "Python could not create .venv because ensurepip is missing. On Debian/Ubuntu, run \
`sudo apt-get install python3-venv`, then press Create venv again."
            .to_string();
    }

    format!(
        "Python could not create .venv because ensurepip is missing. On Debian/Ubuntu, run \
`sudo apt-get install {version_package}`. If that package is unavailable, run \
`sudo apt-get install python3-venv`, then press Create venv again."
    )
}

// Installs project Python requirements into the selected venv for asset extraction.
#[tauri::command]
pub fn install_dependencies(project_path: String) -> Result<ActionResult, String> {
    let project = PathBuf::from(project_path);
    let python = venv_python(&project.join(".venv"))
        .or_else(|| venv_python(&project.join("venv")))
        .ok_or_else(|| "Create a venv before installing dependencies.".to_string())?;

    run_command(
        &display_path(&python),
        &["-m", "pip", "install", "-r", "requirements.txt"],
        &project,
        "Python dependencies installed.",
    )
}

// Adds clone-time ROM copy results to the command message while leaving failed clones untouched.
fn attach_rom_copy_message(
    app: &tauri::AppHandle,
    project_path: &Path,
    result: &mut ActionResult,
) -> Result<(), String> {
    if !result.ok {
        return Ok(());
    }

    let clone_message = result.message.clone();
    result.message = match copy_stored_rom_to_project(app, project_path)? {
        Some(path) => format!("{clone_message} SFC copied to {}.", display_path(&path)),
        None => format!("{clone_message} No uploaded SFC is available to copy yet."),
    };

    Ok(())
}

// Accepts only plain GitHub HTTPS repository URLs so text input cannot become shell syntax.
fn normalize_github_url(repo_url: &str) -> Result<String, String> {
    let trimmed = repo_url.trim();

    if trimmed.starts_with("git clone") {
        return Err("Paste only the GitHub repository URL, not a git clone command.".to_string());
    }

    if trimmed.contains(char::is_whitespace) {
        return Err("The GitHub URL cannot contain spaces.".to_string());
    }

    if !trimmed.starts_with("https://github.com/") {
        return Err("Enter a GitHub URL that starts with https://github.com/.".to_string());
    }

    Ok(trimmed.trim_end_matches('/').to_string())
}

// Derives owner and repo from a validated owner/repo GitHub URL. Both segments are run
// through the same filesystem-safe character whitelist so the nested {owner}/{repo}
// destination cannot contain shell- or path-hostile characters.
fn github_repo_owner_and_name(repo_url: &str) -> Result<(String, String), String> {
    let repo_part = repo_url
        .trim_start_matches("https://github.com/")
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    let mut parts = repo_part.split('/');
    let owner = parts.next().unwrap_or_default().to_string();
    let repo = parts
        .next()
        .unwrap_or_default()
        .trim_end_matches(".git")
        .to_string();

    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return Err(
            "Enter a GitHub repository URL like https://github.com/owner/repo.".to_string(),
        );
    }

    // Same character set for both segments: ascii alphanumerics plus . _ - keeps us safe
    // on every supported OS without rejecting normal GitHub names.
    if !is_safe_segment(&owner) {
        return Err(
            "The owner name contains characters this launcher cannot use for a folder.".to_string(),
        );
    }

    if !is_safe_segment(&repo) {
        return Err(
            "The repository name contains characters this launcher cannot use for a folder."
                .to_string(),
        );
    }

    Ok((owner, repo))
}

// Reusable filesystem-safe segment check shared by the owner and repo validators.
fn is_safe_segment(segment: &str) -> bool {
    segment
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
}

// Executes a fixed command in a fixed working directory and captures output for the UI log.
pub(crate) fn run_command(
    program: &str,
    args: &[&str],
    cwd: &Path,
    success_message: &str,
) -> Result<ActionResult, String> {
    let output = platform_command_in_dir(program, cwd)
        .args(args)
        .output()
        .map_err(|error| format!("Could not run {program}: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(ActionResult {
        ok: output.status.success(),
        message: if output.status.success() {
            success_message.to_string()
        } else {
            format!("{program} exited with status {}", output.status)
        },
        stdout,
        stderr,
    })
}
