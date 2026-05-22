// This library crate wires the Tauri window to focused backend modules for discovery,
// environment checks, and controlled setup/build actions.
mod actions;
mod discovery;
mod env_checks;
mod models;
mod paths;

// Starts Tauri and exposes only the launcher commands that the frontend needs.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            discovery::scan_siblings,
            env_checks::check_environment,
            actions::launch_game,
            actions::choose_scan_root,
            actions::clone_project,
            actions::create_venv,
            actions::install_dependencies,
            actions::extract_assets,
            actions::build_project
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Z3R launcher");
}
