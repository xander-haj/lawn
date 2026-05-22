// This binary entry point delegates app wiring to the library crate because Tauri
// expects the declared `z3r_launcher_lib` library to exist.
fn main() {
    z3r_launcher_lib::run();
}
