// This library crate wires the Tauri window to focused backend modules for discovery,
// environment checks, and controlled setup/build actions.
mod actions;
mod asset_builds;
mod bundled_tools;
mod command_env;
mod discovery;
mod env_checks;
mod external_links;
mod feature_asset_catalog;
mod feature_asset_paths;
mod feature_asset_store;
mod feature_assets;
// Owns line-preserving read/write of project-local zelda3.ini files for the per-card
// aspect ratio widget and the Controls screen.
mod ini_config;
mod makefile_patches;
mod models;
mod paths;
mod randomizer;
mod repo_updates;
mod rom_storage;
mod runtime_info;

#[cfg(target_os = "linux")]
const LINUX_GDK_BACKEND: &str = "x11,wayland";
#[cfg(target_os = "linux")]
const LINUX_GTK_USE_PORTAL: &str = "1";
#[cfg(target_os = "linux")]
const LINUX_GIO_MODULE_DIR: &str = "/nonexistent";
#[cfg(target_os = "linux")]
const LINUX_GIO_VFS: &str = "local";
#[cfg(target_os = "linux")]
const LINUX_WEBKIT_DISABLE_COMPOSITING_MODE: &str = "1";
#[cfg(target_os = "linux")]
const LINUX_WEBKIT_DISABLE_DMABUF_RENDERER: &str = "1";

// Configures Linux process environment before GTK and WebKitGTK initialize. It accepts no
// parameters, returns nothing, and only changes this process plus children spawned by WebKitGTK.
#[cfg(target_os = "linux")]
fn configure_linux_webview_runtime() {
    let flatpak = crate::runtime_info::is_flatpak_runtime();

    // Prefer X11/XWayland for AppImage runs because Wayland EGL startup is crash-prone here.
    set_env_if_missing("GDK_BACKEND", LINUX_GDK_BACKEND);

    if flatpak {
        // Steam Deck/Flatpak file pickers should go through xdg-desktop-portal.
        std::env::set_var("GTK_USE_PORTAL", LINUX_GTK_USE_PORTAL);
    } else {
        // AppImage builds can load the host GVFS module against the bundled GLib/GIO version.
        std::env::set_var("GIO_USE_VFS", LINUX_GIO_VFS);
        // Keeping GIO away from host module directories avoids ABI mismatches in libgvfsdbus.so.
        std::env::set_var("GIO_MODULE_DIR", LINUX_GIO_MODULE_DIR);
        // User-level extra module paths can reintroduce the same host-module ABI mismatch.
        std::env::remove_var("GIO_EXTRA_MODULES");
    }

    // Disabling compositing keeps WebKitGTK off the EGL path that aborts on affected systems.
    std::env::set_var(
        "WEBKIT_DISABLE_COMPOSITING_MODE",
        LINUX_WEBKIT_DISABLE_COMPOSITING_MODE,
    );
    // WebKitGTK's DMABuf renderer can abort during EGL display creation on affected drivers.
    std::env::set_var(
        "WEBKIT_DISABLE_DMABUF_RENDERER",
        LINUX_WEBKIT_DISABLE_DMABUF_RENDERER,
    );
}

// Sets a process environment variable only when the user/session has not made an explicit choice.
// The key and value parameters are copied into this process environment, and the function returns
// nothing after preserving any existing override.
#[cfg(target_os = "linux")]
fn set_env_if_missing(key: &str, value: &str) {
    if std::env::var_os(key).is_none() {
        std::env::set_var(key, value);
    }
}

// Keeps the startup path identical on non-Linux platforms. It accepts no parameters, returns
// nothing, and has no side effects.
#[cfg(not(target_os = "linux"))]
fn configure_linux_webview_runtime() {}

// Starts Tauri and exposes only the launcher commands that the frontend needs.
pub fn run() {
    configure_linux_webview_runtime();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            discovery::scan_siblings,
            runtime_info::app_runtime_info,
            runtime_info::launcher_version,
            env_checks::check_environment,
            actions::launch_game,
            actions::choose_scan_root,
            actions::clone_project,
            actions::clone_custom_project,
            actions::open_project_folder,
            actions::create_venv,
            actions::install_dependencies,
            asset_builds::extract_assets,
            asset_builds::extract_assets_visual_studio,
            asset_builds::extract_assets_tcc,
            external_links::open_external_url,
            feature_assets::read_feature_assets,
            feature_assets::clone_feature_asset,
            feature_assets::choose_and_store_msu,
            feature_assets::store_msu_paths,
            feature_assets::install_feature_asset,
            feature_assets::read_sprite_preview,
            makefile_patches::apply_snesrev_makefile_patch,
            makefile_patches::apply_snesrev_solution_patch,
            rom_storage::stored_rom_status,
            rom_storage::choose_and_store_rom,
            rom_storage::open_stored_rom_folder,
            rom_storage::sync_stored_rom_to_projects,
            randomizer::read_randomizer_setup,
            randomizer::extract_randomizer_assets,
            randomizer::run_randomizer,
            randomizer::restore_vanilla_randomizer_yaml,
            randomizer::compile_randomized_assets,
            repo_updates::preview_repo_update,
            repo_updates::apply_repo_update,
            ini_config::read_zelda_ini,
            ini_config::update_zelda_ini_line
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Z3R launcher");
}
