// This frontend keeps UI state and delegates filesystem/process work to scoped Rust commands.
const { invoke } = window.__TAURI__.core;

const state = {
  candidates: [],
  selectedPath: null,
  scanRoot: null,
};

const elements = {
  parentPath: document.querySelector("#parentPath"),
  projectList: document.querySelector("#projectList"),
  checkList: document.querySelector("#checkList"),
  stepList: document.querySelector("#stepList"),
  logOutput: document.querySelector("#logOutput"),
  refreshButton: document.querySelector("#refreshButton"),
  chooseRootButton: document.querySelector("#chooseRootButton"),
  defaultRootButton: document.querySelector("#defaultRootButton"),
  cloneButton: document.querySelector("#cloneButton"),
  checkButton: document.querySelector("#checkButton"),
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

// Safely invokes a backend command and routes errors into the visible activity log.
async function call(command, payload = {}) {
  try {
    return await invoke(command, payload);
  } catch (error) {
    log(String(error));
    throw error;
  }
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
    <button class="play-button" type="button" ${playDisabled ? "disabled" : ""}>Play</button>
  `;

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
  renderChecks(report.checks);
  renderSteps(report.next_steps);
}

// Renders each environment check as a compact row for quick scanning.
function renderChecks(checks) {
  elements.checkList.textContent = "";

  for (const check of checks) {
    const row = document.createElement("div");
    row.className = `check-row ${isMissingMsbuild(check) ? "has-action" : ""}`;
    row.innerHTML = `
      <span class="check ${escapeHtml(check.state)}">${escapeHtml(check.state)}</span>
      <strong>${escapeHtml(check.label)}</strong>
      <span class="path-line">${escapeHtml(check.detail || "No detail returned.")}</span>
    `;

    if (isMissingMsbuild(check)) {
      const fixButton = document.createElement("button");
      fixButton.className = "check-action-button";
      fixButton.type = "button";
      fixButton.textContent = "Manual fix";
      fixButton.addEventListener("click", () => runAction("open_msbuild_help"));
      row.append(fixButton);
    }

    elements.checkList.append(row);
  }
}

// Detects the one Windows prerequisite where the launcher can only guide the user to Microsoft's installer.
function isMissingMsbuild(check) {
  return check.label === "MSBuild" && check.state === "missing";
}

// Renders setup steps from Rust so OS-specific advice stays consistent with backend checks.
function renderSteps(steps) {
  elements.stepList.textContent = "";

  for (const step of steps) {
    const item = document.createElement("li");
    item.textContent = step;
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
elements.checkButton.addEventListener("click", runEnvironmentChecks);
elements.chooseRootButton.addEventListener("click", async () => {
  const selectedRoot = await call("choose_scan_root");

  if (selectedRoot) {
    state.scanRoot = selectedRoot;
    state.selectedPath = null;
    log(`Scan root set to ${selectedRoot}`);
    await refreshScan();
  }
});
elements.defaultRootButton.addEventListener("click", async () => {
  state.scanRoot = null;
  state.selectedPath = null;
  log("Scan root reset to the launcher default.");
  await refreshScan();
});
elements.cloneButton.addEventListener("click", () => runAction("clone_project", { scanRoot: state.scanRoot }));
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

refreshScan();
