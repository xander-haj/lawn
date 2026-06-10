// This module normalizes child process environment for commands launched from the app.
// Packaged macOS apps inherit a minimal Finder PATH, so build tools installed by
// Homebrew or MacPorts need to be surfaced explicitly before command lookup.
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

// Builds a Command with platform-specific PATH fixes applied. The program parameter is the
// executable name or path to run, and the returned Command is ready for args/current_dir.
pub(crate) fn platform_command(program: &str) -> Command {
    if is_flatpak_runtime() {
        return flatpak_host_command(program, None);
    }

    let mut command = Command::new(resolve_program(program));

    if cfg!(target_os = "macos") {
        command.env("PATH", macos_command_path());
    }

    command
}

// Builds a Command that runs from a specific working directory. Flatpak host spawning needs the
// directory passed as a flatpak-spawn option, while native launches use Command::current_dir.
pub(crate) fn platform_command_in_dir(program: &str, directory: &Path) -> Command {
    if is_flatpak_runtime() {
        return flatpak_host_command(program, Some(directory));
    }

    let mut command = platform_command(program);
    command.current_dir(directory);
    command
}

// Detects Linux Flatpak packaging so launcher-managed tools run on the host toolchain.
#[cfg(target_os = "linux")]
fn is_flatpak_runtime() -> bool {
    Path::new("/.flatpak-info").is_file()
}

// Non-Linux packages should keep their native command behavior.
#[cfg(not(target_os = "linux"))]
fn is_flatpak_runtime() -> bool {
    false
}

// Creates a flatpak-spawn command that preserves the requested host program and cwd.
fn flatpak_host_command(program: &str, directory: Option<&Path>) -> Command {
    let mut command = Command::new("flatpak-spawn");

    command.arg("--host");

    if let Some(directory) = directory {
        let mut directory_arg = OsString::from("--directory=");
        directory_arg.push(directory.as_os_str());
        command.arg(directory_arg);
    }

    command.arg(program);
    command
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
