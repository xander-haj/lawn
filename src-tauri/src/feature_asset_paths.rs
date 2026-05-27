// Shared path helpers for optional feature asset discovery and copying.
use crate::paths::display_path;
use std::fs;
use std::path::{Component, Path, PathBuf};

// Recursively finds files whose extension is included in the provided lowercase list.
pub(crate) fn collect_files(
    dir: &Path,
    extensions: &[&str],
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(dir)
        .map_err(|error| format!("Could not read {}: {error}", display_path(dir)))?
    {
        let entry = entry.map_err(|error| format!("Could not read asset entry: {error}"))?;
        let path = entry.path();

        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy() == ".git")
        {
            continue;
        }

        if path.is_dir() {
            collect_files(&path, extensions, files)?;
        } else if path_has_extension(&path, extensions) {
            files.push(path);
        }
    }

    Ok(())
}

// Accepts only relative paths below the project root/storage root.
pub(crate) fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);

    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err("Selected asset path is not safe to copy.".to_string());
    }

    Ok(path)
}

// Normalizes platform paths into zelda3.ini-friendly forward-slash paths.
pub(crate) fn path_to_slash(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

// Keeps imported pack folder names portable across all supported filesystems.
pub(crate) fn sanitize_folder_name(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .collect()
}

// Case-insensitive extension match without allocating lowercase strings.
pub(crate) fn matches_ignore_ascii(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

// Checks a path extension against the asset-specific allowlist.
fn path_has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extensions
                .iter()
                .any(|expected| matches_ignore_ascii(extension, expected))
        })
}
