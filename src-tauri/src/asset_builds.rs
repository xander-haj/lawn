// This module owns asset extraction plus platform build routes. Keeping these
// commands separate from general launcher actions makes Windows' Visual Studio
// and TCC paths explicit for the frontend.
use crate::actions::run_command;
use crate::bundled_tools::{bundled_sdl2_root, bundled_tcc, find_msbuild};
use crate::makefile_patches::{apply_windows_solution_patch_to_project, is_snesrev_zelda3_project};
use crate::models::ActionResult;
use crate::paths::{display_path, venv_python};
use std::fs;
use std::path::{Path, PathBuf};

enum BuildRoute {
    Automatic,
    VisualStudio,
    Tcc,
}

// Runs asset extraction and then builds with the default platform route. Unix uses
// Make, while Windows keeps the previous automatic route for any older frontend caller.
#[tauri::command]
pub fn extract_assets(project_path: String) -> Result<ActionResult, String> {
    extract_assets_with_route(None, project_path, BuildRoute::Automatic)
}

// Runs asset extraction and then forces the Visual Studio/MSBuild Windows route.
#[tauri::command]
pub fn extract_assets_visual_studio(
    app: tauri::AppHandle,
    project_path: String,
) -> Result<ActionResult, String> {
    extract_assets_with_route(Some(&app), project_path, BuildRoute::VisualStudio)
}

// Runs asset extraction and then forces the lightweight TCC Windows route.
#[tauri::command]
pub fn extract_assets_tcc(
    app: tauri::AppHandle,
    project_path: String,
) -> Result<ActionResult, String> {
    extract_assets_with_route(Some(&app), project_path, BuildRoute::Tcc)
}

// Shared extraction pipeline used by every route-specific button. It extracts
// zelda3_assets.dat first, then runs the selected compiler route only if extraction
// succeeded so build logs point at the failing stage.
fn extract_assets_with_route(
    app: Option<&tauri::AppHandle>,
    project_path: String,
    route: BuildRoute,
) -> Result<ActionResult, String> {
    let project = PathBuf::from(project_path);
    let python = venv_python(&project.join(".venv"))
        .or_else(|| venv_python(&project.join("venv")))
        .ok_or_else(|| "Create a venv before extracting assets.".to_string())?;

    let extract = run_command(
        &display_path(&python),
        &["assets/restool.py", "--extract-from-rom"],
        &project,
        "Asset extraction complete.",
    )?;

    if !extract.ok {
        return Ok(extract);
    }

    let build = build_executable(app, &project, route)?;
    let combined_stdout = join_stage_output(&extract.stdout, &build.stdout);
    let combined_stderr = join_stage_output(&extract.stderr, &build.stderr);
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

// Selects the platform compiler route. Explicit Windows buttons call the exact route
// the user chose, while the automatic route preserves older behavior if invoked.
fn build_executable(
    app: Option<&tauri::AppHandle>,
    project: &Path,
    route: BuildRoute,
) -> Result<ActionResult, String> {
    if cfg!(target_os = "windows") {
        return match route {
            BuildRoute::Tcc => run_tcc_build(app, project),
            BuildRoute::VisualStudio => run_visual_studio_build(project),
            BuildRoute::Automatic => {
                if project
                    .join("third_party")
                    .join("tcc")
                    .join("tcc.exe")
                    .is_file()
                {
                    run_tcc_build(app, project)
                } else {
                    run_visual_studio_build(project)
                }
            }
        };
    }

    let jobs = std::thread::available_parallelism()
        .map(|count| count.get().to_string())
        .unwrap_or_else(|_| "2".to_string());
    let job_arg = format!("-j{jobs}");
    run_command("make", &[job_arg.as_str()], project, "Build complete.")
}

// Applies the bundled solution patch before MSBuild so known invalid solution nesting
// does not block users who choose the Visual Studio route.
fn run_visual_studio_build(project: &Path) -> Result<ActionResult, String> {
    if is_snesrev_zelda3_project(project, None) {
        apply_windows_solution_patch_to_project(project)?;
    }

    let msbuild = find_msbuild().ok_or_else(|| {
        "MSBuild was not found. Install Build Tools for Visual Studio or use the TCC route."
            .to_string()
    })?;
    let msbuild_program = display_path(&msbuild);

    run_command(
        &msbuild_program,
        &[
            "Zelda3.sln",
            "/restore",
            "/p:RestorePackagesConfig=true",
            "/p:Configuration=Release",
            "/p:Platform=x64",
        ],
        project,
        "Visual Studio build complete.",
    )
}

// Prepares project-local TCC/SDL2 folders, then delegates to the project's batch file.
fn run_tcc_build(app: Option<&tauri::AppHandle>, project: &Path) -> Result<ActionResult, String> {
    let prepared_tools = prepare_tcc_project_tools(app, project)?;
    let mut result = run_command(
        "cmd",
        &["/C", "call", "run_with_tcc.bat"],
        project,
        "TCC build complete.",
    )?;

    if result.ok && !prepared_tools.is_empty() {
        result.message = format!("{} {}", prepared_tools.join(" "), result.message);
    }

    Ok(result)
}

// Ensures the project root has the exact local files that run_with_tcc.bat expects.
fn prepare_tcc_project_tools(
    app: Option<&tauri::AppHandle>,
    project: &Path,
) -> Result<Vec<String>, String> {
    if !project.join("run_with_tcc.bat").is_file() {
        return Err("run_with_tcc.bat was not found in the project root.".to_string());
    }

    let mut prepared = Vec::new();

    if ensure_project_tcc(app, project)? {
        prepared.push("Copied bundled TCC into third_party/tcc.".to_string());
    }

    if ensure_project_sdl2(app, project)? {
        prepared.push("Copied bundled SDL2 into third_party/SDL2-2.26.3.".to_string());
    }

    Ok(prepared)
}

// Copies the bundled TCC directory into the project when the batch-required tcc.exe is missing.
fn ensure_project_tcc(app: Option<&tauri::AppHandle>, project: &Path) -> Result<bool, String> {
    let project_tcc = project.join("third_party").join("tcc").join("tcc.exe");

    if project_tcc.is_file() {
        return Ok(false);
    }

    let bundled_tcc = app
        .and_then(bundled_tcc)
        .ok_or_else(|| "TCC was not found in the project or bundled launcher tools.".to_string())?;
    let bundled_tcc_root = bundled_tcc
        .parent()
        .ok_or_else(|| "Bundled TCC path did not have a parent folder.".to_string())?;
    let project_tcc_root = project.join("third_party").join("tcc");

    copy_dir_contents(bundled_tcc_root, &project_tcc_root)?;

    if !project_tcc.is_file() {
        return Err(
            "Copied bundled TCC, but third_party/tcc/tcc.exe is still missing.".to_string(),
        );
    }

    Ok(true)
}

// Copies the bundled SDL2 tree into the versioned project folder expected by the batch file.
fn ensure_project_sdl2(app: Option<&tauri::AppHandle>, project: &Path) -> Result<bool, String> {
    let project_sdl_root = project.join("third_party").join("SDL2-2.26.3");
    let project_sdl_header = project_sdl_root.join("include").join("SDL.h");
    let project_sdl_dll = project_sdl_root.join("lib").join("x64").join("SDL2.dll");

    if project_sdl_header.is_file() && project_sdl_dll.is_file() {
        return Ok(false);
    }

    let bundled_sdl_root = app.and_then(bundled_sdl2_root).ok_or_else(|| {
        "SDL2 headers and SDL2.dll were not found in the project or bundled launcher tools."
            .to_string()
    })?;

    copy_dir_contents(&bundled_sdl_root, &project_sdl_root)?;

    if !project_sdl_header.is_file() || !project_sdl_dll.is_file() {
        return Err(
            "Copied bundled SDL2, but third_party/SDL2-2.26.3 is still incomplete.".to_string(),
        );
    }

    Ok(true)
}

// Recursively copies every file from a bundled tool folder so license files stay with binaries.
fn copy_dir_contents(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Err(format!(
            "Bundled tool folder does not exist: {}",
            display_path(source)
        ));
    }

    fs::create_dir_all(destination)
        .map_err(|error| format!("Could not create {}: {error}", display_path(destination)))?;

    for entry in fs::read_dir(source)
        .map_err(|error| format!("Could not read {}: {error}", display_path(source)))?
    {
        let entry = entry.map_err(|error| format!("Could not read bundled tool entry: {error}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "Could not inspect bundled tool entry {}: {error}",
                display_path(&source_path)
            )
        })?;

        if file_type.is_dir() {
            copy_dir_contents(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!(
                    "Could not copy {} to {}: {error}",
                    display_path(&source_path),
                    display_path(&destination_path)
                )
            })?;
        }
    }

    Ok(())
}

// Concatenates two stage outputs with a blank line between them, skipping empties so
// the UI log does not show stray separators when one stream produced no output.
fn join_stage_output(first: &str, second: &str) -> String {
    match (first.is_empty(), second.is_empty()) {
        (true, true) => String::new(),
        (false, true) => first.to_string(),
        (true, false) => second.to_string(),
        (false, false) => format!("{first}\n{second}"),
    }
}
