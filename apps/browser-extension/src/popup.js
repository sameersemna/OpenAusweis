// @ts-check

/**
 * @typedef {{ healthy: boolean, pcscAvailable: boolean, activeSessionCount: number,
 *   readers: { name: string, cardPresent: boolean, error?: string | null }[],
 *   diagnostics: string[], lastError?: string | null }} DaemonStatus
 * @typedef {{ type: string, data?: unknown }} DaemonPayload
 */

/** @returns {Promise<DaemonStatus | null>} */
async function fetchStatus() {
  try {
    /** @type {{ ok: boolean, response?: DaemonPayload, error?: string }} */
    const result = await chrome.runtime.sendMessage({ type: "GET_STATUS" });
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

  body.append(makeRow("Daemon", "ok", "Connected"));
  body.append(
    makeRow(
      "PC/SC",
      status.pcscAvailable ? "ok" : "err",
      status.pcscAvailable ? "Available" : "Unavailable"
    )
  );

  if (status.activeSessionCount > 0) {
    body.append(makeRow("Sessions", "warn", String(status.activeSessionCount)));
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
}

function renderDisconnected() {
  const body = /** @type {HTMLElement} */ (document.getElementById("body"));
  body.innerHTML = "";
  body.append(makeRow("Daemon", "err", "Disconnected"));
  const note = document.createElement("div");
  note.className = "hint-note";
  note.textContent =
    "Cannot reach the OpenAusweis daemon. " +
    "Ensure the native messaging host is installed and the daemon is running.";
  body.append(note);
}

async function refresh() {
  const status = await fetchStatus();
  if (status) {
    renderStatus(status);
  } else {
    renderDisconnected();
  }
}

document.getElementById("refresh-btn")?.addEventListener("click", () => {
  void refresh();
});

void refresh();
