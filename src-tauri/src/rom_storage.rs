// This module owns launcher-managed ROM storage and the copy step that seeds
// newly cloned projects with the user's legally supplied zelda3.sfc file.
use crate::command_env::open_path;
use crate::file_dialogs::pick_rom_file;
use crate::models::{ActionResult, RomStatus};
use crate::paths::display_path;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

// Keep ROMs under an app-owned subfolder so user uploads stay outside source repos until copied.
const ROM_STORAGE_DIR: &str = "roms";
// The zelda3 toolchain expects this exact filename in the project root.
const STORED_ROM_NAME: &str = "zelda3.sfc";

// Reports whether the launcher-managed ROM copy currently exists.
#[tauri::command]
pub fn stored_rom_status(app: tauri::AppHandle) -> Result<RomStatus, String> {
    rom_status(&app)
}

// Opens a native file picker, validates the selected .sfc file, and stores it as zelda3.sfc.
#[tauri::command]
pub async fn choose_and_store_rom(app: tauri::AppHandle) -> Result<Option<RomStatus>, String> {
    let selected_rom = pick_rom_file(&app).await?;
    let Some(source_path) = selected_rom else {
        return Ok(None);
    };

    if !has_sfc_extension(&source_path) {
        return Err("Select a .sfc ROM file.".to_string());
    }

    let storage_dir = rom_storage_dir(&app)?;
    fs::create_dir_all(&storage_dir).map_err(|error| {
        format!(
            "Could not create ROM storage folder {}: {error}",
            display_path(&storage_dir)
        )
    })?;

    fs::copy(&source_path, stored_rom_path(&app)?)
        .map_err(|error| format!("Could not store selected ROM: {error}"))?;

    rom_status(&app).map(Some)
}

// Opens the launcher-managed ROM storage folder in the user's platform file explorer.
#[tauri::command]
pub fn open_stored_rom_folder(app: tauri::AppHandle) -> Result<ActionResult, String> {
    let storage_dir = rom_storage_dir(&app)?;
    fs::create_dir_all(&storage_dir).map_err(|error| {
        format!(
            "Could not create ROM storage folder {}: {error}",
            display_path(&storage_dir)
        )
    })?;

    open_path(&storage_dir, "ROM storage folder")?;

    Ok(ActionResult {
        ok: true,
        message: format!("Opened ROM storage folder: {}", display_path(&storage_dir)),
        stdout: String::new(),
        stderr: String::new(),
    })
}

// Copies the stored ROM into each detected project that does not already have it beside zelda3.ini.
#[tauri::command]
pub fn sync_stored_rom_to_projects(
    app: tauri::AppHandle,
    project_paths: Vec<String>,
) -> Result<ActionResult, String> {
    let source_path = stored_rom_path(&app)?;

    if !source_path.is_file() {
        return Ok(ActionResult {
            ok: true,
            message: "No uploaded SFC is available to sync.".to_string(),
            stdout: String::new(),
            stderr: String::new(),
        });
    }

    let mut copied = Vec::new();

    for project_path in project_paths {
        let project = PathBuf::from(project_path);
        let target_dir = rom_target_dir(&project);
        let destination = target_dir.join(STORED_ROM_NAME);

        if destination.is_file() {
            continue;
        }

        fs::copy(&source_path, &destination).map_err(|error| {
            format!(
                "Could not copy uploaded SFC to {}: {error}",
                display_path(&destination)
            )
        })?;
        copied.push(display_path(&destination));
    }

    Ok(ActionResult {
        ok: true,
        message: format!("SFC sync complete. {} repo(s) updated.", copied.len()),
        stdout: copied.join("\n"),
        stderr: String::new(),
    })
}

// Copies the stored ROM into a cloned project root as zelda3.sfc when one is available.
pub(crate) fn copy_stored_rom_to_project(
    app: &tauri::AppHandle,
    project_path: &Path,
) -> Result<Option<PathBuf>, String> {
    let source_path = stored_rom_path(app)?;

    if !source_path.is_file() {
        return Ok(None);
    }

    let destination = rom_target_dir(project_path).join(STORED_ROM_NAME);
    fs::copy(&source_path, &destination).map_err(|error| {
        format!(
            "Could not copy stored ROM to {}: {error}",
            display_path(&destination)
        )
    })?;

    Ok(Some(destination))
}

// Finds the folder where zelda3.ini lives; the ROM must sit beside that file.
fn rom_target_dir(project_path: &Path) -> PathBuf {
    for ini_name in ["zelda3.ini", "zelda.ini"] {
        let ini_path = project_path.join(ini_name);

        if ini_path.is_file() {
            return ini_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| project_path.to_path_buf());
        }
    }

    project_path.to_path_buf()
}

// Builds the serializable frontend status from the canonical storage location.
fn rom_status(app: &tauri::AppHandle) -> Result<RomStatus, String> {
    let storage_dir = rom_storage_dir(app)?;
    let rom_path = storage_dir.join(STORED_ROM_NAME);
    let available = rom_path.is_file();

    Ok(RomStatus {
        available,
        file_name: available.then(|| STORED_ROM_NAME.to_string()),
        path: available.then(|| display_path(&rom_path)),
        storage_dir: display_path(&storage_dir),
    })
}

// Resolves the app-owned directory where user-supplied ROMs are stored outside cloned repos.
pub(crate) fn rom_storage_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join(ROM_STORAGE_DIR))
        .map_err(|error| format!("Could not resolve launcher data folder: {error}"))
}

// Resolves the fixed stored ROM path used by both import and clone-copy operations.
fn stored_rom_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(rom_storage_dir(app)?.join(STORED_ROM_NAME))
}

// Accepts only .sfc files, case-insensitively, so accidental .smc or archive uploads do not seed clones.
fn has_sfc_extension(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("sfc"))
}
