// This helper module contains installer process handoff and generated replacement
// scripts used after an update package has already been downloaded.
use crate::command_env::platform_command;
use crate::launcher_update_downloads::describe_output_failure;
use crate::paths::display_path;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const FLATPAK_APP_ID: &str = "io.github.xander_haj.Z3RLauncher";

// Runs an installer command and returns stdout/stderr when it succeeds.
pub(crate) fn checked_output(mut command: Command, label: &str) -> Result<(String, String), String> {
    let output = command
        .output()
        .map_err(|error| format!("{label} could not start: {error}"))?;

    if output.status.success() {
        return Ok((
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Err(describe_output_failure(label, &output))
}

// Starts a long-running installer/update script without tying it to the Tauri process.
pub(crate) fn spawn_detached(mut command: Command, label: &str) -> Result<(), String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("{label} could not start: {error}"))?;

    Ok(())
}

// Lets the frontend receive the command result before closing the app for replacement.
pub(crate) fn schedule_app_exit(app: tauri::AppHandle) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(1200));
        app.exit(0);
    });
}

// Starts a host-side relaunch after the Flatpak process closes.
pub(crate) fn spawn_flatpak_relaunch() -> Result<(), String> {
    let mut command = platform_command("flatpak-spawn");
    let launcher_pid = std::process::id().to_string();
    command.args([
        "--host",
        "sh",
        "-c",
        "while kill -0 \"$1\" 2>/dev/null; do sleep 1; done; flatpak run \"$2\"",
        "z3r-launcher-flatpak-relaunch",
        &launcher_pid,
        FLATPAK_APP_ID,
    ]);
    spawn_detached(command, "Flatpak launcher relaunch")
}

// Resolves the running macOS .app bundle from Contents/MacOS/{binary}.
pub(crate) fn current_macos_bundle_path() -> Result<PathBuf, String> {
    let executable = env::current_exe()
        .map_err(|error| format!("Could not locate the running launcher executable: {error}"))?;
    let app_bundle = executable
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| "Could not locate the current macOS app bundle.".to_string())?;

    if app_bundle
        .extension()
        .is_some_and(|extension| extension.to_string_lossy() == "app")
    {
        return Ok(app_bundle.to_path_buf());
    }

    Err("macOS self-update requires running from the packaged .app bundle.".to_string())
}

// Resolves the AppImage file that launched this process.
pub(crate) fn current_appimage_path() -> Result<PathBuf, String> {
    let appimage = env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .ok_or_else(|| {
            "Linux self-update requires running the AppImage or Flatpak package.".to_string()
        })?;

    if appimage.is_file() {
        return Ok(appimage);
    }

    Err(format!(
        "The APPIMAGE path does not exist anymore: {}",
        display_path(&appimage)
    ))
}

// Resolves the currently running executable so installer scripts can relaunch the updated app.
pub(crate) fn current_executable_path() -> Result<PathBuf, String> {
    env::current_exe()
        .map_err(|error| format!("Could not locate the running launcher executable: {error}"))
}

// Writes the Windows handoff script that waits for this process before running the setup exe.
pub(crate) fn write_windows_update_script(path: &Path) -> Result<(), String> {
    let script = r#"
param(
  [Parameter(Mandatory = $true)][int]$LauncherPid,
  [Parameter(Mandatory = $true)][string]$Installer,
  [Parameter(Mandatory = $true)][string]$Relaunch,
  [Parameter(Mandatory = $true)][string]$Log
)
$ErrorActionPreference = "Stop"
function Write-UpdateLog([string]$Message) {
  $stamp = Get-Date -Format o
  Add-Content -LiteralPath $Log -Value "$stamp $Message"
}
try {
  Write-UpdateLog "Waiting for launcher process $LauncherPid to close."
  Wait-Process -Id $LauncherPid -ErrorAction SilentlyContinue
  Write-UpdateLog "Starting launcher setup exe."
  $process = Start-Process -FilePath $Installer -ArgumentList "/S" -Wait -PassThru
  if ($process.ExitCode -ne 0) {
    throw "Setup exe exited with code $($process.ExitCode)."
  }
  if (Test-Path -LiteralPath $Relaunch) {
    Write-UpdateLog "Relaunching updated launcher."
    Start-Process -FilePath $Relaunch
  }
} catch {
  Write-UpdateLog $_.Exception.Message
  exit 1
}
"#;
    fs::write(path, script).map_err(|error| format!("Could not write Windows update script: {error}"))
}

// Writes the macOS replacement script. Shell work is isolated here so Rust never
// interpolates untrusted release data into command source.
pub(crate) fn write_macos_update_script(path: &Path) -> Result<(), String> {
    let script = r#"#!/bin/sh
set -eu
pid="$1"
dmg="$2"
mount="$3"
target="$4"
app_name="$5"
log="$6"
exec > "$log" 2>&1
while kill -0 "$pid" 2>/dev/null; do
  sleep 1
done
rm -rf "$mount"
mkdir -p "$mount"
hdiutil attach -nobrowse -quiet -mountpoint "$mount" "$dmg"
trap 'hdiutil detach "$mount" -quiet >/dev/null 2>&1 || true; rm -rf "$mount"' EXIT
source_app="$mount/$app_name"
if [ ! -d "$source_app" ]; then
  source_app="$(find "$mount" -maxdepth 2 -name '*.app' -type d | head -n 1)"
fi
if [ -z "$source_app" ] || [ ! -d "$source_app" ]; then
  echo "No app bundle was found in the mounted DMG."
  exit 2
fi
rm -rf "$target"
ditto "$source_app" "$target"
xattr -dr com.apple.quarantine "$target" >/dev/null 2>&1 || true
open "$target"
"#;
    fs::write(path, script).map_err(|error| format!("Could not write macOS update script: {error}"))
}

// Writes the AppImage replacement script that runs after the current process exits.
pub(crate) fn write_appimage_update_script(path: &Path) -> Result<(), String> {
    let script = r#"#!/bin/sh
set -eu
pid="$1"
downloaded="$2"
target="$3"
log="$4"
exec > "$log" 2>&1
while kill -0 "$pid" 2>/dev/null; do
  sleep 1
done
chmod +x "$downloaded"
tmp="${target}.updating"
mv "$downloaded" "$tmp"
mv "$tmp" "$target"
chmod +x "$target"
"$target" >/dev/null 2>&1 &
"#;
    fs::write(path, script).map_err(|error| format!("Could not write AppImage update script: {error}"))
}

// Marks generated update scripts executable on Unix platforms.
pub(crate) fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)
            .map_err(|error| format!("Could not read script permissions: {error}"))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .map_err(|error| format!("Could not mark update script executable: {error}"))?;
    }

    Ok(())
}
