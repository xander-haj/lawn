// Tauri commands for optional feature assets shared across builds: MSU audio packs,
// Link sprite sheets, and GLSL shader files.
use crate::actions::run_command;
use crate::bundled_tools::git_program;
use crate::feature_asset_catalog::{build_group, list_file_options, list_msu_options};
use crate::feature_asset_paths::safe_relative_path;
use crate::feature_asset_store::{
    install_msu_asset, install_single_asset, pick_msu_folder, store_msu_sources,
};
use crate::models::{ActionResult, FeatureAssetReport, SpritePreviewData};
use crate::paths::display_path;
use crate::rom_storage::rom_storage_dir;
use std::fs;
use std::path::PathBuf;

pub(crate) const MSU_DIR: &str = "msu";
pub(crate) const SPRITES_DIR: &str = "sprites-gfx";
pub(crate) const SHADERS_DIR: &str = "glsl-shaders";
pub(crate) const SPRITES_SOURCE_URL: &str = "https://github.com/snesrev/sprites-gfx.git";
pub(crate) const SHADERS_SOURCE_URL: &str = "https://github.com/snesrev/glsl-shaders";
pub(crate) const MSU_DOWNLOAD_URL: &str = "https://www.zeldix.net/f11-msu1-development";

// Reports optional feature assets found in the selected build and shared launcher storage.
#[tauri::command]
pub fn read_feature_assets(
    app: tauri::AppHandle,
    project_path: String,
) -> Result<FeatureAssetReport, String> {
    let project = PathBuf::from(project_path);
    let storage = rom_storage_dir(&app)?;

    Ok(FeatureAssetReport {
        storage_dir: display_path(&storage),
        msu_download_url: MSU_DOWNLOAD_URL.to_string(),
        sprites_source_url: SPRITES_SOURCE_URL.to_string(),
        shaders_source_url: SHADERS_SOURCE_URL.to_string(),
        msu: build_group(
            list_msu_options(&project.join(MSU_DIR), "project", true)?,
            list_msu_options(&storage.join(MSU_DIR), "shared", false)?,
        ),
        sprites: build_group(
            list_file_options(
                &project.join(SPRITES_DIR),
                SPRITES_DIR,
                &["zspr"],
                "project",
            )?,
            list_file_options(&storage.join(SPRITES_DIR), SPRITES_DIR, &["zspr"], "shared")?,
        ),
        shaders: build_group(
            list_file_options(
                &project.join(SHADERS_DIR),
                SHADERS_DIR,
                &["glsl", "glslp"],
                "project",
            )?,
            list_file_options(
                &storage.join(SHADERS_DIR),
                SHADERS_DIR,
                &["glsl", "glslp"],
                "shared",
            )?,
        ),
    })
}

// Clones the public sprite or shader repository into shared launcher storage.
#[tauri::command]
pub fn clone_feature_asset(
    app: tauri::AppHandle,
    asset_kind: String,
) -> Result<ActionResult, String> {
    let (folder, url, label) = match asset_kind.as_str() {
        "sprites" => (SPRITES_DIR, SPRITES_SOURCE_URL, "sprites"),
        "shaders" => (SHADERS_DIR, SHADERS_SOURCE_URL, "shaders"),
        _ => return Err("Unknown cloneable feature asset.".to_string()),
    };
    let storage = rom_storage_dir(&app)?;
    let destination = storage.join(folder);

    if destination.is_dir() {
        return Ok(ActionResult {
            ok: true,
            message: format!("{label} repository is already available."),
            stdout: display_path(&destination),
            stderr: String::new(),
        });
    }

    fs::create_dir_all(&storage).map_err(|error| {
        format!(
            "Could not create shared asset storage {}: {error}",
            display_path(&storage)
        )
    })?;

    run_command(
        &git_program(&app),
        &["clone", url, folder],
        &storage,
        &format!("Cloned {label}."),
    )
}

// Opens a folder picker for an extracted MSU pack and stores it in shared storage.
#[tauri::command]
pub async fn choose_and_store_msu(app: tauri::AppHandle) -> Result<Option<ActionResult>, String> {
    let selected_folder = pick_msu_folder(&app).await?;
    let Some(source_path) = selected_folder else {
        return Ok(None);
    };

    store_msu_sources(&app, &[source_path]).map(Some)
}

// Stores paths received from the frontend drop zone into shared MSU storage.
#[tauri::command]
pub fn store_msu_paths(app: tauri::AppHandle, paths: Vec<String>) -> Result<ActionResult, String> {
    let sources = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    store_msu_sources(&app, &sources)
}

// Copies a selected shared asset into the selected build when it is not already there.
#[tauri::command]
pub fn install_feature_asset(
    app: tauri::AppHandle,
    project_path: String,
    asset_kind: String,
    asset_value: String,
) -> Result<ActionResult, String> {
    let project = PathBuf::from(project_path);
    let storage = rom_storage_dir(&app)?;

    match asset_kind.as_str() {
        "sprites" | "shaders" => install_single_asset(&project, &storage, &asset_value),
        "msu" => install_msu_asset(&project, &storage, &asset_value),
        _ => Err("Unknown feature asset type.".to_string()),
    }
}

// Reads the selected ZSPR file from the build first, then shared storage, so the
// launcher can render an in-app preview of the same sprite path zelda3.ini uses.
#[tauri::command]
pub fn read_sprite_preview(
    app: tauri::AppHandle,
    project_path: String,
    sprite_path: String,
) -> Result<SpritePreviewData, String> {
    let relative = safe_relative_path(&sprite_path)?;
    let project = PathBuf::from(project_path);
    let storage = rom_storage_dir(&app)?;
    let sprite = [project.join(&relative), storage.join(&relative)]
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            format!(
                "Selected sprite was not found in the build or shared storage: {}",
                display_path(&relative)
            )
        })?;
    let bytes = fs::read(&sprite)
        .map_err(|error| format!("Could not read sprite {}: {error}", display_path(&sprite)))?;
    let parsed = parse_zspr_preview(&bytes)?;
    let label = sprite
        .file_stem()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| display_path(&relative));

    Ok(SpritePreviewData {
        label,
        pixel_data: parsed.pixel_data,
        palette_data: parsed.palette_data,
    })
}

struct ParsedZsprPreview {
    pixel_data: Vec<u8>,
    palette_data: Vec<u8>,
}

fn parse_zspr_preview(bytes: &[u8]) -> Result<ParsedZsprPreview, String> {
    if bytes.len() < 21 || &bytes[0..4] != b"ZSPR" {
        return Err("Selected file is not a valid ZSPR sprite.".to_string());
    }

    let pixel_offset = read_u32_le(bytes, 9)?;
    let pixel_length = read_u16_le(bytes, 13)?;
    let palette_offset = read_u32_le(bytes, 15)?;
    let palette_length = read_u16_le(bytes, 19)?;

    if pixel_length == 0 {
        return Err("Selected ZSPR file does not include pixel data.".to_string());
    }

    let pixel_data = read_bounded_slice(bytes, pixel_offset, pixel_length.min(0x7000))?.to_vec();
    let palette_data = read_bounded_slice(bytes, palette_offset, palette_length.min(256))?.to_vec();

    Ok(ParsedZsprPreview {
        pixel_data,
        palette_data,
    })
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Result<usize, String> {
    let pair = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "Selected ZSPR file has a truncated header.".to_string())?;

    Ok(u16::from_le_bytes([pair[0], pair[1]]) as usize)
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<usize, String> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "Selected ZSPR file has a truncated header.".to_string())?;

    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]) as usize)
}

fn read_bounded_slice(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8], String> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| "Selected ZSPR file has invalid offsets.".to_string())?;

    bytes
        .get(offset..end)
        .ok_or_else(|| "Selected ZSPR file points outside its data.".to_string())
}
