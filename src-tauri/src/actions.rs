// This module performs user-triggered actions with fixed commands and arguments.
use crate::models::ActionResult;
use crate::paths::{display_path, resolve_scan_root, venv_python, Z3R_REPO_URL};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri_plugin_dialog::DialogExt;

// Launches the selected game executable with its own folder as the working directory.
#[tauri::command]
pub fn launch_game(executable_path: String) -> Result<ActionResult, String> {
    let executable = PathBuf::from(executable_path);
    let working_dir = executable
        .parent()
        .ok_or_else(|| "The executable path has no parent folder.".to_string())?;

    Command::new(&executable)
        .current_dir(working_dir)
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

// Opens a native folder picker so users can choose where scanning and cloning happen.
#[tauri::command]
pub async fn choose_scan_root(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (sender, mut receiver) = tauri::async_runtime::channel(1);

    app.dialog().file().pick_folder(move |folder| {
        tauri::async_runtime::spawn(async move {
            let _ = sender.send(folder).await;
        });
    });

    let folder = receiver
        .recv()
        .await
        .ok_or_else(|| "Folder picker closed before returning a result.".to_string())?
        .map(|path| {
            path.into_path()
                .map_err(|error| format!("Could not read selected folder path: {error}"))
                .map(|path| display_path(&path))
        })
        .transpose()?;

    Ok(folder)
}

// Clones Xander's Z3R repository into the active scan root when the user requests it.
#[tauri::command]
pub fn clone_project(scan_root: Option<String>) -> Result<ActionResult, String> {
    let parent = resolve_scan_root(scan_root)?;
    let target = parent.join("Z3R");

    if target.exists() {
        return Err(format!(
            "Target folder already exists: {}",
            display_path(&target)
        ));
    }

    run_command(
        "git",
        &["clone", "--recursive", Z3R_REPO_URL, "Z3R"],
        &parent,
        "Clone complete.",
    )
}

// Clones a user-provided GitHub repository URL into the active scan root with fixed git arguments.
#[tauri::command]
pub fn clone_custom_project(
    repo_url: String,
    scan_root: Option<String>,
) -> Result<ActionResult, String> {
    let parent = resolve_scan_root(scan_root)?;
    let normalized_url = normalize_github_url(&repo_url)?;
    let folder_name = github_repo_folder_name(&normalized_url)?;
    let target = parent.join(&folder_name);

    if target.exists() {
        return Err(format!(
            "Target folder already exists: {}",
            display_path(&target)
        ));
    }

    run_command(
        "git",
        &["clone", "--recursive", &normalized_url, &folder_name],
        &parent,
        "Custom clone complete.",
    )
}

// Creates a project-local Python virtual environment without installing packages.
#[tauri::command]
pub fn create_venv(project_path: String) -> Result<ActionResult, String> {
    let project = PathBuf::from(project_path);
    let program = if cfg!(target_os = "windows") {
        "py"
    } else {
        "python3"
    };

    run_command(
        program,
        &["-m", "venv", ".venv"],
        &project,
        "Virtual environment created.",
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

// Runs the repository asset extraction through the venv Python executable.
#[tauri::command]
pub fn extract_assets(project_path: String) -> Result<ActionResult, String> {
    let project = PathBuf::from(project_path);
    let python = venv_python(&project.join(".venv"))
        .or_else(|| venv_python(&project.join("venv")))
        .ok_or_else(|| "Create a venv before extracting assets.".to_string())?;

    run_command(
        &display_path(&python),
        &["assets/restool.py", "--extract-from-rom"],
        &project,
        "Asset extraction complete.",
    )
}

// Builds the selected project using TCC on prepared Windows folders or make elsewhere.
#[tauri::command]
pub fn build_project(project_path: String) -> Result<ActionResult, String> {
    let project = PathBuf::from(project_path);

    if cfg!(target_os = "windows") {
        if project
            .join("third_party")
            .join("tcc")
            .join("tcc.exe")
            .is_file()
        {
            return run_tcc_build(&project);
        }

        return run_command(
            "msbuild",
            &["Zelda3.sln", "/p:Configuration=Release", "/p:Platform=x64"],
            &project,
            "Visual Studio build complete.",
        );
    }

    let jobs = std::thread::available_parallelism()
        .map(|count| count.get().to_string())
        .unwrap_or_else(|_| "2".to_string());
    let job_arg = format!("-j{jobs}");
    run_command("make", &[job_arg.as_str()], &project, "Build complete.")
}

// Builds through TCC without calling run_with_tcc.bat because that batch also launches the game and pauses.
fn run_tcc_build(project: &Path) -> Result<ActionResult, String> {
    let sdl_dll = project
        .join("third_party")
        .join("SDL2-2.26.3")
        .join("lib")
        .join("x64")
        .join("SDL2.dll");

    if !sdl_dll.is_file() {
        return Err("SDL2.dll was not found under third_party\\SDL2-2.26.3\\lib\\x64.".to_string());
    }

    let command = "third_party\\tcc\\tcc.exe -ozelda3.exe -DCOMPILER_TCC=1 -DSTBI_NO_SIMD=1 -DHAVE_STDINT_H=1 -D_HAVE_STDINT_H=1 -DSYSTEM_VOLUME_MIXER_AVAILABLE=0 -Ithird_party\\SDL2-2.26.3\\include -Lthird_party\\SDL2-2.26.3\\lib\\x64 -lSDL2 -I. src\\*.c snes\\*.c third_party\\gl_core\\gl_core_3_1.c third_party\\opus-1.3.1-stripped\\opus_decoder_amalgam.c";
    let mut result = run_command("cmd", &["/C", command], project, "TCC build complete.")?;

    if result.ok {
        fs::copy(&sdl_dll, project.join("SDL2.dll"))
            .map_err(|error| format!("Could not copy SDL2.dll: {error}"))?;
        result.message = "TCC build complete and SDL2.dll copied beside zelda3.exe.".to_string();
    }

    Ok(result)
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

// Derives the destination folder from an owner/repo GitHub URL after URL validation succeeds.
fn github_repo_folder_name(repo_url: &str) -> Result<String, String> {
    let repo_part = repo_url
        .trim_start_matches("https://github.com/")
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    let mut parts = repo_part.split('/');
    let owner = parts.next().unwrap_or_default();
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

    if !repo
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
    {
        return Err(
            "The repository name contains characters this launcher cannot use for a folder."
                .to_string(),
        );
    }

    Ok(repo)
}

// Executes a fixed command in a fixed working directory and captures output for the UI log.
fn run_command(
    program: &str,
    args: &[&str],
    cwd: &Path,
    success_message: &str,
) -> Result<ActionResult, String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
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
