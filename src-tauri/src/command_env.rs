// This module normalizes child process environment for commands launched from the app.
// Packaged macOS apps inherit a minimal Finder PATH, so build tools installed by
// Homebrew or MacPorts need to be surfaced explicitly before command lookup.
use crate::runtime_info::is_flatpak_runtime;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// Builds a Command with platform-specific PATH fixes applied. The program parameter is the
// executable name or path to run, and the returned Command is ready for args/current_dir.
pub(crate) fn platform_command(program: &str) -> Command {
    let mut command = Command::new(resolve_program(program));

    if cfg!(target_os = "macos") {
        command.env("PATH", macos_command_path());
    }

    command
}

// Builds a Command that runs from a specific working directory.
pub(crate) fn platform_command_in_dir(program: &str, directory: &Path) -> Command {
    let mut command = platform_command(program);
    command.current_dir(directory);
    command
}

// Opens a folder/file path with the platform file manager or portal-backed opener.
pub(crate) fn open_path(path: &Path, label: &str) -> Result<(), String> {
    let attempts = open_path_attempts(path);
    let mut errors = Vec::new();

    for mut command in attempts {
        match command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => errors.push(format!("opener exited with status {status}")),
            Err(error) => errors.push(error.to_string()),
        }
    }

    Err(format!("Could not open {label}: {}", errors.join("; ")))
}

// Builds the platform-specific opener command list for a file or folder path.
// The path parameter is passed as an argument, and the returned commands are tried in order.
fn open_path_attempts(path: &Path) -> Vec<Command> {
    if cfg!(target_os = "windows") {
        let mut command = platform_command("explorer");
        command.arg(path);
        return vec![command];
    }

    if cfg!(target_os = "macos") {
        let mut command = platform_command("open");
        command.arg(path);
        return vec![command];
    }

    let mut attempts = Vec::new();

    // Flatpak file-manager launches should happen on the host so selected home or SD-card
    // folders open in the user's desktop session instead of inside the launcher sandbox.
    if is_flatpak_runtime() {
        let mut host_open = platform_command("flatpak-spawn");
        host_open.arg("--host").arg("xdg-open").arg(path);
        attempts.push(host_open);
    }

    let mut xdg_open = linux_host_opener_command("xdg-open");
    xdg_open.arg(path);
    attempts.push(xdg_open);

    let mut gio_open = linux_host_opener_command("gio");
    gio_open.arg("open").arg(path);
    attempts.push(gio_open);

    attempts
}

// Builds Linux file-manager opener commands against the host environment instead of the
// AppImage runtime. The program parameter is a trusted opener name, and the returned
// command avoids inherited AppImage library paths that can break KDE/Dolphin on Steam Deck.
fn linux_host_opener_command(program: &str) -> Command {
    let mut command = Command::new(
        linux_host_program_path(program).unwrap_or_else(|| OsString::from(program)),
    );
    sanitize_appimage_child_env(&mut command);
    command
}

// Finds common host locations before falling back to PATH lookup. AppImage prepends
// $APPDIR/usr/bin to PATH, so absolute host paths avoid accidentally launching a
// bundled helper with host desktop arguments.
fn linux_host_program_path(program: &str) -> Option<OsString> {
    ["/usr/bin", "/bin", "/usr/local/bin"]
        .iter()
        .map(|directory| Path::new(directory).join(program))
        .find(|candidate| candidate.is_file())
        .map(|path| path.into_os_string())
}

// Removes AppImage loader variables from host opener children. The launcher itself needs
// these variables for bundled WebKitGTK, but host file managers should load host libraries.
fn sanitize_appimage_child_env(command: &mut Command) {
    for key in ["APPDIR", "APPIMAGE", "ARGV0", "OWD", "LD_LIBRARY_PATH"] {
        command.env_remove(key);
    }
}

// Resolves bare macOS tool names through the augmented PATH before std::process performs lookup.
// The program parameter is left unchanged when it already contains a path separator.
fn resolve_program(program: &str) -> OsString {
    if !cfg!(target_os = "macos") || program.contains('/') || program.contains('\\') {
        return OsString::from(program);
    }

    macos_search_paths()
        .into_iter()
        .map(|path| path.join(program))
        .find(|candidate| candidate.is_file())
        .map(|path| path.into_os_string())
        .unwrap_or_else(|| OsString::from(program))
}

// Returns an augmented PATH that includes the common package-manager locations hidden from
// Finder-launched apps. It preserves the inherited PATH after the known tool locations.
#[cfg(target_os = "macos")]
fn macos_command_path() -> OsString {
    let paths = macos_search_paths();

    env::join_paths(paths)
        .unwrap_or_else(|_| env::var_os("PATH").unwrap_or_else(|| OsString::from("")))
}

// Returns the unchanged PATH on non-macOS platforms where command discovery should remain native.
#[cfg(not(target_os = "macos"))]
fn macos_command_path() -> OsString {
    env::var_os("PATH").unwrap_or_else(|| OsString::from(""))
}

// Produces macOS search paths with Homebrew, MacPorts, and the inherited PATH de-duplicated.
fn macos_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    for path in [
        "/opt/homebrew/bin",
        "/opt/homebrew/opt/sdl2/bin",
        "/usr/local/bin",
        "/usr/local/opt/sdl2/bin",
        "/opt/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ] {
        push_unique_path(&mut paths, PathBuf::from(path));
    }

    if let Some(current_path) = env::var_os("PATH") {
        for path in env::split_paths(&current_path) {
            push_unique_path(&mut paths, path);
        }
    }

    paths
}

// Adds a path once so the final PATH remains predictable and avoids repeated directories.
fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}
