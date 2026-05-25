// This module owns asset extraction plus platform build routes. Keeping these
// commands separate from general launcher actions makes Windows' Visual Studio
// and TCC paths explicit for the frontend.
use crate::actions::run_command;
use crate::bundled_tools::{bundled_sdl2_dll, bundled_sdl2_root, bundled_tcc, find_msbuild};
use crate::makefile_patches::apply_windows_solution_patch_to_project;
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
    apply_windows_solution_patch_to_project(project)?;
    let msbuild = find_msbuild().ok_or_else(|| {
        "MSBuild was not found. Install Build Tools for Visual Studio or use the TCC route."
            .to_string()
    })?;
    let msbuild_program = display_path(&msbuild);

    run_command(
        &msbuild_program,
        &["Zelda3.sln", "/p:Configuration=Release", "/p:Platform=x64"],
        project,
        "Visual Studio build complete.",
    )
}

// Builds through TCC without calling run_with_tcc.bat because that batch also launches
// the game and pauses, which would trap the launcher behind a command prompt.
fn run_tcc_build(app: Option<&tauri::AppHandle>, project: &Path) -> Result<ActionResult, String> {
    let project_tcc = project.join("third_party").join("tcc").join("tcc.exe");
    let project_sdl_root = project.join("third_party").join("SDL2-2.26.3");
    let project_sdl_dll = project_sdl_root.join("lib").join("x64").join("SDL2.dll");
    let bundled_tcc = app.and_then(bundled_tcc);
    let bundled_sdl_root = app.and_then(bundled_sdl2_root);
    let bundled_sdl_dll = app.and_then(bundled_sdl2_dll);
    let tcc = if project_tcc.is_file() {
        project_tcc
    } else {
        bundled_tcc.ok_or_else(|| {
            "TCC was not found in the project or bundled launcher tools.".to_string()
        })?
    };
    let (sdl_root, sdl_dll) = if project_sdl_dll.is_file() {
        (project_sdl_root, project_sdl_dll)
    } else {
        (
            bundled_sdl_root.ok_or_else(|| {
                "SDL2 headers were not found in the project or bundled launcher tools.".to_string()
            })?,
            bundled_sdl_dll.ok_or_else(|| {
                "SDL2.dll was not found in the project or bundled launcher tools.".to_string()
            })?,
        )
    };

    if !tcc.is_file() {
        return Err("TCC executable was not found.".to_string());
    }

    if !sdl_dll.is_file() {
        return Err("SDL2.dll was not found.".to_string());
    }

    let tcc_program = quote_cmd_path(&tcc);
    let sdl_include = quote_cmd_path(&sdl_root.join("include"));
    let sdl_lib = quote_cmd_path(&sdl_root.join("lib").join("x64"));
    let command = [
        &format!("{tcc_program} -ozelda3.exe -DCOMPILER_TCC=1 -DSTBI_NO_SIMD=1"),
        "-DHAVE_STDINT_H=1 -D_HAVE_STDINT_H=1 -DSYSTEM_VOLUME_MIXER_AVAILABLE=0",
        &format!("-I{sdl_include} -L{sdl_lib} -lSDL2"),
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

// Quotes Windows command paths for the cmd.exe command string used to preserve wildcards.
fn quote_cmd_path(path: &Path) -> String {
    format!("\"{}\"", display_path(path))
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
