// This module applies launcher-bundled build-file patches to known upstream forks.
use crate::models::ActionResult;
use crate::paths::display_path;
use std::fs;
use std::path::{Path, PathBuf};

const SNESREV_ZELDA3_MAKEFILE: &str = include_str!("../patches/snesrev-zelda3/Makefile");
const SNESREV_ZELDA3_SOLUTION: &str = include_str!("../patches/windows/Zelda3.sln");

// Replaces snesrev/zelda3's Makefile with the launcher-bundled patched Makefile.
#[tauri::command]
pub fn apply_snesrev_makefile_patch(project_path: String) -> Result<ActionResult, String> {
    let project = PathBuf::from(project_path);

    if !project.is_dir() {
        return Err(format!(
            "Project folder does not exist: {}",
            display_path(&project)
        ));
    }

    let destination = project.join("Makefile");
    fs::write(&destination, SNESREV_ZELDA3_MAKEFILE)
        .map_err(|error| format!("Could not replace Makefile: {error}"))?;

    Ok(ActionResult {
        ok: true,
        message: format!(
            "Patched Makefile installed at {}.",
            display_path(&destination)
        ),
        stdout: String::new(),
        stderr: String::new(),
    })
}

// Replaces snesrev/zelda3's Visual Studio solution with the launcher-bundled Windows patch.
#[tauri::command]
pub fn apply_snesrev_solution_patch(project_path: String) -> Result<ActionResult, String> {
    let project = PathBuf::from(project_path);

    apply_windows_solution_patch_to_project(&project)?;

    Ok(ActionResult {
        ok: true,
        message: format!("Patched solution installed in {}.", display_path(&project)),
        stdout: String::new(),
        stderr: String::new(),
    })
}

// Applies the Windows-only solution patch needed before MSBuild reads the project.
pub(crate) fn apply_windows_solution_patch_to_project(project: &Path) -> Result<(), String> {
    if !project.is_dir() {
        return Err(format!(
            "Project folder does not exist: {}",
            display_path(project)
        ));
    }

    fs::write(project.join("Zelda3.sln"), SNESREV_ZELDA3_SOLUTION)
        .map_err(|error| format!("Could not replace Zelda3.sln: {error}"))
}

// Checks whether the selected project already has the launcher-bundled snesrev Makefile.
pub(crate) fn has_snesrev_makefile_patch(project_path: &Path) -> bool {
    fs::read_to_string(project_path.join("Makefile"))
        .is_ok_and(|content| content == SNESREV_ZELDA3_MAKEFILE)
}

// Checks whether the selected project already has the launcher-bundled snesrev solution.
pub(crate) fn has_snesrev_solution_patch(project_path: &Path) -> bool {
    fs::read_to_string(project_path.join("Zelda3.sln"))
        .is_ok_and(|content| content == SNESREV_ZELDA3_SOLUTION)
}
