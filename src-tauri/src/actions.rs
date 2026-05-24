// This module performs user-triggered actions with fixed commands and arguments.
use crate::makefile_patches::apply_windows_solution_patch_to_project;
use crate::models::ActionResult;
use crate::paths::{display_path, resolve_scan_root, venv_python, Z3R_REPO_URL};
use crate::rom_storage::copy_stored_rom_to_project;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri_plugin_dialog::DialogExt;

// Launches the selected game executable with its own folder as the working directory.
#[tauri::command]
pub fn launch_game(executable_path: String) -> Result<ActionResult, String> {
    let executable = PathBuf::from(executable_path);
    let executable_dir = executable
        .parent()
        .ok_or_else(|| "The executable path has no parent folder.".to_string())?;
    let working_dir = launch_working_dir(&executable, executable_dir);

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
pub fn clone_project(
    app: tauri::AppHandle,
    scan_root: Option<String>,
) -> Result<ActionResult, String> {
    let parent = resolve_scan_root(scan_root)?;
    let target = parent.join("Z3R");

    if target.exists() {
        return Err(format!(
            "Target folder already exists: {}",
            display_path(&target)
        ));
    }

    let mut result = run_command(
        "git",
        &["clone", "--recursive", Z3R_REPO_URL, "Z3R"],
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
        "git",
        &["clone", "--recursive", &normalized_url, &relative_target],
        &parent,
        "Custom clone complete.",
    )?;

    attach_rom_copy_message(&app, &target, &mut result)?;
    Ok(result)
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

// Runs asset extraction through the venv Python and then compiles the platform executable
// so a single Build assets press produces both zelda3_assets.dat and the runnable binary.
#[tauri::command]
pub fn extract_assets(project_path: String) -> Result<ActionResult, String> {
    let project = PathBuf::from(project_path);
    let python = venv_python(&project.join(".venv"))
        .or_else(|| venv_python(&project.join("venv")))
        .ok_or_else(|| "Create a venv before extracting assets.".to_string())?;

    // Stage 1: extract resources and compile zelda3_assets.dat via restool.py.
    let extract = run_command(
        &display_path(&python),
        &["assets/restool.py", "--extract-from-rom"],
        &project,
        "Asset extraction complete.",
    )?;

    // Surface the extract failure as-is so the user sees restool's stderr and stops here.
    if !extract.ok {
        return Ok(extract);
    }

    // Stage 2: compile the platform executable so the project becomes Play-ready.
    let build = build_executable(&project)?;
    let combined_stdout = join_stage_output(&extract.stdout, &build.stdout);
    let combined_stderr = join_stage_output(&extract.stderr, &build.stderr);

    // Name the failing stage when the build step fails so the user knows which output to read.
    let message = if build.ok {
        "Asset extraction and build complete.".to_string()
    } else {
        format!(
            "Build step failed after asset extraction: {}",
            build.message
        )
    };

    Ok(ActionResult {
        ok: build.ok,
        message,
        stdout: combined_stdout,
        stderr: combined_stderr,
    })
}

// Compiles the selected project using TCC on prepared Windows folders or make elsewhere.
// Kept crate-private because extract_assets is now the only caller; no Tauri command is exposed.
fn build_executable(project: &Path) -> Result<ActionResult, String> {
    if cfg!(target_os = "windows") {
        if project
            .join("third_party")
            .join("tcc")
            .join("tcc.exe")
            .is_file()
        {
            return run_tcc_build(project);
        }

        apply_windows_solution_patch_to_project(project)?;

        return run_command(
            "msbuild",
            &["Zelda3.sln", "/p:Configuration=Release", "/p:Platform=x64"],
            project,
            "Visual Studio build complete.",
        );
    }

    let jobs = std::thread::available_parallelism()
        .map(|count| count.get().to_string())
        .unwrap_or_else(|_| "2".to_string());
    let job_arg = format!("-j{jobs}");
    run_command("make", &[job_arg.as_str()], project, "Build complete.")
}

// Concatenates two stage outputs with a blank line between them, skipping empties so the UI log
// does not show stray separators when one stage produced no output on a given stream.
fn join_stage_output(first: &str, second: &str) -> String {
    match (first.is_empty(), second.is_empty()) {
        (true, true) => String::new(),
        (false, true) => first.to_string(),
        (true, false) => second.to_string(),
        (false, false) => format!("{first}\n{second}"),
    }
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

    let command = [
        "third_party\\tcc\\tcc.exe -ozelda3.exe -DCOMPILER_TCC=1 -DSTBI_NO_SIMD=1",
        "-DHAVE_STDINT_H=1 -D_HAVE_STDINT_H=1 -DSYSTEM_VOLUME_MIXER_AVAILABLE=0",
        "-Ithird_party\\SDL2-2.26.3\\include -Lthird_party\\SDL2-2.26.3\\lib\\x64 -lSDL2",
        "-I. src\\*.c snes\\*.c third_party\\gl_core\\gl_core_3_1.c",
        "third_party\\opus-1.3.1-stripped\\opus_decoder_amalgam.c",
    ]
    .join(" ");
    let mut result = run_command("cmd", &["/C", &command], project, "TCC build complete.")?;

    if result.ok {
        fs::copy(&sdl_dll, project.join("SDL2.dll"))
            .map_err(|error| format!("Could not copy SDL2.dll: {error}"))?;
        result.message = "TCC build complete and SDL2.dll copied beside zelda3.exe.".to_string();
    }

    Ok(result)
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
