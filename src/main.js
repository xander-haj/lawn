// Launcher bootstrap module. Owns shared state, the Tauri invoker wrapper, view
// switching, and top-bar button wiring. Per-screen DOM building lives in dedicated
// modules so this file stays focused on app-wide concerns.
import { loadManualInstallGuides } from "./manual-guides.js";
import { connectRandomizerSetup } from "./randomizer-setup.js";
import { connectProjectCards } from "./project-cards.js";
import { connectEnvironmentScreen } from "./environment-screen.js";
import { connectControlsScreen } from "./controls-screen.js";
import {
  connectScanPathManager,
  loadStoredClonePath,
  loadStoredScanPaths,
} from "./scan-path-manager.js";
const { invoke } = window.__TAURI__.core;

// App-wide mutable state. Each screen module reads from this through the helpers bag
// so there is exactly one source of truth for the selected project, scan paths, etc.
const state = {
  candidates: [],
  scanGroups: [],
  selectedPath: null,
  scanPaths: loadStoredScanPaths(),
  clonePath: loadStoredClonePath(),
  hasStoredRom: false,
  activeView: "builds",
  environmentOs: "macos",
  setupGuidance: null,
  manualInstallGuides: null,
};

// DOM references collected once at boot so screen modules don't repeat querySelector
// lookups on every render.
const elements = {
  viewPanels: document.querySelectorAll(".view-panel"),
  parentPath: document.querySelector("#parentPath"),
  projectList: document.querySelector("#projectList"),
  checkList: document.querySelector("#checkList"),
  stepList: document.querySelector("#stepList"),
  manualGuideTitle: document.querySelector("#manualGuideTitle"),
  manualGuideMeta: document.querySelector("#manualGuideMeta"),
  manualGuideContent: document.querySelector("#manualGuideContent"),
  logOutput: document.querySelector("#logOutput"),
  activityToggle: document.querySelector("#activityToggle"),
  activityPanel: document.querySelector("#activityPanel"),
  refreshButton: document.querySelector("#refreshButton"),
  scanPathButton: document.querySelector("#scanPathButton"),
  uploadRomButton: document.querySelector("#uploadRomButton"),
  scanPathDialog: document.querySelector("#scanPathDialog"),
  scanPathForm: document.querySelector("#scanPathForm"),
  scanPathInput: document.querySelector("#scanPathInput"),
  scanPathSelectButton: document.querySelector("#scanPathSelectButton"),
  scanPathAddButton: document.querySelector("#scanPathAddButton"),
  scanPathList: document.querySelector("#scanPathList"),
  scanPathCloseButton: document.querySelector("#scanPathCloseButton"),
  scanPathsTabButton: document.querySelector("#scanPathsTabButton"),
  cloneTabButton: document.querySelector("#cloneTabButton"),
  scanPathsTabPanel: document.querySelector("#scanPathsTabPanel"),
  cloneTabPanel: document.querySelector("#cloneTabPanel"),
  clonePathSelect: document.querySelector("#clonePathSelect"),
  cloneZ3RModalButton: document.querySelector("#cloneZ3RModalButton"),
  cloneCustomUrl: document.querySelector("#cloneCustomUrl"),
  cloneCustomModalButton: document.querySelector("#cloneCustomModalButton"),
  backButton: document.querySelector("#backButton"),
  checkButton: document.querySelector("#checkButton"),
  guideBackButton: document.querySelector("#guideBackButton"),
  venvButton: document.querySelector("#venvButton"),
  dependenciesButton: document.querySelector("#dependenciesButton"),
  extractButton: document.querySelector("#extractButton"),
  clearLogButton: document.querySelector("#clearLogButton"),
};

// Timestamped activity console entry used by every screen for command output and
// non-fatal warnings. Keeps the log entries consistent and auto-scrolls to bottom.
function log(message) {
  const now = new Date().toLocaleTimeString();
  elements.logOutput.textContent += `\n[${now}] ${message}`;
  elements.logOutput.scrollTop = elements.logOutput.scrollHeight;
}

// Safe Tauri invoker that routes backend errors into the activity log AND re-throws so
// callers can guard their own UI flow when needed.
async function call(command, payload = {}) {
  try {
    return await invoke(command, payload);
  } catch (error) {
    log(`${command} failed: ${error}`);
    throw error;
  }
}

// View switching toggles the .active class on the matching panel. The Back to home
// button is hidden on the home view; the global topbar actions are home-only
// because they operate on ROM storage, scan paths, or new project folders.
function showView(view) {
  state.activeView = view;
  for (const panel of elements.viewPanels) {
    panel.classList.toggle("active", panel.dataset.view === view);
  }
  const onHome = view === "builds";
  elements.backButton.classList.toggle("hidden", onHome);
  elements.scanPathButton.classList.toggle("hidden", !onHome);
  elements.uploadRomButton.classList.toggle("hidden", !onHome);

  // Refresh the per-view content lazily so screens always reflect on-disk truth.
  if (view === "controls") {
    controlsScreen.refresh();
  }
}

// Stores the selected project path and refreshes both the card grid (selected style)
// and the environment screen (which reacts to the new project's local files).
async function selectProject(projectPath) {
  state.selectedPath = projectPath;
  projectCards.render();
  await environmentScreen.runChecks();
}

// Opens the environment view for a specific project, mirroring openControls below.
async function openEnvironment(projectPath) {
  await selectProject(projectPath);
  showView("environment");
}

// Launches a ready project. The backend takes only the executable path and runs it
// from its own folder so no arbitrary shell execution happens here.
async function launchProject(candidate) {
  const result = await call("launch_game", { executablePath: candidate.executable_path });
  log(result.message);
}

// Runs a setup action and then refreshes scan + environment so the UI catches up.
async function runAction(command, payload = {}) {
  const result = await call(command, payload);
  log(result.message);

  if (result.stdout) {
    log(result.stdout.trim());
  }

  if (result.stderr) {
    log(result.stderr.trim());
  }

  await refreshScan();
}

// Guard used by setup buttons that require a selected project — logs a hint and
// returns null so the calling handler can short-circuit cleanly.
function selectedProjectPayload() {
  if (!state.selectedPath) {
    log("Select or clone a Z3R folder first.");
    return null;
  }

  return { projectPath: state.selectedPath };
}

// Re-runs the backend sibling scan, keeps the selected project alive when it still
// exists, and repaints the card grid and environment screen.
async function refreshScan() {
  const scan = await call("scan_siblings", { scanRoots: state.scanPaths });
  state.candidates = scan.candidates;
  state.scanGroups = scan.groups ?? [];
  elements.parentPath.textContent = "";

  if (state.hasStoredRom && state.candidates.length > 0) {
    const result = await call("sync_stored_rom_to_projects", {
      projectPaths: state.candidates.map((candidate) => candidate.path),
    });

    if (result.stdout) {
      log(`SFC copied to:\n${result.stdout}`);
    }
  }

  if (!state.candidates.some((candidate) => candidate.path === state.selectedPath)) {
    state.selectedPath = state.candidates[0]?.path ?? null;
  }

  projectCards.render();
  await environmentScreen.runChecks();
}

// Refreshes the launcher-managed ROM status independently from project scanning.
async function refreshRomStatus() {
  const status = await call("stored_rom_status");
  state.hasStoredRom = status.available;
  elements.uploadRomButton.textContent = status.available ? "Open SFC Folder" : "Upload SFC";
  elements.scanPathButton.disabled = !status.available;
  elements.scanPathButton.title = status.available ? "" : "Upload an SFC before managing repos.";
}

// Loads the editable Setup Path JSON so step copy can change without Rust edits.
async function loadSetupGuidance() {
  try {
    const response = await fetch("./setup-guidance.json");
    state.setupGuidance = await response.json();
  } catch (error) {
    log(`Could not load setup guidance: ${error}`);
    state.setupGuidance = null;
  }
}

// Loads the editable manual-install guide JSON consumed by environment-screen.js when
// a missing dependency row exposes a Manual install button.
async function loadGuideContent() {
  state.manualInstallGuides = await loadManualInstallGuides();
}

// One helpers bag shared with every screen module so they all see the same state +
// shared callbacks without reaching for module-level globals of their own.
const helpers = {
  state,
  elements,
  call,
  log,
  showView,
  selectProject,
  openEnvironment,
  launchProject,
  refreshScan,
  runAction,
  selectedProjectPayload,
};

// Each connect*() returns a small object the bootstrap calls into (render/refresh).
const projectCards = connectProjectCards(helpers);
const environmentScreen = connectEnvironmentScreen(helpers);
const controlsScreen = connectControlsScreen(helpers);
connectScanPathManager(helpers);

elements.refreshButton.addEventListener("click", refreshScan);
elements.backButton.addEventListener("click", () => showView("builds"));
elements.guideBackButton.addEventListener("click", () => showView("environment"));
elements.activityToggle.addEventListener("click", () => {
  const isOpen = elements.activityPanel.classList.toggle("open");
  elements.activityToggle.setAttribute("aria-expanded", String(isOpen));
});
elements.checkButton.addEventListener("click", environmentScreen.runChecks);
elements.uploadRomButton.addEventListener("click", async () => {
  elements.uploadRomButton.disabled = true;

  try {
    if (state.hasStoredRom) {
      const result = await call("open_stored_rom_folder");
      log(result.message);
      return;
    }

    const status = await call("choose_and_store_rom");

    if (status) {
      log(`SFC stored at ${status.path}`);
      await refreshRomStatus();
      await refreshScan();
    }
  } finally {
    elements.uploadRomButton.disabled = false;
  }
});
connectRandomizerSetup({
  state,
  call,
  log,
  refreshScan,
  runAction,
  selectedProjectPayload,
});
elements.clearLogButton.addEventListener("click", () => {
  elements.logOutput.textContent = "Ready.";
});
elements.venvButton.addEventListener("click", () => {
  const payload = selectedProjectPayload();
  if (payload) {
    runAction("create_venv", payload);
  }
});
elements.dependenciesButton.addEventListener("click", () => {
  const payload = selectedProjectPayload();
  if (payload) {
    runAction("install_dependencies", payload);
  }
});
elements.extractButton.addEventListener("click", () => {
  const payload = selectedProjectPayload();
  if (payload) {
    runAction("extract_assets", payload);
  }
});

showView(state.activeView);
await loadSetupGuidance();
await loadGuideContent();
await refreshRomStatus();
refreshScan();
