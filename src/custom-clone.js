// This module owns the custom GitHub clone dialog and keeps clone-specific UI out of the main app file.

// Wires the custom clone modal to a backend clone command and refresh callback.
export function connectCustomCloneDialog(options) {
  const { elements, getScanRoot, call, log, refreshScan } = options;

  elements.customCloneButton.addEventListener("click", () => {
    elements.customCloneUrl.value = "";
    elements.customCloneDialog.showModal();
    elements.customCloneUrl.focus();
  });

  elements.customCloneCancelButton.addEventListener("click", () => {
    elements.customCloneDialog.close();
  });

  elements.customCloneForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    await cloneCustomRepository(elements, getScanRoot, call, log, refreshScan);
  });
}

// Sends only the typed URL to Rust; the backend rejects pasted full git clone commands.
async function cloneCustomRepository(elements, getScanRoot, call, log, refreshScan) {
  const repoUrl = elements.customCloneUrl.value.trim();

  if (!repoUrl) {
    log("Paste a GitHub repository URL before cloning.");
    return;
  }

  elements.customCloneSubmitButton.disabled = true;

  try {
    const result = await call("clone_custom_project", {
      repoUrl,
      scanRoot: getScanRoot(),
    });
    log(result.message);

    if (result.stdout) {
      log(result.stdout.trim());
    }

    if (result.stderr) {
      log(result.stderr.trim());
    }

    elements.customCloneDialog.close();
    await refreshScan();
  } finally {
    elements.customCloneSubmitButton.disabled = false;
  }
}
