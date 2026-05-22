// This module defines serializable data contracts shared by the Rust backend and
// JavaScript frontend.
use serde::Serialize;

#[derive(Serialize)]
pub struct AppScan {
    pub launcher_parent: String,
    pub candidates: Vec<ProjectCandidate>,
}

#[derive(Serialize)]
pub struct ProjectCandidate {
    pub name: String,
    pub path: String,
    pub asset_path: Option<String>,
    pub executable_path: Option<String>,
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
