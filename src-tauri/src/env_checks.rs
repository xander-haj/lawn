// This module performs read-only setup checks and produces OS-specific guidance.
use crate::bundled_tools::{
    bundled_detail, bundled_git, bundled_python, bundled_sdl2_dll, bundled_tcc, find_msbuild,
};
use crate::command_env::platform_command;
use crate::models::{EnvironmentCheck, EnvironmentReport};
use crate::paths::{display_path, resolve_scan_root, venv_python};
use std::env;
use std::path::{Path, PathBuf};

// Reports installed tools and project-local setup state without installing anything.
#[tauri::command]
pub fn check_environment(
    app: tauri::AppHandle,
    project_path: Option<String>,
    scan_root: Option<String>,
) -> Result<EnvironmentReport, String> {
    let parent = resolve_scan_root(scan_root)?;
    let project = project_path.map(PathBuf::from);
    let mut checks = vec![
        check_git(&app),
        check_python(&app),
        check_venv(project.as_deref()),
        check_python_dependencies(project.as_deref()),
        check_rom(project.as_deref()),
    ];

    if cfg!(target_os = "windows") {
        checks.extend(check_windows_build_tools(&app, project.as_deref()));
    } else {
        checks.extend(check_unix_build_tools());
    }

    Ok(EnvironmentReport {
        os: env::consts::OS.to_string(),
        parent_path: display_path(&parent),
        checks,
        next_steps: Vec::new(),
    })
}

// Checks the most likely Python launchers for this platform.
fn check_git(app: &tauri::AppHandle) -> EnvironmentCheck {
    if cfg!(target_os = "windows") {
        if let Some(path) = bundled_git(app) {
            return ok_check("git", "Git", &bundled_detail("Git", &path));
        }
    }

    check_command(
        "git",
        "git",
        "Git",
        &["--version"],
        "Required for cloning and updating the Z3R repo.",
    )
}

// Checks the most likely Python launchers for this platform.
fn check_python(app: &tauri::AppHandle) -> EnvironmentCheck {
    if cfg!(target_os = "windows") {
        if let Some(path) = bundled_python(app) {
            return ok_check("python", "Python", &bundled_detail("Python", &path));
        }
    }

    let commands = if cfg!(target_os = "windows") {
        vec![("py", vec!["--version"]), ("python", vec!["--version"])]
    } else {
        vec![
            ("python3", vec!["--version"]),
            ("python", vec!["--version"]),
        ]
    };

    for (program, args) in commands {
        let check = check_command(
            "python",
            program,
            "Python",
            &args,
            "Required for asset extraction and venv setup.",
        );
        if check.state == "ok" {
            return check;
        }
    }

    EnvironmentCheck {
        id: "python".to_string(),
        label: "Python".to_string(),
        state: "missing".to_string(),
        detail: "Python was not found on PATH.".to_string(),
    }
}

// Checks whether the selected project already has a usable local virtual environment folder.
fn check_venv(project_path: Option<&Path>) -> EnvironmentCheck {
    let Some(project_path) = project_path else {
        return EnvironmentCheck {
            id: "venv".to_string(),
            label: "Python virtual environment".to_string(),
            state: "unknown".to_string(),
            detail: "Select or clone a Z3R folder before checking its venv.".to_string(),
        };
    };
    let venv_path = project_path.join(".venv");
    let fallback_path = project_path.join("venv");

    if venv_python(&venv_path).is_some() {
        return ok_check(
            "venv",
            "Python virtual environment",
            &format!("Found {}", display_path(&venv_path)),
        );
    }

    if venv_python(&fallback_path).is_some() {
        return ok_check(
            "venv",
            "Python virtual environment",
            &format!("Found {}", display_path(&fallback_path)),
        );
    }

    EnvironmentCheck {
        id: "venv".to_string(),
        label: "Python virtual environment".to_string(),
        state: "missing".to_string(),
        detail: missing_venv_detail(),
    }
}

// Gives Linux users the Debian/Ubuntu package prerequisite before the Create venv action can fail.
fn missing_venv_detail() -> String {
    if cfg!(target_os = "linux") {
        return "Create one with the Create venv button. On Debian/Ubuntu, install `python3-venv` \
if Python reports ensurepip is missing."
            .to_string();
    }

    "Create one with `python -m venv .venv` inside the Z3R folder.".to_string()
}

// Checks whether the selected project's venv can import the Python packages used by asset extraction.
fn check_python_dependencies(project_path: Option<&Path>) -> EnvironmentCheck {
    let Some(project_path) = project_path else {
        return EnvironmentCheck {
            id: "python-dependencies".to_string(),
            label: "Python dependencies".to_string(),
            state: "unknown".to_string(),
            detail: "Select or clone a Z3R folder before checking Pillow and PyYAML.".to_string(),
        };
    };
    let Some(python) = venv_python(&project_path.join(".venv"))
        .or_else(|| venv_python(&project_path.join("venv")))
    else {
        return EnvironmentCheck {
            id: "python-dependencies".to_string(),
            label: "Python dependencies".to_string(),
            state: "missing".to_string(),
            detail: "Create a venv before installing or checking Python requirements.".to_string(),
        };
    };

    check_command(
        "python-dependencies",
        &display_path(&python),
        "Python dependencies",
        &["-c", "import PIL, yaml"],
        "Install dependencies with the venv before extracting assets.",
    )
}

// Checks whether the selected project root contains the user-supplied US ROM file.
// The launcher gates Build assets on this because `restool.py --extract-from-rom` requires it.
fn check_rom(project_path: Option<&Path>) -> EnvironmentCheck {
    let Some(project_path) = project_path else {
        return EnvironmentCheck {
            id: "rom".to_string(),
            label: "Game ROM (zelda3.sfc)".to_string(),
            state: "unknown".to_string(),
            detail: "Select or clone a Z3R folder before checking the ROM.".to_string(),
        };
    };
    let rom_path = project_path.join("zelda3.sfc");

    if rom_path.is_file() {
        return ok_check(
            "rom",
            "Game ROM (zelda3.sfc)",
            &format!("Found {}", display_path(&rom_path)),
        );
    }

    EnvironmentCheck {
        id: "rom".to_string(),
        label: "Game ROM (zelda3.sfc)".to_string(),
        state: "missing".to_string(),
        detail: "Upload your SFC in the launcher, or place it as zelda3.sfc in the Z3R folder."
            .to_string(),
    }
}

// Checks Visual Studio and TCC-oriented Windows build prerequisites.
fn check_windows_build_tools(
    app: &tauri::AppHandle,
    project_path: Option<&Path>,
) -> Vec<EnvironmentCheck> {
    let mut checks = vec![
        check_msbuild(),
        check_command(
            "powershell",
            "where",
            "PowerShell",
            &["powershell"],
            "PowerShell can activate .venv and run setup commands.",
        ),
    ];

    if let Some(project_path) = project_path {
        let tcc = project_path.join("third_party").join("tcc").join("tcc.exe");
        let sdl = project_path
            .join("third_party")
            .join("SDL2-2.26.3")
            .join("lib")
            .join("x64")
            .join("SDL2.dll");
        checks.push(check_project_or_bundled_file(
            "tcc",
            "TCC",
            &tcc,
            bundled_tcc(app),
            "Required only for the lightweight TCC route.",
        ));
        checks.push(check_project_or_bundled_file(
            "sdl2",
            "SDL2",
            &sdl,
            bundled_sdl2_dll(app),
            "Required by the TCC route and game runtime on Windows.",
        ));
    }

    checks
}

// Checks MSBuild through PATH, vswhere, and common Visual Studio install folders.
fn check_msbuild() -> EnvironmentCheck {
    find_msbuild().map_or_else(
        || EnvironmentCheck {
            id: "msbuild".to_string(),
            label: "MSBuild".to_string(),
            state: "missing".to_string(),
            detail: "Install Build Tools for Visual Studio with Desktop development with C++."
                .to_string(),
        },
        |path| {
            ok_check(
                "msbuild",
                "MSBuild",
                &format!("Found {}", display_path(&path)),
            )
        },
    )
}

// Checks Unix-style build tools used by the Makefile.
fn check_unix_build_tools() -> Vec<EnvironmentCheck> {
    vec![
        check_command(
            "make",
            "make",
            "Make",
            &["--version"],
            "Required to compile Z3R on macOS and Linux.",
        ),
        check_command(
            "sdl2-dev",
            "sdl2-config",
            "SDL2 development files",
            &["--version"],
            "Required by the Makefile compiler flags.",
        ),
    ]
}

// Runs a harmless version or lookup command and translates the result into a UI check row.
fn check_command(
    id: &str,
    program: &str,
    label: &str,
    args: &[&str],
    missing_detail: &str,
) -> EnvironmentCheck {
    match platform_command(program).args(args).output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            ok_check(id, label, if stdout.is_empty() { &stderr } else { &stdout })
        }
        Ok(output) => EnvironmentCheck {
            id: id.to_string(),
            label: label.to_string(),
            state: "missing".to_string(),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Err(_) => EnvironmentCheck {
            id: id.to_string(),
            label: label.to_string(),
            state: "missing".to_string(),
            detail: missing_detail.to_string(),
        },
    }
}

// Creates a successful environment check row with consistent field names.
fn ok_check(id: &str, label: &str, detail: &str) -> EnvironmentCheck {
    EnvironmentCheck {
        id: id.to_string(),
        label: label.to_string(),
        state: "ok".to_string(),
        detail: detail.to_string(),
    }
}

// Checks project-local files first, then accepts a launcher-bundled fallback.
fn check_project_or_bundled_file(
    id: &str,
    label: &str,
    project_path: &Path,
    bundled_path: Option<PathBuf>,
    missing_detail: &str,
) -> EnvironmentCheck {
    if project_path.is_file() {
        return ok_check(id, label, &format!("Found {}", display_path(project_path)));
    }

    if let Some(path) = bundled_path {
        return ok_check(id, label, &bundled_detail(label, &path));
    }

    EnvironmentCheck {
        id: id.to_string(),
        label: label.to_string(),
        state: "missing".to_string(),
        detail: missing_detail.to_string(),
    }
}
