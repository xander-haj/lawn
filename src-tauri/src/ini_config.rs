// This module reads and rewrites a project-local zelda3.ini one line at a time so the
// launcher can edit aspect ratio, window size, keymap, and gamepad settings without
// touching unrelated lines, comments, or whitespace. The frontend (card-aspect-ratio.js
// and controls-screen.js) drives all schema decisions; this file only knows how to
// parse the file into addressable line snapshots and replace a single line in place.
//
// Two Tauri commands are exposed:
//   read_zelda_ini(project_path) -> ZeldaIniSnapshot
//     Returns the editable [Graphics], [Sound], [Features], [KeyMap], and
//     [GamepadMap] lines for the frontend screens, plus a derived AspectRatioState
//     view of ExtendedAspectRatio + WindowSize for the per-card aspect ratio widget.
//     Each line snapshot carries a 1-based line_number so subsequent writes are
//     addressed unambiguously.
//
//   update_zelda_ini_line(project_path, line_number, raw_line) -> ActionResult
//     Replaces one line (1-based) in zelda3.ini with raw_line. The frontend composes
//     the full replacement line (including any leading `#` or `;` comment prefix) so
//     the backend never has to know which comment glyph the file uses for that line.
use crate::models::{ActionResult, AspectRatioState, IniLineSnapshot, ZeldaIniSnapshot};
use std::fs;
use std::path::PathBuf;

// Tauri command: read project-local zelda3.ini and return a frontend-ready snapshot.
// Errors propagate up to the activity drawer when the file is missing or unreadable.
#[tauri::command]
pub fn read_zelda_ini(project_path: String) -> Result<ZeldaIniSnapshot, String> {
    let path = PathBuf::from(&project_path).join("zelda3.ini");
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;

    Ok(build_snapshot(&project_path, &contents))
}

// Tauri command: replace one 1-based line in zelda3.ini with raw_line, preserving the
// rest of the file. Returns ActionResult so the existing Activity drawer plumbing can
// surface the outcome the same way every other launcher command does.
#[tauri::command]
pub fn update_zelda_ini_line(
    project_path: String,
    line_number: usize,
    raw_line: String,
) -> Result<ActionResult, String> {
    let path = PathBuf::from(&project_path).join("zelda3.ini");
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;

    // Preserve the file's original newline style by splitting before mutation and
    // re-joining with the same separator that was discovered.
    let (lines, newline) = split_preserving_newline(&contents);

    if line_number == 0 || line_number > lines.len() {
        return Err(format!(
            "zelda3.ini line {line_number} is out of range (file has {} lines).",
            lines.len()
        ));
    }

    let mut mutated = lines;
    mutated[line_number - 1] = raw_line.clone();
    let rewritten = mutated.join(newline);

    fs::write(&path, &rewritten)
        .map_err(|error| format!("Could not write {}: {error}", path.display()))?;

    Ok(ActionResult {
        ok: true,
        message: format!("zelda3.ini line {line_number} updated."),
        stdout: raw_line,
        stderr: String::new(),
    })
}

// Walks the raw ini text once and produces the structured snapshot consumed by the
// frontend. Section tracking is plain-state, no regex — the file format is simple
// enough that line.starts_with checks cover every case in the canonical zelda3.ini.
fn build_snapshot(project_path: &str, contents: &str) -> ZeldaIniSnapshot {
    let mut current_section = String::new();
    let mut graphics_lines = Vec::new();
    let mut sound_lines = Vec::new();
    let mut feature_lines = Vec::new();
    let mut keymap_lines = Vec::new();
    let mut gamepad_lines = Vec::new();
    let mut aspect_value: Option<String> = None;
    let mut aspect_line: Option<usize> = None;
    let mut window_size_value: Option<String> = None;
    let mut window_size_line: Option<usize> = None;

    for (zero_based, raw_line) in contents.lines().enumerate() {
        let line_number = zero_based + 1;
        let trimmed = raw_line.trim_start();

        // Section header lines update our running section name and never become editable
        // rows in the controls screen, so they are skipped after the section update.
        if let Some(section) = parse_section_header(trimmed) {
            current_section = section;
            continue;
        }

        // Pure comment lines (no key=value content after the # / ;) and blank lines are
        // not surfaced to the frontend — the controls screen only renders rows that
        // correspond to a real settable key.
        let Some(parsed) = parse_key_line(trimmed) else {
            continue;
        };

        // [General] ExtendedAspectRatio and [Graphics] WindowSize feed the per-card
        // aspect ratio widget directly; they are NOT surfaced as Controls-screen rows.
        if current_section == "General" && parsed.key.eq_ignore_ascii_case("ExtendedAspectRatio") {
            aspect_value = Some(parsed.value.clone());
            aspect_line = Some(line_number);
            continue;
        }
        if current_section == "Graphics" && parsed.key.eq_ignore_ascii_case("WindowSize") {
            window_size_value = Some(parsed.value.clone());
            window_size_line = Some(line_number);
            continue;
        }

        let snapshot = IniLineSnapshot {
            line_number,
            section: current_section.clone(),
            key: parsed.key,
            value: parsed.value,
            commented: parsed.commented,
            raw: raw_line.to_string(),
        };

        // Bucket the editable line into the matching tab's vector so the frontend
        // doesn't have to do any section filtering of its own.
        match current_section.as_str() {
            "Graphics" => graphics_lines.push(snapshot),
            "Sound" => sound_lines.push(snapshot),
            "Features" => feature_lines.push(snapshot),
            "KeyMap" => keymap_lines.push(snapshot),
            "GamepadMap" => gamepad_lines.push(snapshot),
            _ => {}
        }
    }

    ZeldaIniSnapshot {
        project_path: project_path.to_string(),
        aspect_ratio: AspectRatioState {
            line_number: aspect_line.unwrap_or(0),
            raw_value: aspect_value.unwrap_or_default(),
            window_size_line: window_size_line.unwrap_or(0),
            window_size_value: window_size_value.unwrap_or_else(|| "Auto".to_string()),
        },
        graphics_lines,
        sound_lines,
        feature_lines,
        keymap_lines,
        gamepad_lines,
    }
}

// Returns Some("Section") when the trimmed line is a [Section] header, None otherwise.
// Tolerates trailing whitespace and stray characters after the closing bracket so the
// scan does not bail on a slightly malformed user-edited header.
fn parse_section_header(trimmed: &str) -> Option<String> {
    let bytes = trimmed.as_bytes();
    if bytes.first() != Some(&b'[') {
        return None;
    }
    let end = trimmed.find(']')?;
    let name = trimmed[1..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

// Holds the parsed form of a key=value or #key=value (or ;key=value) line. The
// frontend never sees ParsedKey directly; it is unpacked into IniLineSnapshot in
// build_snapshot above.
struct ParsedKey {
    key: String,
    value: String,
    commented: bool,
}

// Parses any line that contributes a settable key. Returns None for pure comments,
// blank lines, and section headers. Comment glyphs accepted: `#` and `;` — the project
// ini uses both interchangeably so we accept both.
fn parse_key_line(trimmed: &str) -> Option<ParsedKey> {
    if trimmed.is_empty() {
        return None;
    }

    // Strip a single leading comment glyph and remember the commented state. The
    // payload after the glyph still has to contain "key = value" for this to be a
    // settable key — bare comments like "# this is a note" return None.
    let (commented, body) = if let Some(stripped) = trimmed.strip_prefix('#') {
        (true, stripped.trim_start())
    } else if let Some(stripped) = trimmed.strip_prefix(';') {
        (true, stripped.trim_start())
    } else {
        (false, trimmed)
    };

    let equals = body.find('=')?;
    let key = body[..equals].trim();
    if key.is_empty() || !is_key_shape(key) {
        return None;
    }
    let value = body[equals + 1..].trim().to_string();

    Some(ParsedKey {
        key: key.to_string(),
        value,
        commented,
    })
}

// Cheap heuristic so prose like "# Order: Up, Down, Left = right of the joypad" is not
// mistaken for a settable key. INI keys in zelda3.ini are ascii identifiers (letters,
// digits, underscores) — no spaces, no punctuation. Bare comment lines with `=` in the
// English text are filtered out by this check.
fn is_key_shape(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

// Splits text into lines while remembering which newline style the file used, so the
// rewrite uses the same separator (avoids accidentally converting CRLF to LF on Windows).
fn split_preserving_newline(contents: &str) -> (Vec<String>, &'static str) {
    let newline = if contents.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let lines = contents
        .split(newline)
        .map(|line| line.to_string())
        .collect();
    (lines, newline)
}
