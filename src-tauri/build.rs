// This build script lets Tauri generate platform resources and embed app configuration.
// It also forwards release metadata from CI into the compiled binary for update checks.
fn main() {
    forward_release_env("LAUNCHER_RELEASE_TAG");
    forward_release_env("LAUNCHER_BUILD_SHA");
    tauri_build::build();
}

// Copies release metadata from CI into rustc-env values so packaged apps can compare
// against GitHub release tags without relying only on the Cargo package version.
fn forward_release_env(key: &str) {
    println!("cargo:rerun-if-env-changed={key}");

    let Ok(value) = std::env::var(key) else {
        return;
    };

    if !value.trim().is_empty() {
        println!("cargo:rustc-env={key}={value}");
    }
}
