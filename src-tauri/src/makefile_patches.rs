// This module applies launcher-bundled build-file patches to known upstream forks.
use crate::models::ActionResult;
use crate::paths::display_path;
use std::fs;
use std::path::{Path, PathBuf};

const SNESREV_ZELDA3_MAKEFILE: &str =
    include_str!("../patches/snesrev-zelda3/Makefile");

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
        message: format!("Patched Makefile installed at {}.", display_path(&destination)),
        stdout: String::new(),
        stderr: String::new(),
    })
}

// Checks whether the selected project already has the launcher-bundled snesrev Makefile.
pub(crate) fn has_snesrev_makefile_patch(project_path: &Path) -> bool {
    fs::read_to_string(project_path.join("Makefile"))
        .is_ok_and(|content| content == SNESREV_ZELDA3_MAKEFILE)
}
