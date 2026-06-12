// This module owns native file and folder selection for user-supplied launcher paths.
// Linux Flatpak builds set GTK_USE_PORTAL before Tauri starts so GTK-backed dialogs
// can ask Steam Deck users for sandbox-safe file and directory access.
use crate::paths::display_path;
use std::path::PathBuf;
use tauri_plugin_dialog::DialogExt;

// Opens a native or portal-backed folder picker for repository scan roots.
// The app parameter supplies Tauri's dialog handle, and the return value is the selected
// folder as a displayable path string or None when the user cancels the picker.
#[tauri::command]
pub async fn choose_scan_root(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (sender, mut receiver) = tauri::async_runtime::channel(1);

    app.dialog().file().pick_folder(move |folder| {
        tauri::async_runtime::spawn(async move {
            let _ = sender.send(folder).await;
        });
    });

    receiver
        .recv()
        .await
        .ok_or_else(|| "Folder picker closed before returning a result.".to_string())?
        .map(|path| {
            path.into_path()
                .map_err(|error| format!("Could not read selected folder path: {error}"))
                .map(|path| display_path(&path))
        })
        .transpose()
}

// Opens a native or portal-backed single-file picker restricted to .sfc ROM candidates.
// The app parameter supplies Tauri's dialog handle, and the return value is the selected
// filesystem path or None when the user cancels the picker.
pub(crate) async fn pick_rom_file(app: &tauri::AppHandle) -> Result<Option<PathBuf>, String> {
    let (sender, mut receiver) = tauri::async_runtime::channel(1);

    app.dialog()
        .file()
        .add_filter("SNES ROM", &["sfc"])
        .pick_file(move |file| {
            tauri::async_runtime::spawn(async move {
                let _ = sender.send(file).await;
            });
        });

    receiver
        .recv()
        .await
        .ok_or_else(|| "ROM picker closed before returning a result.".to_string())?
        .map(|path| {
            path.into_path()
                .map_err(|error| format!("Could not read selected ROM path: {error}"))
        })
        .transpose()
}
