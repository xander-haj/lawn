// This module reads and runs the project-local randomizer workflow without changing launcher scan roots.
use crate::actions::run_command;
use crate::models::{
    ActionResult, RandomizerConfigFile, RandomizerItemOption, RandomizerRunOptions,
    RandomizerSetupReport,
};
use crate::paths::{display_path, venv_python};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

// Reads randomizer instructions and required file status from the selected Z3R project.
#[tauri::command]
pub fn read_randomizer_setup(project_path: String) -> Result<RandomizerSetupReport, String> {
    let project = PathBuf::from(project_path);
    let entry_path = project.join("assets").join("restool-randomize.py");
    let engine_path = project.join("assets").join("randomizer.py");
    let config_files = vec![
        file_status("Randomizer CLI", &entry_path),
        capability_status(
            "Safe Mode support",
            &entry_path,
            "--mode",
            "Update the selected Z3R folder's randomizer scripts before using Safe Mode.",
        ),
        file_status("Randomizer engine", &engine_path),
        file_status(
            "Vanilla masterlist",
            &project.join("assets").join("randomizer-masterlist.json"),
        ),
        folder_status(
            "Dungeon YAML",
            &project.join("assets").join("dungeon"),
            "Extract assets before randomizing if this folder is missing.",
        ),
        folder_status(
            "Spoiler logs",
            &project.join("assets").join("randomizer-spoilers"),
            "Created automatically when randomizer runs with spoiler output enabled.",
        ),
    ];

    Ok(RandomizerSetupReport {
        project_path: display_path(&project),
        available: entry_path.is_file() && engine_path.is_file(),
        item_options: read_item_options(&project.join("assets").join("randomizer-masterlist.json")),
        config_files,
    })
}

// Extracts clean asset YAML without compiling, then generates the vanilla randomizer masterlist.
#[tauri::command]
pub fn extract_randomizer_assets(project_path: String) -> Result<ActionResult, String> {
    let project = PathBuf::from(project_path);
    let python = project_python(&project)?;
    let extract = run_command(
        &display_path(&python),
        &["assets/restool.py", "--extract-from-rom", "--no-build"],
        &project,
        "Randomizer asset extraction complete.",
    )?;

    if !extract.ok {
        return Ok(extract);
    }

    let masterlist = run_command(
        &display_path(&python),
        &["assets/restool-randomize.py", "--generate-masterlist"],
        &project,
        "Randomizer masterlist generated.",
    )?;

    Ok(combine_results(
        "Randomizer assets extracted and vanilla masterlist generated.",
        extract,
        masterlist,
    ))
}

// Runs assets/restool-randomize.py with fixed arguments built from the setup form.
#[tauri::command]
pub fn run_randomizer(
    project_path: String,
    options: RandomizerRunOptions,
) -> Result<ActionResult, String> {
    let project = PathBuf::from(project_path);
    let python = project_python(&project)?;
    let entry_path = project.join("assets").join("restool-randomize.py");
    let requested_mode = options.mode.as_deref().unwrap_or("safe");

    if requested_mode == "safe" && !file_contains(&entry_path, "--mode") {
        return Err(
            "The selected Z3R folder's randomizer CLI does not support Safe Mode yet. Update \
             assets/restool-randomize.py and assets/randomizer.py in that folder, then try again."
                .replace('\n', " "),
        );
    }

    let mut args = vec!["assets/restool-randomize.py".to_string()];

    push_option(&mut args, "--mode", options.mode);
    push_option(&mut args, "--seed", options.seed);

    if options.dry_run {
        args.push("--dry-run".to_string());
    }

    if options.no_spoiler {
        args.push("--no-spoiler".to_string());
    }

    if options.include_small_keys {
        args.push("--include-small-keys".to_string());
    }

    if options.include_big_chests {
        args.push("--include-big-chests".to_string());
    }

    push_option(&mut args, "--exclude-room", options.exclude_rooms);
    push_option(&mut args, "--exclude-location", options.exclude_locations);
    push_option(&mut args, "--exclude-item", options.exclude_items);
    push_option(&mut args, "--exclude-category", options.exclude_categories);

    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_command(
        &display_path(&python),
        &arg_refs,
        &project,
        "Randomizer run complete.",
    )
}

// Reads unique item ids from the masterlist so the frontend can render clickable item exclusions.
fn read_item_options(masterlist_path: &Path) -> Vec<RandomizerItemOption> {
    let Ok(contents) = fs::read_to_string(masterlist_path) else {
        return Vec::new();
    };
    let Ok(manifest) = serde_json::from_str::<Value>(&contents) else {
        return Vec::new();
    };
    let Some(locations) = manifest.get("locations").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut counts = BTreeMap::new();
    for id in locations
        .iter()
        .filter_map(|entry| entry.get("item").and_then(Value::as_u64))
        .filter_map(|item| u8::try_from(item).ok())
    {
        *counts.entry(id).or_insert(0usize) += 1;
    }

    counts
        .into_iter()
        .map(|(id, count)| RandomizerItemOption {
            id,
            label: item_label(id).to_string(),
            count,
            detail: format!("Item id {id}; appears in {count} vanilla chest location(s)."),
        })
        .collect()
}

// Names the item ids currently seen in the dungeon chest masterlist.
fn item_label(id: u8) -> &'static str {
    match id {
        6 => "Mirror Shield",
        7 => "Fire Rod",
        8 => "Ice Rod",
        9 => "Magic Hammer",
        10 => "Hookshot",
        11 => "Bow",
        12 => "Boomerang",
        18 => "Lamp",
        21 => "Cane of Somaria",
        22 => "Magic Bottle",
        23 => "Piece of Heart",
        24 => "Cane of Byrna",
        25 => "Magic Cape",
        27 => "Power Glove",
        28 => "Titan's Mitt",
        31 => "Moon Pearl",
        34 => "Blue Mail",
        35 => "Red Mail",
        36 => "Small Key",
        37 => "Compass",
        40 => "Bombs",
        42 => "Magical Boomerang",
        50 => "Big Key",
        51 => "Dungeon Map",
        52 => "Rupee",
        53 => "Rupees (5)",
        54 => "Rupees (20)",
        63 => "Heart Container",
        64 => "Rupees (100)",
        65 => "Rupees (50)",
        67 => "Arrow",
        68 => "Arrows (10)",
        70 => "Rupees (300)",
        _ => "Item",
    }
}

// Restores dungeon chest YAML values from the generated vanilla masterlist.
#[tauri::command]
pub fn restore_vanilla_randomizer_yaml(project_path: String) -> Result<ActionResult, String> {
    let project = PathBuf::from(project_path);
    let python = project_python(&project)?;

    run_command(
        &display_path(&python),
        &["assets/restool-randomize.py", "--restore-vanilla"],
        &project,
        "Vanilla randomizer YAML restored.",
    )
}

// Compiles already-randomized extracted YAML into zelda3_assets.dat without re-extracting from ROM first.
#[tauri::command]
pub fn compile_randomized_assets(project_path: String) -> Result<ActionResult, String> {
    let project = PathBuf::from(project_path);
    let python = project_python(&project)?;

    run_command(
        &display_path(&python),
        &["assets/restool.py"],
        &project,
        "Randomized assets compiled.",
    )
}

// Merges two fixed-command results so the activity log shows both extraction and masterlist output.
fn combine_results(message: &str, first: ActionResult, second: ActionResult) -> ActionResult {
    ActionResult {
        ok: first.ok && second.ok,
        message: if second.ok {
            message.to_string()
        } else {
            second.message
        },
        stdout: [first.stdout, second.stdout]
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        stderr: [first.stderr, second.stderr]
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

// Finds the selected project's virtual-environment Python executable.
fn project_python(project: &Path) -> Result<PathBuf, String> {
    venv_python(&project.join(".venv"))
        .or_else(|| venv_python(&project.join("venv")))
        .ok_or_else(|| "Create a venv before using the randomizer setup screen.".to_string())
}

// Adds one optional text argument pair when the user entered a value in the form.
fn push_option(args: &mut Vec<String>, flag: &str, value: Option<String>) {
    if let Some(value) = value {
        let trimmed = value.trim();

        if !trimmed.is_empty() {
            args.push(flag.to_string());
            args.push(trimmed.to_string());
        }
    }
}

// Reports whether an expected randomizer file exists.
fn file_status(label: &str, path: &Path) -> RandomizerConfigFile {
    RandomizerConfigFile {
        label: label.to_string(),
        state: if path.is_file() { "found" } else { "missing" }.to_string(),
        detail: display_path(path),
    }
}

// Reports whether an expected randomizer folder exists, with setup guidance when absent.
fn folder_status(label: &str, path: &Path, missing_detail: &str) -> RandomizerConfigFile {
    RandomizerConfigFile {
        label: label.to_string(),
        state: if path.is_dir() { "found" } else { "missing" }.to_string(),
        detail: if path.is_dir() {
            display_path(path)
        } else {
            missing_detail.to_string()
        },
    }
}

// Reports whether a selected project script contains a required command-line feature.
fn capability_status(
    label: &str,
    path: &Path,
    needle: &str,
    missing_detail: &str,
) -> RandomizerConfigFile {
    RandomizerConfigFile {
        label: label.to_string(),
        state: if file_contains(path, needle) {
            "found"
        } else {
            "missing"
        }
        .to_string(),
        detail: if file_contains(path, needle) {
            format!("{} supports {needle}.", display_path(path))
        } else {
            missing_detail.to_string()
        },
    }
}

// Checks script text for a feature marker while treating unreadable files as unsupported.
fn file_contains(path: &Path, needle: &str) -> bool {
    fs::read_to_string(path)
        .map(|contents| contents.contains(needle))
        .unwrap_or(false)
}
