// Copy/import helpers for optional feature assets selected from the Features screen.
use crate::feature_asset_paths::{path_to_slash, safe_relative_path, sanitize_folder_name};
use crate::feature_assets::MSU_DIR;
use crate::models::ActionResult;
use crate::paths::display_path;
use crate::rom_storage::rom_storage_dir;
use std::fs;
use std::path::{Component, Path, PathBuf};
use tauri_plugin_dialog::DialogExt;

// Stores selected or dropped MSU files/folders under shared storage/msu/{pack-name}.
pub(crate) fn store_msu_sources(
    app: &tauri::AppHandle,
    sources: &[PathBuf],
) -> Result<ActionResult, String> {
    if sources.is_empty() {
        return Err("No MSU files or folders were provided.".to_string());
    }

    let storage = rom_storage_dir(app)?.join(MSU_DIR);
    fs::create_dir_all(&storage).map_err(|error| {
        format!(
            "Could not create MSU storage folder {}: {error}",
            display_path(&storage)
        )
    })?;
    let pack_name = msu_pack_name(sources)?;
    let destination = storage.join(&pack_name);
    let mut copied = 0usize;

    for source in sources {
        if source.is_dir() {
            copied += copy_dir_contents(source, &destination)?;
        } else if source.is_file() {
            fs::create_dir_all(&destination).map_err(|error| {
                format!(
                    "Could not create MSU pack folder {}: {error}",
                    display_path(&destination)
                )
            })?;
            let target = destination.join(source.file_name().ok_or_else(|| {
                format!("Could not determine file name for {}", display_path(source))
            })?);
            fs::copy(source, &target).map_err(|error| {
                format!(
                    "Could not copy {} to {}: {error}",
                    display_path(source),
                    display_path(&target)
                )
            })?;
            copied += 1;
        }
    }

    Ok(ActionResult {
        ok: true,
        message: format!("Stored MSU pack {pack_name}."),
        stdout: format!("{copied} file(s) copied to {}", display_path(&destination)),
        stderr: String::new(),
    })
}

// Copies a selected sprite/shader file from shared storage into the selected project.
pub(crate) fn install_single_asset(
    project: &Path,
    storage: &Path,
    asset_value: &str,
) -> Result<ActionResult, String> {
    let relative = safe_relative_path(asset_value)?;
    let destination = project.join(&relative);

    if destination.is_file() {
        return installed_result("Asset already exists in the selected build.", &relative);
    }

    let source = storage.join(&relative);
    if !source.is_file() {
        return Err(format!(
            "Selected asset was not found in shared storage: {}",
            display_path(&source)
        ));
    }

    copy_file_with_parents(&source, &destination)?;
    installed_result("Asset copied into the selected build.", &relative)
}

// Copies a selected MSU pack into project/msu/{pack} when needed.
pub(crate) fn install_msu_asset(
    project: &Path,
    storage: &Path,
    asset_value: &str,
) -> Result<ActionResult, String> {
    let relative = safe_relative_path(asset_value)?;

    if msu_prefix_exists(project, asset_value) {
        return installed_msu_result(
            "MSU pack already exists in the selected build.",
            &relative,
            &msu_mode_for_prefix(project, asset_value),
        );
    }

    let pack = relative
        .components()
        .nth(1)
        .and_then(|component| match component {
            Component::Normal(value) => Some(value.to_os_string()),
            _ => None,
        })
        .ok_or_else(|| "Selected MSU path did not include a pack folder.".to_string())?;
    let pack_path = PathBuf::from(pack);
    let source = storage.join(MSU_DIR).join(&pack_path);
    let destination = project.join(MSU_DIR).join(&pack_path);

    if !source.is_dir() {
        return Err(format!(
            "Selected MSU pack was not found in shared storage: {}",
            display_path(&source)
        ));
    }

    copy_dir_contents(&source, &destination)?;
    installed_msu_result(
        "MSU pack copied into the selected build.",
        &relative,
        &msu_mode_for_prefix(project, asset_value),
    )
}

// Runs the non-blocking native folder picker for extracted MSU packs.
pub(crate) async fn pick_msu_folder(app: &tauri::AppHandle) -> Result<Option<PathBuf>, String> {
    let (sender, mut receiver) = tauri::async_runtime::channel(1);

    app.dialog().file().pick_folder(move |folder| {
        tauri::async_runtime::spawn(async move {
            let _ = sender.send(folder).await;
        });
    });

    receiver
        .recv()
        .await
        .ok_or_else(|| "MSU folder picker closed before returning a result.".to_string())?
        .map(|path| {
            path.into_path()
                .map_err(|error| format!("Could not read selected MSU folder path: {error}"))
        })
        .transpose()
}

// Uses the folder name for folder imports and the parent folder name for grouped file drops.
fn msu_pack_name(sources: &[PathBuf]) -> Result<String, String> {
    let first = sources
        .first()
        .ok_or_else(|| "No MSU files or folders were provided.".to_string())?;
    let name_source = if first.is_dir() {
        first.file_name()
    } else {
        first.parent().and_then(|parent| parent.file_name())
    };

    name_source
        .map(|name| sanitize_folder_name(&name.to_string_lossy()))
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "Could not determine a folder name for the MSU pack.".to_string())
}

// Checks whether a project already has at least one audio file for the selected prefix.
fn msu_prefix_exists(project: &Path, asset_value: &str) -> bool {
    let prefix_path = project.join(asset_value);
    let Some(parent) = prefix_path.parent() else {
        return false;
    };
    let Some(prefix) = prefix_path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    if !parent.is_dir() {
        return false;
    }

    fs::read_dir(parent).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let file_name = entry.file_name().to_string_lossy().to_string();
            file_name.starts_with(prefix)
                && (file_name.ends_with(".pcm")
                    || file_name.ends_with(".opuz")
                    || file_name.ends_with(".msu"))
        })
    })
}

// Creates a consistent ActionResult after an asset has been made available to the project.
fn installed_result(message: &str, relative: &Path) -> Result<ActionResult, String> {
    Ok(ActionResult {
        ok: true,
        message: message.to_string(),
        stdout: path_to_slash(relative),
        stderr: String::new(),
    })
}

// Returns the installed MSU prefix and the EnableMSU mode expected by zelda3.ini.
fn installed_msu_result(
    message: &str,
    relative: &Path,
    mode: &str,
) -> Result<ActionResult, String> {
    Ok(ActionResult {
        ok: true,
        message: message.to_string(),
        stdout: format!("{}\n{mode}", path_to_slash(relative)),
        stderr: String::new(),
    })
}

// OPUZ packs need EnableMSU = opuz; PCM packs use the normal true setting.
fn msu_mode_for_prefix(project: &Path, asset_value: &str) -> String {
    let prefix_path = project.join(asset_value);
    let Some(parent) = prefix_path.parent() else {
        return "true".to_string();
    };
    let Some(prefix) = prefix_path.file_name().and_then(|name| name.to_str()) else {
        return "true".to_string();
    };

    fs::read_dir(parent)
        .ok()
        .and_then(|entries| {
            entries.flatten().find_map(|entry| {
                let file_name = entry.file_name().to_string_lossy().to_string();
                (file_name.starts_with(prefix) && file_name.ends_with(".opuz"))
                    .then(|| "opuz".to_string())
            })
        })
        .unwrap_or_else(|| "true".to_string())
}

// Copies a directory recursively, preserving included metadata and license text files.
fn copy_dir_contents(source: &Path, destination: &Path) -> Result<usize, String> {
    if !source.is_dir() {
        return Err(format!(
            "Source folder does not exist: {}",
            display_path(source)
        ));
    }

    fs::create_dir_all(destination)
        .map_err(|error| format!("Could not create {}: {error}", display_path(destination)))?;
    let mut copied = 0usize;

    for entry in fs::read_dir(source)
        .map_err(|error| format!("Could not read {}: {error}", display_path(source)))?
    {
        let entry = entry.map_err(|error| format!("Could not read copy entry: {error}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());

        if source_path.is_dir() {
            copied += copy_dir_contents(&source_path, &destination_path)?;
        } else if source_path.is_file() {
            copy_file_with_parents(&source_path, &destination_path)?;
            copied += 1;
        }
    }

    Ok(copied)
}

// Copies one file after creating the destination parent directory.
fn copy_file_with_parents(source: &Path, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create {}: {error}", display_path(parent)))?;
    }

    fs::copy(source, destination).map_err(|error| {
        format!(
            "Could not copy {} to {}: {error}",
            display_path(source),
            display_path(destination)
        )
    })?;

    Ok(())
}
