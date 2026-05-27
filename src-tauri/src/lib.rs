// This library crate wires the Tauri window to focused backend modules for discovery,
// environment checks, and controlled setup/build actions.
mod actions;
mod asset_builds;
mod bundled_tools;
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
mod rom_storage;

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
            actions::clone_custom_project,
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
            ini_config::read_zelda_ini,
            ini_config::update_zelda_ini_line
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Z3R launcher");
}
