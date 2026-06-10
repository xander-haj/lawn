// This module opens trusted documentation URLs in the user's default browser.
use crate::command_env::platform_command;
use std::process::Stdio;

// Opens an external documentation URL with the platform-native opener.
#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    if !is_safe_web_url(&url) {
        return Err("Only http and https documentation links can be opened.".to_string());
    }

    let mut command = if cfg!(target_os = "windows") {
        let mut command = platform_command("rundll32");
        command.args(["url.dll,FileProtocolHandler", &url]);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = platform_command("open");
        command.arg(&url);
        command
    } else {
        let mut command = platform_command("xdg-open");
        command.arg(&url);
        command
    };

    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not open documentation link: {error}"))?;

    Ok(())
}

// Restricts opener input to ordinary web links from the bundled guide JSON.
fn is_safe_web_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}
