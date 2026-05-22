// This module performs user-triggered actions with fixed commands and arguments.
use crate::models::ActionResult;
use crate::paths::{display_path, resolve_scan_root, venv_python, Z3R_REPO_URL};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri_plugin_dialog::DialogExt;

const MSBUILD_HELP_URL: &str =
    "https://learn.microsoft.com/en-us/visualstudio/msbuild/msbuild?view=visualstudio";

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
pub fn choose_scan_root(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let folder = app
        .dialog()
        .file()
        .blocking_pick_folder()
        .map(|path| {
            path.into_path()
                .map_err(|error| format!("Could not read selected folder path: {error}"))
                .map(|path| display_path(&path))
        })
        .transpose()?;

    Ok(folder)
}

// Opens Microsoft's MSBuild guidance so Windows users can install the required build tools manually.
#[tauri::command]
pub fn open_msbuild_help() -> Result<ActionResult, String> {
    open_url(MSBUILD_HELP_URL)?;

    Ok(ActionResult {
        ok: true,
        message: "Opened Microsoft MSBuild installation guidance.".to_string(),
        stdout: String::new(),
        stderr: String::new(),
    })
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

// Opens a fixed trusted URL with the operating system's default browser.
fn open_url(url: &str) -> Result<(), String> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg(url);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not open help page: {error}"))?;

    Ok(())
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
