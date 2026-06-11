// This module owns the launcher update command. Downloading and installer handoff
// live in helper modules so each platform path stays readable and bounded.
use crate::command_env::platform_command;
use crate::launcher_update_downloads::{
    compare_versions, download_release_asset, exact_asset, fetch_latest_release, update_result,
    update_work_dir, GithubRelease,
};
use crate::launcher_update_installers::{
    checked_output, current_appimage_path, current_executable_path, current_macos_bundle_path,
    make_executable, schedule_app_exit, spawn_detached, spawn_flatpak_relaunch,
    write_appimage_update_script, write_macos_update_script, write_windows_update_script,
};
use crate::models::ActionResult;
use crate::paths::display_path;
use crate::runtime_info::is_flatpak_runtime;
use std::cmp::Ordering;
use std::fs;
use std::path::Path;

const WINDOWS_SETUP_ASSET: &str = "Z3R-Launcher-windows-x64-setup.exe";
const MACOS_DMG_ASSET: &str = "Z3R-Launcher-macos-universal.dmg";
const LINUX_APPIMAGE_ASSET: &str = "Z3R-Launcher-linux-x64.AppImage";
const FLATPAK_BUNDLE_ASSET: &str = "Z3R-Launcher-linux.flatpak";
const FLATPAK_INFO_PATH: &str = "/.flatpak-info";

// Checks the latest release, downloads the right package for the current install type,
// starts the updater/installer, and exits the launcher when replacement requires it.
#[tauri::command]
pub fn install_launcher_update(app: tauri::AppHandle) -> Result<ActionResult, String> {
    let current_version = current_update_version();
    let update_dir = update_work_dir();
    fs::create_dir_all(&update_dir).map_err(|error| {
        format!(
            "Could not create update folder {}: {error}",
            display_path(&update_dir)
        )
    })?;

    let release = fetch_latest_release(&update_dir)?;
    match compare_versions(&release.tag_name, current_version) {
        Ordering::Less => {
            return Ok(update_result(
                true,
                format!(
                    "Launcher {current_version} is newer than the latest published release {}.",
                    release.tag_name
                ),
                "",
                "",
            ));
        }
        Ordering::Equal => {
            return Ok(update_result(
                true,
                format!("Launcher is already up to date ({current_version})."),
                "",
                "",
            ));
        }
        Ordering::Greater => {}
    }

    if is_flatpak_runtime() {
        return install_flatpak_update(&app, &release, &update_dir);
    }

    if cfg!(target_os = "windows") {
        return install_windows_update(app, &release, &update_dir);
    }

    if cfg!(target_os = "macos") {
        return install_macos_update(app, &release, &update_dir);
    }

    if cfg!(target_os = "linux") {
        return install_appimage_update(app, &release, &update_dir);
    }

    Err("Launcher updates are not packaged for this operating system yet.".to_string())
}

// Windows updates are applied by the NSIS setup exe, which is the only Windows asset
// selected here because it carries the bundled Git, Python, SDL2, and TCC resources.
fn install_windows_update(
    app: tauri::AppHandle,
    release: &GithubRelease,
    update_dir: &Path,
) -> Result<ActionResult, String> {
    let asset = exact_asset(release, WINDOWS_SETUP_ASSET)?;
    let installer = download_release_asset(&asset, update_dir)?;
    let script_path = update_dir.join("apply-windows-update.ps1");
    let log_path = update_dir.join("apply-windows-update.log");
    let relaunch_path = current_executable_path()?;
    let launcher_pid = std::process::id().to_string();
    write_windows_update_script(&script_path)?;

    let mut command = platform_command("powershell.exe");
    command
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script_path)
        .args(["-LauncherPid", &launcher_pid, "-Installer"])
        .arg(&installer)
        .arg("-Relaunch")
        .arg(&relaunch_path)
        .arg("-Log")
        .arg(&log_path);
    spawn_detached(command, "Windows launcher setup handoff")?;
    schedule_app_exit(app);

    Ok(update_result(
        true,
        format!(
            "Launcher update {} downloaded and setup started. The launcher will close so the setup exe can replace it.",
            release.tag_name
        ),
        &format!(
            "Installer: {}\nUpdater log: {}",
            display_path(&installer),
            display_path(&log_path)
        ),
        "",
    ))
}

// macOS updates mount the release DMG after the current app exits, copy the new .app
// over the running bundle, remove quarantine metadata when possible, and relaunch it.
fn install_macos_update(
    app: tauri::AppHandle,
    release: &GithubRelease,
    update_dir: &Path,
) -> Result<ActionResult, String> {
    let bundle_path = current_macos_bundle_path()?;
    let asset = exact_asset(release, MACOS_DMG_ASSET)?;
    let dmg_path = download_release_asset(&asset, update_dir)?;
    let script_path = update_dir.join("apply-macos-update.sh");
    let mount_path = update_dir.join("macos-dmg-mount");
    let log_path = update_dir.join("apply-macos-update.log");
    let app_name = bundle_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Could not determine the current macOS app bundle name.".to_string())?;

    write_macos_update_script(&script_path)?;
    make_executable(&script_path)?;

    let mut command = platform_command("/bin/sh");
    command
        .arg(&script_path)
        .arg(std::process::id().to_string())
        .arg(&dmg_path)
        .arg(&mount_path)
        .arg(&bundle_path)
        .arg(app_name)
        .arg(&log_path);
    spawn_detached(command, "macOS launcher updater")?;
    schedule_app_exit(app);

    Ok(update_result(
        true,
        format!(
            "Launcher update {} downloaded. The launcher will close, replace the app bundle, and reopen.",
            release.tag_name
        ),
        &format!("Updater log: {}", display_path(&log_path)),
        "",
    ))
}

// Flatpak updates download the release bundle and ask the host Flatpak installation
// to install-or-update it. This avoids the uninstall-first behavior that can fail
// with "Directory not empty" while the current Flatpak is still running.
fn install_flatpak_update(
    app: &tauri::AppHandle,
    release: &GithubRelease,
    update_dir: &Path,
) -> Result<ActionResult, String> {
    let asset = exact_asset(release, FLATPAK_BUNDLE_ASSET)?;
    let bundle = download_release_asset(&asset, update_dir)?;
    let scope_arg = flatpak_install_scope_arg();
    let mut command = platform_command("flatpak-spawn");
    command.args([
        "--host",
        "flatpak",
        "install",
        scope_arg,
        "--or-update",
        "--assumeyes",
        "--noninteractive",
        &display_path(&bundle),
    ]);
    let output = checked_output(command, "Flatpak launcher install")?;
    spawn_flatpak_relaunch()?;
    schedule_app_exit(app.clone());

    Ok(update_result(
        true,
        format!(
            "Launcher update {} installed through Flatpak. The launcher will close and reopen.",
            release.tag_name
        ),
        &output.0,
        &output.1,
    ))
}

// Uses the CI-stamped release tag when available, falling back to Cargo's version
// for local builds and older packages that predate release metadata stamping.
fn current_update_version() -> &'static str {
    option_env!("LAUNCHER_RELEASE_TAG")
        .filter(|tag| !tag.trim().is_empty())
        .unwrap_or(env!("CARGO_PKG_VERSION"))
}

// Matches the host Flatpak install scope to the running package whenever /.flatpak-info
// exposes an app path. Steam Deck/user installs stay --user; system installs use --system.
fn flatpak_install_scope_arg() -> &'static str {
    let Ok(flatpak_info) = fs::read_to_string(FLATPAK_INFO_PATH) else {
        return "--user";
    };

    for line in flatpak_info.lines() {
        let Some(app_path) = line.strip_prefix("app-path=") else {
            continue;
        };

        if app_path.contains("/.local/share/flatpak/") {
            return "--user";
        }

        if app_path.contains("/var/lib/flatpak/") {
            return "--system";
        }
    }

    "--user"
}

// AppImage updates replace the running AppImage file after the current process exits
// and then relaunch the newly downloaded executable.
fn install_appimage_update(
    app: tauri::AppHandle,
    release: &GithubRelease,
    update_dir: &Path,
) -> Result<ActionResult, String> {
    let current_appimage = current_appimage_path()?;
    let asset = exact_asset(release, LINUX_APPIMAGE_ASSET)?;
    let downloaded_appimage = download_release_asset(&asset, update_dir)?;
    let script_path = update_dir.join("apply-appimage-update.sh");
    let log_path = update_dir.join("apply-appimage-update.log");

    write_appimage_update_script(&script_path)?;
    make_executable(&script_path)?;

    let mut command = platform_command("/bin/sh");
    command
        .arg(&script_path)
        .arg(std::process::id().to_string())
        .arg(&downloaded_appimage)
        .arg(&current_appimage)
        .arg(&log_path);
    spawn_detached(command, "AppImage launcher updater")?;
    schedule_app_exit(app);

    Ok(update_result(
        true,
        format!(
            "Launcher update {} downloaded. The launcher will close, replace the AppImage, and reopen.",
            release.tag_name
        ),
        &format!("Updater log: {}", display_path(&log_path)),
        "",
    ))
}
