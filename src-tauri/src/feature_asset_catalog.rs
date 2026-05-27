// Discovery helpers for optional feature assets in project-local and shared folders.
use crate::feature_asset_paths::{collect_files, matches_ignore_ascii, path_to_slash};
use crate::feature_assets::MSU_DIR;
use crate::models::{FeatureAssetGroup, FeatureAssetOption};
use crate::paths::display_path;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

// Combines project-local and shared options while preferring project-local duplicates.
pub(crate) fn build_group(
    project_options: Vec<FeatureAssetOption>,
    shared_options: Vec<FeatureAssetOption>,
) -> FeatureAssetGroup {
    let project_available = !project_options.is_empty();
    let shared_available = !shared_options.is_empty();
    let mut options_by_value = BTreeMap::new();

    for option in shared_options {
        options_by_value.insert(option.value.clone(), option);
    }

    for option in project_options {
        options_by_value.insert(option.value.clone(), option);
    }

    let options = options_by_value.into_values().collect::<Vec<_>>();

    FeatureAssetGroup {
        available: project_available || shared_available,
        project_available,
        shared_available,
        options,
    }
}

// Recursively lists sprite or shader files below a repository-style asset folder.
pub(crate) fn list_file_options(
    base_dir: &Path,
    value_root: &str,
    extensions: &[&str],
    source: &str,
) -> Result<Vec<FeatureAssetOption>, String> {
    let mut files = Vec::new();
    collect_files(base_dir, extensions, &mut files)?;
    files.sort();

    files
        .into_iter()
        .map(|path| {
            let relative = path.strip_prefix(base_dir).map_err(|error| {
                format!(
                    "Could not derive relative asset path for {}: {error}",
                    display_path(&path)
                )
            })?;
            let value = path_to_slash(&PathBuf::from(value_root).join(relative));
            let label = path
                .file_stem()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| value.clone());

            Ok(FeatureAssetOption {
                label,
                value,
                source: source.to_string(),
            })
        })
        .collect()
}

// Lists MSU pack prefixes below either project-local msu/ or shared storage/msu/.
pub(crate) fn list_msu_options(
    root: &Path,
    source: &str,
    include_root_pack: bool,
) -> Result<Vec<FeatureAssetOption>, String> {
    let mut options = Vec::new();

    if !root.is_dir() {
        return Ok(options);
    }

    if include_root_pack {
        if let Some(prefix) = detect_msu_prefix(root, root)? {
            options.push(FeatureAssetOption {
                label: "Project MSU".to_string(),
                value: format!("{MSU_DIR}/{prefix}"),
                source: source.to_string(),
            });
        }
    }

    for entry in fs::read_dir(root)
        .map_err(|error| format!("Could not read MSU folder {}: {error}", display_path(root)))?
    {
        let entry = entry.map_err(|error| format!("Could not read MSU entry: {error}"))?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        if let Some(prefix) = detect_msu_prefix(root, &path)? {
            let label = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| prefix.clone());

            options.push(FeatureAssetOption {
                label,
                value: format!("{MSU_DIR}/{prefix}"),
                source: source.to_string(),
            });
        }
    }

    options.sort_by(|left, right| left.label.cmp(&right.label));
    Ok(options)
}

// Finds the first MSU audio file and returns the path prefix used by zelda3.ini.
fn detect_msu_prefix(root: &Path, folder: &Path) -> Result<Option<String>, String> {
    let mut audio_files = Vec::new();
    collect_files(folder, &["pcm", "opuz", "msu"], &mut audio_files)?;
    audio_files.sort();

    for file in audio_files {
        if let Some(prefix) = msu_prefix_from_file(root, &file)? {
            return Ok(Some(prefix));
        }
    }

    Ok(None)
}

// Derives `pack/alttp_msu-` from a file such as `pack/alttp_msu-1.pcm`.
fn msu_prefix_from_file(root: &Path, file: &Path) -> Result<Option<String>, String> {
    let Some(extension) = file.extension().and_then(|ext| ext.to_str()) else {
        return Ok(None);
    };
    let stem = file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let prefix =
        if matches_ignore_ascii(extension, "pcm") || matches_ignore_ascii(extension, "opuz") {
            numbered_msu_prefix(stem)
        } else if matches_ignore_ascii(extension, "msu") {
            Some(format!("{stem}-"))
        } else {
            None
        };
    let Some(prefix) = prefix else {
        return Ok(None);
    };
    let parent = file.parent().unwrap_or(root);
    let relative_parent = parent.strip_prefix(root).map_err(|error| {
        format!(
            "Could not derive MSU prefix from {}: {error}",
            display_path(file)
        )
    })?;
    let relative_prefix = relative_parent.join(prefix);

    Ok(Some(path_to_slash(&relative_prefix)))
}

// Removes the final track number from names like `alttp_msu-1`.
fn numbered_msu_prefix(stem: &str) -> Option<String> {
    let dash = stem.rfind('-')?;
    let track = &stem[dash + 1..];

    (!track.is_empty() && track.chars().all(|character| character.is_ascii_digit()))
        .then(|| stem[..=dash].to_string())
}
