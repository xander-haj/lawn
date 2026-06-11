// This module defines serializable data contracts shared by the Rust backend and
// JavaScript frontend.
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct AppScan {
    pub launcher_parent: String,
    pub candidates: Vec<ProjectCandidate>,
    pub groups: Vec<ProjectScanGroup>,
}

// One ordered scan section returned to the home screen. The frontend uses these
// groups to render the same top-down order that the path manager stores.
#[derive(Serialize)]
pub struct ProjectScanGroup {
    pub label: String,
    pub path: String,
    pub is_default: bool,
    pub candidates: Vec<ProjectCandidate>,
}

#[derive(Clone, Serialize)]
pub struct ProjectCandidate {
    pub name: String,
    // Set when the project was discovered inside a nested {owner}/{repo} layout used by
    // custom forks. Top-level folders (e.g. the canonical Z3R clone) keep this as None.
    pub owner: Option<String>,
    pub path: String,
    pub asset_path: Option<String>,
    pub executable_path: Option<String>,
    pub git_repo: bool,
    pub snesrev_makefile_patch_applied: bool,
    pub snesrev_solution_patch_applied: bool,
    pub source_patch_needed: Option<String>,
    pub status: String,
    pub notes: Vec<String>,
}

#[derive(Serialize)]
pub struct EnvironmentReport {
    pub os: String,
    pub parent_path: String,
    pub checks: Vec<EnvironmentCheck>,
    pub next_steps: Vec<String>,
}

#[derive(Serialize)]
pub struct EnvironmentCheck {
    pub id: String,
    pub label: String,
    pub state: String,
    pub detail: String,
}

#[derive(Serialize)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Serialize)]
pub struct AppRuntimeInfo {
    pub os: String,
    pub default_scan_root: String,
    pub flatpak: bool,
    pub packaged_macos: bool,
    pub default_clone_requires_scan_path: bool,
    pub default_clone_warning: Option<String>,
}

#[derive(Serialize)]
pub struct RepoUpdatePreview {
    pub project_path: String,
    pub upstream: Option<String>,
    pub behind_count: usize,
    pub changes: Vec<RepoChange>,
    pub warnings: Vec<String>,
    pub dirty_files: Vec<String>,
    pub can_apply: bool,
}

#[derive(Clone, Serialize)]
pub struct RepoChange {
    pub path: String,
    pub old_path: Option<String>,
    pub status: String,
    pub label: String,
}

#[derive(Serialize)]
pub struct FeatureAssetReport {
    pub storage_dir: String,
    pub msu_download_url: String,
    pub sprites_source_url: String,
    pub shaders_source_url: String,
    pub msu: FeatureAssetGroup,
    pub sprites: FeatureAssetGroup,
    pub shaders: FeatureAssetGroup,
}

#[derive(Serialize)]
pub struct FeatureAssetGroup {
    pub available: bool,
    pub project_available: bool,
    pub shared_available: bool,
    pub options: Vec<FeatureAssetOption>,
}

#[derive(Serialize)]
pub struct FeatureAssetOption {
    pub label: String,
    pub value: String,
    pub source: String,
}

#[derive(Serialize)]
pub struct SpritePreviewData {
    pub label: String,
    pub pixel_data: Vec<u8>,
    pub palette_data: Vec<u8>,
}

// Launcher-managed ROM storage status returned to the home screen.
#[derive(Serialize)]
pub struct RomStatus {
    pub available: bool,
    pub file_name: Option<String>,
    pub path: Option<String>,
    pub storage_dir: String,
}

#[derive(Serialize)]
pub struct RandomizerSetupReport {
    pub project_path: String,
    pub available: bool,
    pub item_options: Vec<RandomizerItemOption>,
    pub config_files: Vec<RandomizerConfigFile>,
}

#[derive(Serialize)]
pub struct RandomizerItemOption {
    pub id: u8,
    pub label: String,
    pub count: usize,
    pub detail: String,
}

#[derive(Serialize)]
pub struct RandomizerConfigFile {
    pub label: String,
    pub state: String,
    pub detail: String,
}

#[derive(Deserialize)]
pub struct RandomizerRunOptions {
    pub mode: Option<String>,
    pub seed: Option<String>,
    pub dry_run: bool,
    pub no_spoiler: bool,
    pub include_small_keys: bool,
    pub include_big_chests: bool,
    pub exclude_rooms: Option<String>,
    pub exclude_locations: Option<String>,
    pub exclude_items: Option<String>,
    pub exclude_categories: Option<String>,
}

// Per-project zelda3.ini snapshot returned to the frontend. Only the lines that the
// card widgets, controls screen, and feature toggles need are surfaced; non-editable
// lines (pure comments, blanks, section headers, and lines from sections the launcher
// does not edit) are intentionally omitted to keep the JSON payload small.
#[derive(Serialize)]
pub struct ZeldaIniSnapshot {
    pub project_path: String,
    pub aspect_ratio: AspectRatioState,
    pub graphics_lines: Vec<IniLineSnapshot>,
    pub sound_lines: Vec<IniLineSnapshot>,
    pub feature_lines: Vec<IniLineSnapshot>,
    pub keymap_lines: Vec<IniLineSnapshot>,
    pub gamepad_lines: Vec<IniLineSnapshot>,
}

// Derived view of the two ini fields the per-card Aspect Ratio widget binds to:
// [General] ExtendedAspectRatio and [Graphics] WindowSize. The frontend parses the
// raw_value into ratio + flag checkboxes itself so the backend stays format-agnostic.
#[derive(Serialize)]
pub struct AspectRatioState {
    pub line_number: usize,
    pub raw_value: String,
    pub window_size_line: usize,
    pub window_size_value: String,
}

// One editable line in zelda3.ini. line_number is 1-based and used as the address for
// subsequent update_zelda_ini_line calls. `raw` is the file's original line text so
// the frontend can show the original whitespace if it ever wants to.
#[derive(Serialize)]
pub struct IniLineSnapshot {
    pub line_number: usize,
    pub section: String,
    pub key: String,
    pub value: String,
    pub commented: bool,
    pub raw: String,
}
