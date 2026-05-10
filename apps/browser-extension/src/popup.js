// @ts-check

/**
 * @typedef {{ healthy: boolean, pcscAvailable: boolean, activeSessionCount: number,
 *   readers: { name: string, cardPresent: boolean, error?: string | null }[],
 *   diagnostics: string[], lastError?: string | null }} DaemonStatus
 * @typedef {{ type: string, data?: unknown }} DaemonPayload
 * @typedef {{ nativeDisconnects: number, nativeTimeouts: number, watchRetries: number,
 *   daemonErrors: number, sessionStarts: number, sessionCompletions: number,
 *   sessionGuardRejects: number, waitAborts: number, lastError?: string | null,
 *   updatedAt?: string | null }} BridgeDiagnostics
 */

function getExtensionApi() {
  if (typeof chrome !== "undefined" && chrome?.runtime) {
    return chrome;
  }

  if (typeof browser !== "undefined" && browser?.runtime) {
    return browser;
  }

  throw new Error("Browser extension runtime API is not available");
}

const EXT_API = getExtensionApi();

/** @param {unknown} message @returns {Promise<any>} */
function sendMessage(message) {
  return EXT_API.runtime.sendMessage(message);
}

/** @returns {Promise<DaemonStatus | null>} */
async function fetchStatus() {
  try {
    /** @type {{ ok: boolean, response?: DaemonPayload, error?: string }} */
    const result = await sendMessage({ type: "GET_STATUS" });
    if (!result.ok || !result.response) {
      return null;
    }
    const payload = result.response;
    if (payload.type !== "STATUS" || !payload.data) {
      return null;
    }
    return /** @type {DaemonStatus} */ (payload.data);
  } catch (_) {
    return null;
  }
}

/** @returns {Promise<BridgeDiagnostics | null>} */
async function fetchBridgeDiagnostics() {
  try {
    const result = await sendMessage({ type: "GET_BRIDGE_DIAGNOSTICS" });
    if (!result?.ok || !result?.diagnostics) {
      return null;
    }

    return /** @type {BridgeDiagnostics} */ (result.diagnostics);
  } catch (_) {
    return null;
  }
}

/** @returns {Promise<BridgeDiagnostics | null>} */
async function clearBridgeDiagnostics() {
  try {
    const result = await sendMessage({ type: "CLEAR_BRIDGE_DIAGNOSTICS" });
    if (!result?.ok || !result?.diagnostics) {
      return null;
    }

    return /** @type {BridgeDiagnostics} */ (result.diagnostics);
  } catch (_) {
    return null;
  }
}

/** @param {string} message */
function updateLiveRegion(message) {
  const live = document.getElementById("status-live");
  if (live) {
    live.textContent = message;
  }
}

/** @param {string} label @param {string} badgeClass @param {string} badgeText @returns {HTMLElement} */
function makeRow(label, badgeClass, badgeText) {
  const row = document.createElement("div");
  row.className = "row";
  const l = document.createElement("span");
  l.className = "row-label";
  l.textContent = label;
  const b = document.createElement("span");
  b.className = `badge ${badgeClass}`;
  b.textContent = badgeText;
  row.append(l, b);
  return row;
}

/** @param {DaemonStatus} status */
function renderStatus(status) {
  const body = /** @type {HTMLElement} */ (document.getElementById("body"));
  body.innerHTML = "";

  const statusTitle = document.createElement("div");
  statusTitle.className = "section-title";
  statusTitle.textContent = "Status";
  body.append(statusTitle);

  const statusPanel = document.createElement("section");
  statusPanel.className = "status-panel";
  statusPanel.append(
    makeRow("Daemon", status.healthy ? "ok" : "warn", status.healthy ? "Connected" : "Degraded")
  );
  statusPanel.append(
    makeRow(
      "PC/SC",
      status.pcscAvailable ? "ok" : "err",
      status.pcscAvailable ? "Available" : "Unavailable"
    )
  );
  statusPanel.append(
    makeRow(
      "Sessions",
      status.activeSessionCount > 0 ? "warn" : "ok",
      status.activeSessionCount > 0 ? `${status.activeSessionCount} active` : "0 active"
    )
  );
  body.append(statusPanel);

  if (status.activeSessionCount > 0) {
    const sessionPanel = document.createElement("section");
    sessionPanel.className = "session-panel";

    const sessionTitle = document.createElement("div");
    sessionTitle.className = "section-title";
    sessionTitle.textContent = "Active Session";

    const sessionCount = document.createElement("div");
    sessionCount.className = "session-meta";
    sessionCount.innerHTML =
      '<span class="session-meta-label">Open sessions</span><span>' +
      String(status.activeSessionCount) +
      "</span>";

    const sessionHint = document.createElement("div");
    sessionHint.className = "hint-note";
    sessionHint.textContent =
      "Authentication is in progress. Keep this popup open to monitor connection health.";

    sessionPanel.append(sessionTitle, sessionCount, sessionHint);
    body.append(sessionPanel);
  }

  // Readers section
  const section = document.createElement("div");
  section.className = "readers-section";
  const sectionLabel = document.createElement("div");
  sectionLabel.className = "label";
  sectionLabel.textContent = "Readers";
  section.append(sectionLabel);

  if (status.readers.length === 0) {
    const note = document.createElement("div");
    note.className = "hint-note";
    note.textContent = status.pcscAvailable
      ? "No readers detected. Attach a USB smartcard reader."
      : "Install and start pcscd to enable smartcard support.";
    section.append(note);
  } else {
    for (const reader of status.readers) {
      const item = document.createElement("div");
      item.className = "reader-item";
      const name = document.createElement("span");
      name.className = "reader-name";
      name.title = reader.name;
      name.textContent = reader.name;
      const badge = document.createElement("span");
      badge.className = reader.cardPresent ? "badge ok" : "badge warn";
      badge.textContent = reader.cardPresent ? "Card present" : "No card";
      item.append(name, badge);
      section.append(item);
    }
  }

  body.append(section);

  if (status.lastError) {
    const errNote = document.createElement("div");
    errNote.className = "error-note";
    errNote.textContent = status.lastError;
    body.append(errNote);
  }

  if (!status.pcscAvailable) {
    const guide = document.createElement("section");
    guide.className = "guide";
    guide.innerHTML =
      "<strong>PC/SC unavailable</strong>" +
      "<span>Install and start pcscd, then refresh this popup.</span>";
    body.append(guide);
  }

  updateLiveRegion(
    `Daemon ${status.healthy ? "connected" : "degraded"}. ` +
      `PCSC ${status.pcscAvailable ? "available" : "unavailable"}. ` +
      `${status.activeSessionCount} active sessions.`
  );
}

function renderDisconnected() {
  const body = /** @type {HTMLElement} */ (document.getElementById("body"));
  body.innerHTML = "";
  body.append(makeRow("Daemon", "err", "Disconnected"));
  const guide = document.createElement("section");
  guide.className = "guide";
  guide.innerHTML =
    "<strong>Daemon unavailable</strong>" +
    "<span>Run ./scripts/run-daemon.sh, then click refresh.</span>";
  body.append(guide);
  updateLiveRegion("Daemon disconnected");
}

/** @param {BridgeDiagnostics} diagnostics */
function renderBridgeDiagnostics(diagnostics) {
  const body = /** @type {HTMLElement} */ (document.getElementById("body"));
  const details = document.createElement("details");
  details.className = "diag";

  const summary = document.createElement("summary");
  summary.setAttribute("role", "button");
  summary.setAttribute("aria-label", "Toggle bridge diagnostics");
  summary.innerHTML = '<span class="section-title">Bridge Diagnostics</span><span>Toggle</span>';
  details.append(summary);

  const diagBody = document.createElement("div");
  diagBody.className = "diag-body";
  diagBody.append(makeRow("Session starts", "ok", String(diagnostics.sessionStarts)));
  diagBody.append(makeRow("Completions", "ok", String(diagnostics.sessionCompletions)));
  diagBody.append(makeRow("Watch retries", "warn", String(diagnostics.watchRetries)));
  diagBody.append(makeRow("Native timeouts", "warn", String(diagnostics.nativeTimeouts)));
  diagBody.append(makeRow("Native disconnects", "warn", String(diagnostics.nativeDisconnects)));
  diagBody.append(makeRow("Validation errors", "warn", String(diagnostics.daemonErrors)));

  if (diagnostics.updatedAt) {
    const updatedAt = document.createElement("div");
    updatedAt.className = "hint-note";
    updatedAt.textContent = `Updated: ${diagnostics.updatedAt}`;
    diagBody.append(updatedAt);
  }

  if (diagnostics.lastError) {
    const errNote = document.createElement("div");
    errNote.className = "error-note";
    errNote.textContent = diagnostics.lastError;
    diagBody.append(errNote);
  }

  const controls = document.createElement("div");
  controls.className = "diag-controls";
  const clearButton = document.createElement("button");
  clearButton.type = "button";
  clearButton.textContent = "Clear metrics";
  clearButton.addEventListener("click", async () => {
    await clearBridgeDiagnostics();
    await refresh();
  });
  controls.append(clearButton);
  diagBody.append(controls);

  details.append(diagBody);
  body.append(details);
}

async function refresh() {
  const [status, diagnostics] = await Promise.all([fetchStatus(), fetchBridgeDiagnostics()]);
  if (status) {
    renderStatus(status);
  } else {
    renderDisconnected();
  }

  if (diagnostics) {
    renderBridgeDiagnostics(diagnostics);
  }
}

document.getElementById("refresh-btn")?.addEventListener("click", () => {
  void refresh();
});

void refresh();
