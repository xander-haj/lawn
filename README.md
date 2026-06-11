# Z3R Launcher

Z3R Launcher is a cross-platform desktop launcher for Xander's Z3R fork. It helps users find, clone, set up, build, customize, randomize, and launch local Z3R projects from one place.

Prebuilt installers and packages are provided from the repository's [GitHub Releases tab](https://github.com/xander-haj/lawn/releases). Use those unless you specifically want to build the launcher from source.

## App Overview

The launcher is built with Tauri 2: a plain HTML/CSS/JavaScript frontend in `src/` and a Rust backend in `src-tauri/`.

Main features:

- Scans the launcher folder and user-added repo folders for Z3R builds.
- Uploads and stores a user-supplied `zelda3.sfc`, then copies it into detected or cloned projects.
- Clones the default Z3R repo or a custom GitHub fork.
- Checks project setup state for Git, Python, virtualenv, Python packages, ROM files, SDL2, Make, MSBuild, TCC, and related platform tools.
- Runs guided setup actions: create venv, install project requirements, extract assets, and build the game.
- Launches ready builds.
- Edits selected `zelda3.ini` settings, including aspect ratio, controls, gamepad settings, and feature toggles.
- Manages optional feature assets such as MSU packs, sprites, and shaders.
- Provides a randomizer setup screen for supported Z3R folders.
- Checks GitHub Releases from the Rust backend and installs launcher updates with the
  matching platform package instead of opening a browser download page.

The launcher does not include a ROM. Users must provide their own legally obtained compatible US `.sfc` file.

## Build From Source

Build each desktop package on its target operating system. Cross-compiling Tauri desktop bundles is outside the scope of this project.

### Shared Requirements

- Git
- Rust with Cargo, at least Rust `1.77.2`
- Tauri CLI 2

Install the Tauri CLI:

```sh
cargo install tauri-cli --version "^2"
```

This project does not require Node.js or npm for normal builds. The frontend is static and Tauri packages `src/` directly through `src-tauri/tauri.conf.json`.

### Build Command

Clone the repo, then build from the repository root:

```sh
git clone https://github.com/xander-haj/lawn.git
cd lawn
cargo tauri build
```

Development mode:

```sh
cargo tauri dev
```

Build output is written under `src-tauri/target/release/bundle/`.

## Windows Build

Install:

- Rust with the MSVC toolchain
- Microsoft C++ Build Tools with the `Desktop development with C++` workload
- Microsoft Edge WebView2 Runtime, if it is not already installed
- VBSCRIPT optional Windows feature, if building MSI packages

Then run from Windows Terminal or a bash-style shell:

```bash
cargo install tauri-cli --version "^2"
cargo tauri build
```

### Windows Release Toolkit

The Windows setup exe always bundles portable Git, Python, TCC, and SDL2 from
`src-tauri/bundled-tools/windows/`. The release workflow prepares that toolkit, verifies the required files,
and then runs the Tauri build so the setup exe cannot be published without the bundled tools.

To populate or refresh that toolkit locally before a Windows release, install 7-Zip and run from Windows
Terminal or a bash-style shell:

```bash
powershell.exe -ExecutionPolicy Bypass -File ./scripts/prepare-windows-toolkit.ps1
powershell.exe -ExecutionPolicy Bypass -File ./scripts/verify-windows-toolkit.ps1
cargo tauri build
```

The generated files live under `src-tauri/bundled-tools/windows/`; the committed source tree only keeps the
placeholder file.

## macOS Build

Install:

- Xcode Command Line Tools for desktop builds
- Rust with Cargo
- Tauri CLI 2

Install Apple's command line tools:

```sh
xcode-select --install
```

Then build:

```sh
cargo install tauri-cli --version "^2"
cargo tauri build
```

Local builds are unsigned unless you configure Apple signing and notarization outside this repo.

## Linux Build

Install Rust, Cargo, Tauri CLI 2, and your distribution's Tauri system dependencies.

For Debian or Ubuntu:

```sh
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

Then build:

```sh
cargo install tauri-cli --version "^2"
cargo tauri build
```

For Arch, Fedora, openSUSE, Alpine, NixOS, and other distributions, use the current Tauri 2 prerequisites page for the matching package names:

https://v2.tauri.app/start/prerequisites/

## Dependencies for Building Z3R Projects Inside the Launcher

These are needed by users who want the launcher to clone and build Z3R projects from source.

All platforms:

- A legally obtained compatible US ROM uploaded through `Upload SFC`, or placed in the project as `zelda3.sfc`
- Git
- Python 3
- A project-local Python virtual environment
- Python packages from the selected Z3R project's `requirements.txt`

macOS:

- Xcode Command Line Tools for `make`
- SDL2 development files, for example `brew install sdl2`

Linux:

- `make` and a C/C++ build toolchain
- SDL2 development files, for example `sudo apt-get install libsdl2-dev` on Debian/Ubuntu
- `python3-venv` on Debian/Ubuntu if Python cannot create a virtual environment

Prebuilt Flatpak releases use the GNOME SDK runtime so Steam Deck and other Flatpak users can run the
launcher-managed Git, Python, venv, pip, SDL2, and Make build path inside the sandbox instead of installing
those tools on the host OS. The Flatpak can work in the home folder and Steam Deck removable-media paths
under `/run/media`.

Windows:

- Visual Studio Build Tools with `Desktop development with C++` for the MSBuild route, or
- TCC and SDL2 for the lightweight TCC route
- Windows Terminal or another terminal app for running setup commands

Prebuilt Windows releases include bundled portable Git, Python, TCC, and SDL2 for the launcher-managed setup path.
MSBuild is still required if you choose the Visual Studio build route.

## Launcher Updates

The top-bar Updates button checks the latest published GitHub Release for `xander-haj/lawn`.
When a newer release exists, the launcher downloads and starts the matching package:

- Windows uses `Z3R-Launcher-windows-x64-setup.exe`, preserving the required bundled Git, Python, SDL2,
  and TCC toolkit.
- macOS uses the universal DMG and replaces the running `.app` bundle after the launcher closes.
- AppImage releases replace the running AppImage file and relaunch it.
- Flatpak releases download the `.flatpak` bundle and run the host Flatpak install-or-update command.

The updater does not execute remote scripts. It downloads release packages and runs fixed platform installer commands.
