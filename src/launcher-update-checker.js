// Checks GitHub releases for launcher updates without adding Rust HTTP dependencies.
const LAUNCHER_RELEASE_API = "https://api.github.com/repos/xander-haj/lawn/releases/latest";
const LAUNCHER_RELEASES_PAGE = "https://github.com/xander-haj/lawn/releases";

export function connectLauncherUpdateChecker(helpers) {
  const { elements } = helpers;

  elements.updateCheckButton.addEventListener("click", async () => {
    await checkLauncherUpdates(helpers);
  });
}

async function checkLauncherUpdates(helpers) {
  const { elements, call, log, openExternalUrl } = helpers;
  const originalText = elements.updateCheckButton.textContent;
  elements.updateCheckButton.disabled = true;
  elements.updateCheckButton.textContent = "Checking";

  try {
    const currentVersion = await call("launcher_version");
    const latestRelease = await fetchLatestRelease();
    const latestTag = latestRelease.tag_name ?? "";
    const latestUrl = latestRelease.html_url ?? LAUNCHER_RELEASES_PAGE;
    const comparison = compareVersions(latestTag, currentVersion);

    if (comparison > 0) {
      log(`Launcher update available: ${currentVersion} -> ${latestTag}`);

      if (window.confirm(`Launcher update ${latestTag} is available. Open the download page?`)) {
        await openExternalUrl(latestUrl);
      }

      return;
    }

    if (comparison < 0) {
      log(`Launcher ${currentVersion} is newer than the latest published release ${latestTag}.`);
      return;
    }

    log(`Launcher is up to date (${currentVersion}).`);
  } catch (error) {
    log(`Could not check launcher updates: ${error}`);
  } finally {
    elements.updateCheckButton.disabled = false;
    elements.updateCheckButton.textContent = originalText;
  }
}

async function fetchLatestRelease() {
  const response = await fetch(LAUNCHER_RELEASE_API, {
    headers: {
      Accept: "application/vnd.github+json",
    },
  });

  if (!response.ok) {
    throw new Error(`GitHub release check failed with HTTP ${response.status}`);
  }

  return response.json();
}

function compareVersions(left, right) {
  const leftParts = versionParts(left);
  const rightParts = versionParts(right);

  for (let index = 0; index < Math.max(leftParts.length, rightParts.length); index += 1) {
    const leftValue = leftParts[index] ?? 0;
    const rightValue = rightParts[index] ?? 0;

    if (leftValue > rightValue) {
      return 1;
    }

    if (leftValue < rightValue) {
      return -1;
    }
  }

  return 0;
}

function versionParts(value) {
  const match = String(value).match(/\d+(?:\.\d+)*/);

  if (!match) {
    return [0];
  }

  return match[0].split(".").map((part) => Number.parseInt(part, 10) || 0);
}
