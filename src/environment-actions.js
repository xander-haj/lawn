// This module derives Environment screen setup-action availability from read-only check rows.

// Enables each setup button only after the dependency gates before it are satisfied.
// elements contains the Environment screen action buttons, and checks are backend check rows.
// Returns nothing after updating disabled states in place.
export function updateEnvironmentActions(elements, checks) {
  const pythonReady = checkReady(checks, "python");
  const venvReady = checkReady(checks, "venv");
  const dependenciesReady = checkReady(checks, "python-dependencies");
  // Build assets invokes restool.py --extract-from-rom, which fails without zelda3.sfc in the project root.
  const romReady = checkReady(checks, "rom");

  elements.venvButton.disabled = !pythonReady;
  elements.dependenciesButton.disabled = !pythonReady || !venvReady;
  elements.extractButton.disabled = !pythonReady || !venvReady || !dependenciesReady || !romReady;
}

// Looks up one environment check by stable id and treats only the explicit ok state as ready.
// checks is the backend report list, and id is the required dependency id.
// Returns true when the matching check exists and reports ok.
function checkReady(checks, id) {
  return checks.some((check) => check.id === id && check.state === "ok");
}
