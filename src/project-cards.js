// Renders the Detected Builds card grid and wires per-card buttons.
// Extracted from main.js so the card markup, click handlers, and per-card widget mounts
// have a focused home as the card surface area grows (Aspect Ratio + Controls buttons,
// nested {owner}/{repo} discoveries, etc).

// Imports: shared utilities and the per-card Aspect Ratio compound widget.
import { escapeHtml, labelStatus } from "./shared-utils.js";
import { mountAspectRatioWidget } from "./card-aspect-ratio.js";

// Wires up the project-list rendering loop and exposes a `render()` callback the host
// uses to repaint after any state change. `helpers` carries state, DOM refs, the Tauri
// invoker, the logger, and view-switch callbacks so the module stays free of globals.
export function connectProjectCards(helpers) {
  return {
    // Renders every candidate card and the empty-state card when none were discovered.
    render() {
      renderProjectList(helpers);
    },
  };
}

// Replaces the project list with one card per candidate, or an empty-state card.
function renderProjectList(helpers) {
  const { elements, state } = helpers;
  elements.projectList.textContent = "";

  if (state.candidates.length === 0) {
    elements.projectList.append(buildEmptyCard());
    return;
  }

  for (const candidate of state.candidates) {
    elements.projectList.append(buildProjectCard(candidate, helpers));
  }
}

// Builds the "no folders found" placeholder shown when scan_siblings returned 0 results.
// Kept identical to the previous empty-card markup so the visual layout doesn't shift.
function buildEmptyCard() {
  const empty = document.createElement("article");
  empty.className = "project-card";
  empty.innerHTML = `
    <span class="status warning">Setup needed</span>
    <h3>No Z3R folders found</h3>
    <p class="path-line">Use Clone Z3R or place a Z3R folder beside this launcher.</p>
  `;
  return empty;
}

// Constructs one fully-wired card for a discovered candidate, including all click
// handlers and the inline Aspect Ratio widget. Each button stops click propagation so
// pressing Environment / Randomizer / Controls / Play does NOT also trigger the
// card-level selectProject handler.
function buildProjectCard(candidate, helpers) {
  const { state, selectProject } = helpers;
  const card = document.createElement("article");
  card.className = `project-card ${candidate.path === state.selectedPath ? "selected" : ""}`;
  card.addEventListener("click", () => selectProject(candidate.path));

  // Status pill colors derive from the backend status string; unknown statuses fall back to
  // the "warning" gold palette so they remain visible rather than disappearing.
  const statusClass = { ready: "ready", "missing-assets": "missing" }[candidate.status] ?? "warning";
  const playDisabled = candidate.status !== "ready" || !candidate.executable_path;
  const authorLine = candidate.owner
    ? `<p class="card-author">by ${escapeHtml(candidate.owner)}</p>`
    : "";

  card.innerHTML = buildCardMarkup({
    statusClass,
    statusLabel: labelStatus(candidate.status),
    playDisabled,
    nameSafe: escapeHtml(candidate.name),
    authorLine,
  });

  wireCardButtons(card, candidate, helpers);
  // Mount the inline Aspect Ratio compound widget into the placeholder slot. The widget
  // owns its own debounce + auto-save loop against the project's zelda3.ini.
  mountAspectRatioWidget(card.querySelector(".card-aspect-mount"), candidate, helpers);

  return card;
}

// Centralizes the card HTML so wireCardButtons can stay focused on event wiring. The
// card now has FOUR grid rows: status/play, title-block, card-config-actions (aspect +
// controls), and card-setup-actions (environment + randomizer).
function buildCardMarkup({ statusClass, statusLabel, playDisabled, nameSafe, authorLine }) {
  return `
    <span class="status ${statusClass}">${statusLabel}</span>
    <button class="play-button" type="button" ${playDisabled ? "disabled" : ""}>Play</button>
    <div class="card-title-block">
      <h3>${nameSafe}</h3>
      ${authorLine}
    </div>
    <div class="card-config-actions">
      <div class="card-aspect-mount"></div>
      <button class="secondary-button controls-button" type="button">Controls</button>
    </div>
    <div class="card-setup-actions">
      <button class="secondary-button environment-button" type="button">Environment</button>
      <button class="secondary-button randomizer-button" type="button">Randomizer</button>
    </div>
  `;
}

// Attaches the per-button click handlers. The aspect ratio widget mounts later because
// it lives inside the placeholder element and owns its own DOM structure.
function wireCardButtons(card, candidate, helpers) {
  const { selectProject, openEnvironment, showView, launchProject } = helpers;

  card.querySelector(".environment-button").addEventListener("click", async (event) => {
    event.stopPropagation();
    await openEnvironment(candidate.path);
  });

  card.querySelector(".randomizer-button").addEventListener("click", async (event) => {
    event.stopPropagation();
    await selectProject(candidate.path);
    showView("randomizer");
  });

  card.querySelector(".controls-button").addEventListener("click", async (event) => {
    event.stopPropagation();
    await selectProject(candidate.path);
    showView("controls");
  });

  card.querySelector(".play-button").addEventListener("click", async (event) => {
    event.stopPropagation();
    await launchProject(candidate);
  });
}
