// This module resolves launcher-bundled Windows tools and common external build
// tools. Callers use these helpers before falling back to PATH so packaged releases
// can behave like a self-contained toolkit.
use crate::paths::display_path;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::Manager;

// Finds bundled Git for Windows inside the Tauri resource directory.
pub fn bundled_git(app: &tauri::AppHandle) -> Option<PathBuf> {
    first_existing(&[
        windows_tools_dir(app)
            .join("git")
            .join("cmd")
            .join("git.exe"),
        windows_tools_dir(app)
            .join("git")
            .join("bin")
            .join("git.exe"),
    ])
}

// Finds bundled Python inside either the NuGet package layout or a flat runtime folder.
pub fn bundled_python(app: &tauri::AppHandle) -> Option<PathBuf> {
    first_existing(&[
        windows_tools_dir(app)
            .join("python")
            .join("tools")
            .join("python.exe"),
        windows_tools_dir(app).join("python").join("python.exe"),
    ])
}

// Finds bundled TCC inside the expected launcher toolkit folder.
pub fn bundled_tcc(app: &tauri::AppHandle) -> Option<PathBuf> {
    first_existing(&[windows_tools_dir(app).join("tcc").join("tcc.exe")])
}

// Finds the bundled SDL2 runtime DLL in the normalized toolkit layout.
pub fn bundled_sdl2_dll(app: &tauri::AppHandle) -> Option<PathBuf> {
    first_existing(&[windows_tools_dir(app)
        .join("sdl2")
        .join("lib")
        .join("x64")
        .join("SDL2.dll")])
}

// Finds the bundled SDL2 root so TCC can reference include and library folders.
pub fn bundled_sdl2_root(app: &tauri::AppHandle) -> Option<PathBuf> {
    let root = windows_tools_dir(app).join("sdl2");
    root.join("include").is_dir().then_some(root)
}

// Returns the best available Git command, preferring bundled Git on Windows releases.
pub fn git_program(app: &tauri::AppHandle) -> String {
    if cfg!(target_os = "windows") {
        if let Some(path) = bundled_git(app) {
            return display_path(&path);
        }
    }

    "git".to_string()
}

// Returns the best available Python command for creating venvs.
pub fn python_program(app: &tauri::AppHandle) -> String {
    if cfg!(target_os = "windows") {
        if let Some(path) = bundled_python(app) {
            return display_path(&path);
        }

        return "py".to_string();
    }

    "python3".to_string()
}

// Finds MSBuild from PATH, Visual Studio's vswhere locator, or common Build Tools paths.
pub fn find_msbuild() -> Option<PathBuf> {
    if let Some(path) = first_command_stdout_path("where", &["msbuild"]) {
        return Some(path);
    }

    if let Some(path) = find_msbuild_with_vswhere() {
        return Some(path);
    }

    first_existing(&common_msbuild_paths())
}

// Builds a readable check detail for bundled tools.
pub fn bundled_detail(label: &str, path: &Path) -> String {
    format!("Using bundled {label}: {}", display_path(path))
}

// Resolves `src-tauri/bundled-tools` in dev and the packaged resource copy in release.
fn bundled_tools_dir(app: &tauri::AppHandle) -> PathBuf {
    if cfg!(debug_assertions) {
        return PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bundled-tools");
    }

    app.path()
        .resource_dir()
        .map(|path| path.join("bundled-tools"))
        .unwrap_or_else(|_| PathBuf::from("bundled-tools"))
}

// Returns the Windows-specific toolkit root inside the bundled tools directory.
fn windows_tools_dir(app: &tauri::AppHandle) -> PathBuf {
    bundled_tools_dir(app).join("windows")
}

// Returns the first path that exists as a file.
fn first_existing(paths: &[PathBuf]) -> Option<PathBuf> {
    paths.iter().find(|path| path.is_file()).cloned()
}

// Runs a lookup command and treats the first stdout line as a filesystem path.
fn first_command_stdout_path(program: &str, args: &[&str]) -> Option<PathBuf> {
    let output = Command::new(program).args(args).output().ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

// Uses Visual Studio's official locator when Build Tools installed MSBuild outside PATH.
fn find_msbuild_with_vswhere() -> Option<PathBuf> {
    let program_files_x86 = env::var_os("ProgramFiles(x86)").map(PathBuf::from)?;
    let vswhere = program_files_x86
        .join("Microsoft Visual Studio")
        .join("Installer")
        .join("vswhere.exe");

    if !vswhere.is_file() {
        return None;
    }

    first_command_stdout_path(
        &display_path(&vswhere),
        &[
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.Component.MSBuild",
            "-find",
            "MSBuild\\**\\Bin\\MSBuild.exe",
        ],
    )
}

// Covers the default Visual Studio 2022 install locations when vswhere is unavailable.
fn common_msbuild_paths() -> Vec<PathBuf> {
    let Some(program_files) = env::var_os("ProgramFiles").map(PathBuf::from) else {
        return Vec::new();
    };

    let editions = ["BuildTools", "Community", "Professional", "Enterprise"];
    editions
        .iter()
        .map(|edition| {
            program_files
                .join("Microsoft Visual Studio")
                .join("2022")
                .join(edition)
                .join("MSBuild")
                .join("Current")
                .join("Bin")
                .join("MSBuild.exe")
        })
        .collect()
}
