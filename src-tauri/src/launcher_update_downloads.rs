// This helper module fetches GitHub release metadata and downloads update assets
// through bounded platform tools instead of the WebView networking stack.
use crate::command_env::platform_command;
use crate::models::ActionResult;
use crate::paths::display_path;
use serde::Deserialize;
use std::cmp::Ordering;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const RELEASE_API_URL: &str = "https://api.github.com/repos/xander-haj/lawn/releases/latest";

#[derive(Deserialize)]
pub(crate) struct GithubRelease {
    pub(crate) tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct GithubAsset {
    name: String,
    browser_download_url: String,
}

// Downloads and parses the latest GitHub release JSON through system download tools.
pub(crate) fn fetch_latest_release(update_dir: &Path) -> Result<GithubRelease, String> {
    let release_json = update_dir.join("latest-release.json");
    download_url_to_file(RELEASE_API_URL, &release_json, true)?;
    let body = fs::read_to_string(&release_json)
        .map_err(|error| format!("Could not read GitHub release metadata: {error}"))?;
    let release: GithubRelease = serde_json::from_str(&body)
        .map_err(|error| format!("Could not parse GitHub release metadata: {error}"))?;

    if release.tag_name.trim().is_empty() {
        return Err("GitHub returned a release without a tag name.".to_string());
    }

    Ok(release)
}

// Downloads one release asset into the update work directory and returns its local path.
pub(crate) fn download_release_asset(
    asset: &GithubAsset,
    update_dir: &Path,
) -> Result<PathBuf, String> {
    let file_name = asset_file_name(asset)?;
    let target = update_dir.join(file_name);
    download_url_to_file(&asset.browser_download_url, &target, false)?;
    Ok(target)
}

// Finds an exact release asset by name so update channels cannot silently swap package types.
pub(crate) fn exact_asset(release: &GithubRelease, name: &str) -> Result<GithubAsset, String> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .cloned()
        .ok_or_else(|| {
            let available = release
                .assets
                .iter()
                .map(|asset| asset.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Release {} does not include required update asset {name}. Available assets: {available}.",
                release.tag_name
            )
        })
}

// Writes a URL to disk by trying curl, wget, and on Windows PowerShell with bounded timeouts.
fn download_url_to_file(url: &str, destination: &Path, github_api: bool) -> Result<(), String> {
    let partial = destination.with_extension("download");
    let _ = fs::remove_file(&partial);
    let _ = fs::remove_file(destination);
    let mut errors = Vec::new();

    for (label, mut command) in download_commands(url, &partial, github_api) {
        match command.output() {
            Ok(output) if output.status.success() => {
                fs::rename(&partial, destination)
                    .map_err(|error| format!("Could not store downloaded update file: {error}"))?;
                return Ok(());
            }
            Ok(output) => errors.push(describe_output_failure(label, &output)),
            Err(error) => errors.push(format!("{label} could not start: {error}")),
        }
    }

    Err(format!("Could not download update file: {}", errors.join("; ")))
}

// Builds the bounded downloader command list. The first available command that succeeds wins.
fn download_commands(url: &str, destination: &Path, github_api: bool) -> Vec<(&'static str, Command)> {
    let mut commands = Vec::new();
    let mut curl = platform_command(if cfg!(target_os = "windows") {
        "curl.exe"
    } else {
        "curl"
    });
    curl.args([
        "--location",
        "--fail",
        "--show-error",
        "--silent",
        "--retry",
        "4",
        "--retry-delay",
        "2",
        "--connect-timeout",
        "20",
        "--max-time",
        "300",
    ]);

    if github_api {
        curl.args([
            "--header",
            "Accept: application/vnd.github+json",
            "--user-agent",
            "Z3R-Launcher-Updater",
        ]);
    }

    curl.arg("--output").arg(destination).arg(url);
    commands.push(("curl", curl));

    let mut wget = platform_command("wget");
    wget.args(["--tries=4", "--timeout=30"]);

    if github_api {
        wget.arg("--header=Accept: application/vnd.github+json");
        wget.arg("--user-agent=Z3R-Launcher-Updater");
    }

    wget.arg("-O").arg(destination).arg(url);
    commands.push(("wget", wget));

    if cfg!(target_os = "windows") {
        let mut powershell = platform_command("powershell.exe");
        powershell.args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "$ProgressPreference = 'SilentlyContinue'; Invoke-WebRequest -UseBasicParsing \
             -MaximumRedirection 5 -TimeoutSec 300 -Uri $args[0] -OutFile $args[1]",
        ]);
        powershell.arg(url).arg(destination);
        commands.push(("PowerShell Invoke-WebRequest", powershell));
    }

    commands
}

// Chooses an update workspace that both the app and platform installer can access.
pub(crate) fn update_work_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local_app_data)
                .join("Z3R Launcher")
                .join("updates");
        }
    }

    if cfg!(target_os = "macos") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Caches")
                .join("Z3R Launcher")
                .join("updates");
        }
    }

    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("z3r-launcher")
            .join("updates");
    }

    env::temp_dir().join("z3r-launcher-updates")
}

// Produces the shared ActionResult shape used by the rest of the launcher.
pub(crate) fn update_result(ok: bool, message: String, stdout: &str, stderr: &str) -> ActionResult {
    ActionResult {
        ok,
        message,
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    }
}

// Compares tags like v0.1.2 against Cargo versions without depending on semver.
pub(crate) fn compare_versions(left: &str, right: &str) -> Ordering {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    let max_len = left_parts.len().max(right_parts.len());

    for index in 0..max_len {
        let left_value = *left_parts.get(index).unwrap_or(&0);
        let right_value = *right_parts.get(index).unwrap_or(&0);

        match left_value.cmp(&right_value) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }

    Ordering::Equal
}

// Keeps asset filenames path-local even if a release ever reports a malformed name.
fn asset_file_name(asset: &GithubAsset) -> Result<&str, String> {
    Path::new(&asset.name)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("Release asset has an invalid filename: {}", asset.name))
}

// Renders a command failure with stderr first, falling back to stdout when needed.
pub(crate) fn describe_output_failure(label: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if stderr.is_empty() { stdout } else { stderr };

    if detail.is_empty() {
        format!("{label} exited with status {}", output.status)
    } else {
        format!("{label} exited with status {}: {detail}", output.status)
    }
}

// Extracts digit groups from version text so v1.2.3 and release-1.2.3 compare the same.
fn version_parts(value: &str) -> Vec<u64> {
    let mut parts = Vec::new();
    let mut current = String::new();

    for character in value.chars() {
        if character.is_ascii_digit() {
            current.push(character);
        } else if !current.is_empty() {
            parts.push(current.parse::<u64>().unwrap_or(0));
            current.clear();
        }
    }

    if !current.is_empty() {
        parts.push(current.parse::<u64>().unwrap_or(0));
    }

    if parts.is_empty() {
        parts.push(0);
    }

    parts
}
