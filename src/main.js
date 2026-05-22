// This frontend keeps UI state and delegates filesystem/process work to scoped Rust commands.
import { connectCustomCloneDialog } from "./custom-clone.js";
import {
  getManualInstallGuide,
  hasManualInstallGuide,
  loadManualInstallGuides,
  renderManualInstallGuide,
} from "./manual-guides.js";

const { invoke } = window.__TAURI__.core;

const state = {
  candidates: [],
  selectedPath: null,
  scanRoot: null,
  activeView: "builds",
  environmentOs: "macos",
  setupGuidance: null,
  manualInstallGuides: null,
};

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
  chooseRootButton: document.querySelector("#chooseRootButton"),
  defaultRootButton: document.querySelector("#defaultRootButton"),
  cloneButton: document.querySelector("#cloneButton"),
  customCloneButton: document.querySelector("#customCloneButton"),
  customCloneDialog: document.querySelector("#customCloneDialog"),
  customCloneForm: document.querySelector("#customCloneForm"),
  customCloneUrl: document.querySelector("#customCloneUrl"),
  customCloneCancelButton: document.querySelector("#customCloneCancelButton"),
  customCloneSubmitButton: document.querySelector("#customCloneSubmitButton"),
  backButton: document.querySelector("#backButton"),
  checkButton: document.querySelector("#checkButton"),
  guideBackButton: document.querySelector("#guideBackButton"),
  venvButton: document.querySelector("#venvButton"),
  dependenciesButton: document.querySelector("#dependenciesButton"),
  extractButton: document.querySelector("#extractButton"),
  buildButton: document.querySelector("#buildButton"),
  clearLogButton: document.querySelector("#clearLogButton"),
};

// Appends a timestamped entry to the activity console so users can follow long setup flows.
function log(message) {
  const timestamp = new Date().toLocaleTimeString();
  elements.logOutput.textContent += `\n[${timestamp}] ${message}`;
  elements.logOutput.scrollTop = elements.logOutput.scrollHeight;
}

// Shows either the builds home view or the selected build's environment view.
function showView(viewName) {
  state.activeView = viewName;

  for (const panel of elements.viewPanels) {
    panel.classList.toggle("active", panel.dataset.view === viewName);
  }

  elements.backButton.classList.toggle("hidden", viewName !== "environment");
}

// Safely invokes a backend command and routes errors into the visible activity log.
async function call(command, payload = {}) {
  try {
    return await invoke(command, payload);
  } catch (error) {
    log(String(error));
    throw error;
  }
}

// Loads editable setup guidance copy from JSON so wording changes do not require Rust edits.
async function loadSetupGuidance() {
  try {
    const response = await fetch("./setup-guidance.json");
    state.setupGuidance = await response.json();
  } catch (error) {
    log(`Could not load setup-guidance.json: ${error}`);
    state.setupGuidance = {
      macos: [],
      windows: [],
      linux: [],
    };
  }
}

// Loads editable manual-install guide copy for environment dependencies that require outside setup.
async function loadGuideContent() {
  state.manualInstallGuides = await loadManualInstallGuides(log);
}

// Refreshes sibling project discovery and keeps the selected project when it still exists.
async function refreshScan() {
  const scan = await call("scan_siblings", { scanRoot: state.scanRoot });
  state.candidates = scan.candidates;
  elements.parentPath.textContent = `Scan and clone root: ${scan.launcher_parent}`;

  if (!state.candidates.some((candidate) => candidate.path === state.selectedPath)) {
    state.selectedPath = state.candidates[0]?.path ?? null;
  }

  renderProjects();
  await runEnvironmentChecks();
}

// Renders the project cards with launch/build state based on backend scan results.
function renderProjects() {
  elements.projectList.textContent = "";

  if (state.candidates.length === 0) {
    const empty = document.createElement("article");
    empty.className = "project-card";
    empty.innerHTML = `
      <span class="status warning">Setup needed</span>
      <h3>No Z3R folders found</h3>
      <p class="path-line">Use Clone Z3R or place a Z3R folder beside this launcher.</p>
    `;
    elements.projectList.append(empty);
    return;
  }

  for (const candidate of state.candidates) {
    elements.projectList.append(projectCard(candidate));
  }
}

// Creates one selectable card for a discovered folder and wires its Play button.
function projectCard(candidate) {
  const card = document.createElement("article");
  card.className = `project-card ${candidate.path === state.selectedPath ? "selected" : ""}`;
  card.addEventListener("click", () => selectProject(candidate.path));

  const statusClass = candidate.status === "ready" ? "ready" : candidate.status === "missing-assets" ? "missing" : "warning";
  const playDisabled = candidate.status !== "ready" || !candidate.executable_path;
  card.innerHTML = `
    <span class="status ${statusClass}">${labelStatus(candidate.status)}</span>
    <h3>${escapeHtml(candidate.name)}</h3>
    <p class="path-line">${escapeHtml(candidate.path)}</p>
    <p class="path-line">Assets: ${escapeHtml(candidate.asset_path ?? "not found")}</p>
    <p class="path-line">Executable: ${escapeHtml(candidate.executable_path ?? "not found")}</p>
    <div class="card-actions">
      <button class="secondary-button environment-button" type="button">Environment</button>
      <button class="play-button" type="button" ${playDisabled ? "disabled" : ""}>Play</button>
    </div>
  `;

  card.querySelector(".environment-button").addEventListener("click", async (event) => {
    event.stopPropagation();
    await openEnvironment(candidate.path);
  });

  card.querySelector(".play-button").addEventListener("click", async (event) => {
    event.stopPropagation();
    await launchProject(candidate);
  });

  return card;
}

// Stores the selected project path and refreshes checks that depend on project-local files.
async function selectProject(projectPath) {
  state.selectedPath = projectPath;
  renderProjects();
  await runEnvironmentChecks();
}

// Opens setup checks for exactly the project chosen from the builds page.
async function openEnvironment(projectPath) {
  await selectProject(projectPath);
  showView("environment");
}

// Maps backend status ids into short user-facing labels.
function labelStatus(status) {
  const labels = {
    ready: "Ready",
    "needs-deploy-copy": "Needs deploy copy",
    "assets-ready": "Assets ready",
    "missing-assets": "Missing assets",
    "source-only": "Source only",
  };

  return labels[status] ?? status;
}

// Escapes text inserted through template strings so filesystem names cannot become markup.
function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

// Runs environment checks and renders setup guidance for the selected project.
async function runEnvironmentChecks() {
  const report = await call("check_environment", {
    projectPath: state.selectedPath,
    scanRoot: state.scanRoot,
  });
  state.environmentOs = report.os;
  renderChecks(report.checks);
  renderSteps();
}

// Renders each environment check as a compact row for quick scanning.
function renderChecks(checks) {
  elements.checkList.textContent = "";

  for (const check of checks) {
    const row = document.createElement("div");
    row.className = `check-row state-${check.state} ${hasManualAction(check) ? "has-action" : ""}`;
    row.innerHTML = `
      <span class="check ${escapeHtml(check.state)}">${escapeHtml(check.state)}</span>
      <strong>${escapeHtml(check.label)}</strong>
      <span class="path-line">${escapeHtml(check.detail || "No detail returned.")}</span>
    `;

    if (hasManualAction(check)) {
      const fixButton = document.createElement("button");
      fixButton.className = "check-action-button";
      fixButton.type = "button";
      fixButton.textContent = "Manual install";
      fixButton.addEventListener("click", () => openManualInstallGuide(check));
      row.append(fixButton);
    }

    elements.checkList.append(row);
  }
}

// Detects missing dependencies with editable manual guides, excluding rows covered by existing action buttons.
function hasManualAction(check) {
  const automaticRows = ["venv", "python-dependencies"];
  return (
    check.state === "missing" &&
    !automaticRows.includes(check.id) &&
    hasManualInstallGuide(state.manualInstallGuides, state.environmentOs, check.id)
  );
}

// Opens the in-app manual guide page for the dependency represented by a missing check row.
function openManualInstallGuide(check) {
  const guide = getManualInstallGuide(state.manualInstallGuides, state.environmentOs, check.id);

  if (!guide) {
    log(`No manual install guide found for ${check.label} on ${state.environmentOs}.`);
    return;
  }

  renderManualInstallGuide(guide, elements, state.selectedPath);
  showView("manual-guide");
}

// Renders setup steps from editable JSON and substitutes the selected project path when available.
function renderSteps() {
  const steps = state.setupGuidance?.[state.environmentOs] ?? [];
  elements.stepList.textContent = "";

  for (const step of steps) {
    if (!state.selectedPath && step.includes("{projectPath}")) {
      continue;
    }

    const item = document.createElement("li");
    item.textContent = step.replace("{projectPath}", state.selectedPath ?? "");
    elements.stepList.append(item);
  }
}

// Launches a ready project through the backend with no arbitrary shell execution.
async function launchProject(candidate) {
  const result = await call("launch_game", { executablePath: candidate.executable_path });
  log(result.message);
}

// Runs a setup action and refreshes project/environment state after it completes.
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

// Requires a selected project for actions that operate inside the Z3R folder.
function selectedProjectPayload() {
  if (!state.selectedPath) {
    log("Select or clone a Z3R folder first.");
    return null;
  }

  return { projectPath: state.selectedPath };
}

elements.refreshButton.addEventListener("click", refreshScan);
elements.backButton.addEventListener("click", () => showView("builds"));
elements.guideBackButton.addEventListener("click", () => showView("environment"));
elements.activityToggle.addEventListener("click", () => {
  const isOpen = elements.activityPanel.classList.toggle("open");
  elements.activityToggle.setAttribute("aria-expanded", String(isOpen));
});
elements.checkButton.addEventListener("click", runEnvironmentChecks);
elements.chooseRootButton.addEventListener("click", async () => {
  elements.chooseRootButton.disabled = true;

  try {
    const selectedRoot = await call("choose_scan_root");

    if (selectedRoot) {
      state.scanRoot = selectedRoot;
      state.selectedPath = null;
      log(`Scan root set to ${selectedRoot}`);
      await refreshScan();
    }
  } finally {
    elements.chooseRootButton.disabled = false;
  }
});
elements.defaultRootButton.addEventListener("click", async () => {
  state.scanRoot = null;
  state.selectedPath = null;
  log("Scan root reset to the launcher default.");
  await refreshScan();
});
elements.cloneButton.addEventListener("click", () => runAction("clone_project", { scanRoot: state.scanRoot }));
connectCustomCloneDialog({
  elements,
  getScanRoot: () => state.scanRoot,
  call,
  log,
  refreshScan,
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
elements.buildButton.addEventListener("click", () => {
  const payload = selectedProjectPayload();

  if (payload) {
    runAction("build_project", payload);
  }
});

showView(state.activeView);
await loadSetupGuidance();
await loadGuideContent();
refreshScan();
