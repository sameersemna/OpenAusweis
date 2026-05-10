import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

const THEME_STORAGE_KEY = "openausweis-theme";

type ThemePreference = "system" | "light" | "dark";
type ResolvedTheme = "light" | "dark";

function getSystemTheme(): ResolvedTheme {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return "light";
  }

  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function parseStoredTheme(value: string | null): ThemePreference {
  if (value === "light" || value === "dark" || value === "system") {
    return value;
  }

  return "system";
}

type DaemonStatus = {
  healthy: boolean;
  pcscAvailable: boolean;
  activeSessionCount: number;
  readers: { name: string; cardPresent: boolean; error?: string | null }[];
  diagnostics: string[];
  lastError?: string | null;
  ipcDiagnostics?: {
    requestCount: number;
    errorCount: number;
    validationRejections: number;
    connectionFailures: number;
  };
};

type OriginPolicy = {
  allowedExactOrigins: string[];
  allowedSuffixes: string[];
};

type SessionUpdate = {
  connected: boolean;
  sessionId?: string | null;
  state?: string | null;
  error?: string | null;
};

type RuntimeContext = {
  desktopEnv?: string | null;
  sessionType?: string | null;
  trayStrategy: string;
  notes: string[];
};

export function App() {
  const [themePreference, setThemePreference] = useState<ThemePreference>("system");
  const [resolvedTheme, setResolvedTheme] = useState<ResolvedTheme>(() => getSystemTheme());
  const [status, setStatus] = useState("Disconnected");
  const [details, setDetails] = useState("No probe executed yet");
  const [probeInFlight, setProbeInFlight] = useState(false);
  const [pcscAvailable, setPcscAvailable] = useState(false);
  const [readerStatus, setReaderStatus] = useState<DaemonStatus["readers"]>([]);
  const [diagnostics, setDiagnostics] = useState<string[]>([]);
  const [lastDiagnosticsRunAt, setLastDiagnosticsRunAt] = useState<string | null>(null);
  const [ipcDiagnostics, setIpcDiagnostics] = useState<DaemonStatus["ipcDiagnostics"] | null>(null);
  const [hotplugOpen, setHotplugOpen] = useState(false);
  const [exactOriginsInput, setExactOriginsInput] = useState("");
  const [suffixesInput, setSuffixesInput] = useState("");
  const [policyState, setPolicyState] = useState("Policy not loaded");
  const [sessionUpdate, setSessionUpdate] = useState<SessionUpdate>({
    connected: false,
    state: "IDLE",
  });
  const [pinInput, setPinInput] = useState("");
  const [pinError, setPinError] = useState<string | null>(null);
  const [submitPinBusy, setSubmitPinBusy] = useState(false);
  const [sessionResultMessage, setSessionResultMessage] = useState<string | null>(null);
  const [sessionCompletedAt, setSessionCompletedAt] = useState<string | null>(null);
  const [runtimeContext, setRuntimeContext] = useState<RuntimeContext | null>(null);

  useEffect(() => {
    const storedPreference = parseStoredTheme(window.localStorage.getItem(THEME_STORAGE_KEY));
    setThemePreference(storedPreference);
  }, []);

  useEffect(() => {
    if (themePreference !== "system") {
      setResolvedTheme(themePreference);
      return;
    }

    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const applySystemTheme = (matchesDark: boolean) => {
      setResolvedTheme(matchesDark ? "dark" : "light");
    };

    applySystemTheme(mediaQuery.matches);
    const handleChange = (event: MediaQueryListEvent) => applySystemTheme(event.matches);

    mediaQuery.addEventListener("change", handleChange);
    return () => {
      mediaQuery.removeEventListener("change", handleChange);
    };
  }, [themePreference]);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", resolvedTheme);
  }, [resolvedTheme]);

  useEffect(() => {
    window.localStorage.setItem(THEME_STORAGE_KEY, themePreference);
  }, [themePreference]);

  useEffect(() => {
    void loadPolicy();
    void loadRuntimeContext();
  }, []);

  useEffect(() => {
    if (sessionUpdate.state === "COMPLETED") {
      setSessionResultMessage("Authentication placeholder completed successfully.");
      setSessionCompletedAt(new Date().toLocaleTimeString());
    }

    if (sessionUpdate.state === "ERROR") {
      setSessionResultMessage(
        sessionUpdate.error ? `Authentication failed: ${sessionUpdate.error}` : "Authentication failed."
      );
      setSessionCompletedAt(new Date().toLocaleTimeString());
    }
  }, [sessionUpdate.state, sessionUpdate.sessionId, sessionUpdate.error]);

  useEffect(() => {
    void handleProbeDaemon();
    let unlisten: (() => void) | undefined;
    let unlistenSession: (() => void) | undefined;

    void listen<DaemonStatus>("daemon-status", (event) => {
      applyDaemonStatus(event.payload);
    }).then((dispose) => {
      unlisten = dispose;
    });

    void listen<SessionUpdate>("daemon-session", (event) => {
      setSessionUpdate(event.payload);

      // Reset stale PIN input/errors as session state evolves.
      if (event.payload.state !== "PIN_ENTRY") {
        setPinInput("");
        setPinError(null);
      }
    }).then((dispose) => {
      unlistenSession = dispose;
    });

    return () => {
      if (unlisten) {
        unlisten();
      }
      if (unlistenSession) {
        unlistenSession();
      }
    };
  }, []);

  function applyDaemonStatus(response: DaemonStatus) {
    setStatus(response.healthy ? "Connected" : "Unhealthy");
    setDetails(
      `PC/SC: ${response.pcscAvailable ? "available" : "unavailable"} | Readers: ${response.readers.length} | Sessions: ${response.activeSessionCount}`
    );
    setPcscAvailable(response.pcscAvailable);
    setReaderStatus(response.readers);
    setIpcDiagnostics(response.ipcDiagnostics || null);
    const baseDiagnostics = [...response.diagnostics];
    if (response.lastError) {
      baseDiagnostics.push(`Last error: ${response.lastError}`);
    }
    if (!response.pcscAvailable) {
      baseDiagnostics.push(
        "PC/SC runtime is unavailable. Ensure pcscd is installed and running."
      );
    } else if (response.readers.length === 0) {
      baseDiagnostics.push(
        "PC/SC is available but no readers are detected. Attach a USB reader and verify with pcsc_scan."
      );
    }
    setDiagnostics(baseDiagnostics);
  }

  async function handleProbeDaemon() {
    setProbeInFlight(true);
    try {
      const response = await invoke<DaemonStatus>("probe_daemon_status");
      applyDaemonStatus(response);
    } catch (error) {
      setStatus("Disconnected");
      setDetails(`Probe failed: ${String(error)}`);
      setReaderStatus([]);
      setDiagnostics([`Probe failed: ${String(error)}`]);
    } finally {
      setProbeInFlight(false);
    }
  }

  async function runDiagnostics() {
    await handleProbeDaemon();
    setLastDiagnosticsRunAt(new Date().toLocaleTimeString());
  }

  async function loadPolicy() {
    try {
      const policy = await invoke<OriginPolicy>("get_origin_policy");
      setExactOriginsInput(policy.allowedExactOrigins.join("\n"));
      setSuffixesInput(policy.allowedSuffixes.join("\n"));
      setPolicyState("Policy loaded");
    } catch (error) {
      setPolicyState(`Load failed: ${String(error)}`);
    }
  }

  async function savePolicy() {
    const allowedExactOrigins = exactOriginsInput
      .split("\n")
      .map((value) => value.trim())
      .filter((value) => value.length > 0);

    const allowedSuffixes = suffixesInput
      .split("\n")
      .map((value) => value.trim())
      .filter((value) => value.length > 0);

    try {
      await invoke("save_origin_policy", {
        policy: {
          allowedExactOrigins,
          allowedSuffixes,
        },
      });
      setPolicyState("Policy saved");
    } catch (error) {
      setPolicyState(`Save failed: ${String(error)}`);
    }
  }

  async function loadRuntimeContext() {
    try {
      const context = await invoke<RuntimeContext>("get_runtime_context");
      setRuntimeContext(context);
    } catch {
      setRuntimeContext(null);
    }
  }

  async function handleCancelActiveSession() {
    if (!sessionUpdate.sessionId) {
      return;
    }

    try {
      await invoke("cancel_session", { sessionId: sessionUpdate.sessionId });
      setSessionResultMessage("Session cancelled.");
    } catch (error) {
      setSessionUpdate((previous) => ({
        ...previous,
        error: `Cancel failed: ${String(error)}`,
      }));
    }
  }

  async function handleStartTestSession() {
    try {
      const update = await invoke<SessionUpdate>("start_test_session");
      setSessionUpdate(update);
      setPinError(null);
      setSessionResultMessage(null);
      setSessionCompletedAt(null);
    } catch (error) {
      setSessionUpdate((previous) => ({
        ...previous,
        error: `Start session failed: ${String(error)}`,
      }));
    }
  }

  function lifecycleStateLabel(): { label: string; tone: "ok" | "warn" | "bad" } {
    if (probeInFlight) {
      return { label: "Probing daemon startup state", tone: "warn" };
    }

    if (status === "Connected" && sessionUpdate.connected) {
      return { label: "Ready for authentication", tone: "ok" };
    }

    if (status === "Connected" && !sessionUpdate.connected) {
      return { label: "Daemon ready, session stream recovering", tone: "warn" };
    }

    if (status !== "Connected" && sessionUpdate.connected) {
      return { label: "Session stream live, daemon status recovering", tone: "warn" };
    }

    return { label: "Waiting for daemon startup", tone: "bad" };
  }

  function sessionStateHint(): string {
    switch (sessionUpdate.state) {
      case "PIN_ENTRY":
        return "PIN entry requested. Enter the card PIN to continue.";
      case "CARD_INTERACTION":
        return "Card interaction in progress. Keep card and reader connected.";
      case "COMPLETED":
        return "Authentication completed. The browser handoff can now continue.";
      case "ERROR":
        return "Authentication failed. Review diagnostics and start a new session.";
      case "IDLE":
      default:
        return "No active authentication session.";
    }
  }

  async function handleSubmitPin() {
    if (!sessionUpdate.sessionId) {
      return;
    }

    setSubmitPinBusy(true);
    setPinError(null);
    try {
      const update = await invoke<SessionUpdate>("submit_session_pin", {
        sessionId: sessionUpdate.sessionId,
        pin: pinInput,
      });
      setSessionUpdate(update);
      setPinInput("");
    } catch (error) {
      setPinError(String(error));
    } finally {
      setSubmitPinBusy(false);
    }
  }

  const showPinModal = sessionUpdate.sessionId && sessionUpdate.state === "PIN_ENTRY";
  const lifecycleState = lifecycleStateLabel();

  return (
    <main className="app-shell">
      <section className="card">
        <div className="top-row">
          <div>
            <h1>OpenAusweis</h1>
            <p className="subtitle">Linux-native German eID desktop companion</p>
          </div>
          <div className="theme-controls" role="group" aria-label="Theme selection">
            <button
              type="button"
              className={themePreference === "system" ? "secondary theme-button active" : "secondary theme-button"}
              onClick={() => setThemePreference("system")}
            >
              System
            </button>
            <button
              type="button"
              className={themePreference === "light" ? "secondary theme-button active" : "secondary theme-button"}
              onClick={() => setThemePreference("light")}
            >
              Light
            </button>
            <button
              type="button"
              className={themePreference === "dark" ? "secondary theme-button active" : "secondary theme-button"}
              onClick={() => setThemePreference("dark")}
            >
              Dark
            </button>
          </div>
        </div>

        <div className="status-row">
          <span className="label">Daemon</span>
          <span className="value">{status}</span>
        </div>
        <p className="subtitle">{details}</p>
        <div className={`lifecycle-banner lifecycle-${lifecycleState.tone}`} role="status" aria-live="polite">
          <strong>Lifecycle:</strong> {lifecycleState.label}
        </div>

        <div className="actions">
          <button onClick={handleProbeDaemon} disabled={probeInFlight}>
            {probeInFlight ? "Probing..." : "Probe daemon"}
          </button>
          <button className="secondary" onClick={loadPolicy}>Reload policy</button>
        </div>

        <section className="session-panel">
          <h2>Authentication Session</h2>
          <div className="status-row session-row">
            <span className="label">Stream</span>
            <span className={sessionUpdate.connected ? "value good" : "value bad"}>
              {sessionUpdate.connected ? "Connected" : "Disconnected"}
            </span>
          </div>
          <p className="subtitle">
            State: {sessionUpdate.state ?? "IDLE"}
            {sessionUpdate.sessionId ? ` | Session: ${sessionUpdate.sessionId}` : ""}
          </p>
          <p className="subtitle session-hint">{sessionStateHint()}</p>
          {sessionUpdate.sessionId ? (
            <div className="actions">
              <button className="secondary" onClick={handleCancelActiveSession}>
                Cancel session
              </button>
            </div>
          ) : (
            <div className="actions">
              <button className="secondary" onClick={handleStartTestSession}>
                Start test session
              </button>
            </div>
          )}
          {sessionUpdate.error ? <p className="reader-error">{sessionUpdate.error}</p> : null}
          {sessionResultMessage ? (
            <p className="session-result">
              {sessionResultMessage}
              {sessionCompletedAt ? ` (${sessionCompletedAt})` : ""}
            </p>
          ) : null}
        </section>

        <section className="device-panel">
          <h2>Reader and Card Status</h2>
          <div className="actions diagnostics-actions">
            <button className="secondary" onClick={runDiagnostics}>Run diagnostics</button>
          </div>
          {lastDiagnosticsRunAt ? (
            <p className="subtitle subtle-note">Last diagnostics run: {lastDiagnosticsRunAt}</p>
          ) : null}
          {readerStatus.length === 0 ? (
            <p className="subtitle">No PC/SC readers detected.</p>
          ) : (
            <ul className="device-list">
              {readerStatus.map((reader) => (
                <li key={reader.name}>
                  <span className="reader-name">{reader.name}</span>
                  <span className={reader.cardPresent ? "badge present" : "badge absent"}>
                    {reader.cardPresent ? "Card present" : "No card"}
                  </span>
                  {reader.error ? <p className="reader-error">{reader.error}</p> : null}
                </li>
              ))}
            </ul>
          )}
          {status === "Connected" && pcscAvailable && readerStatus.length === 0 ? (
            <div className="hotplug-guide">
              <button
                className="hotplug-toggle"
                onClick={() => setHotplugOpen((prev) => !prev)}
                aria-expanded={hotplugOpen}
              >
                {hotplugOpen ? "▾" : "▸"} First-run reader troubleshooting
              </button>
              {hotplugOpen ? (
                <ol className="hotplug-checklist">
                  <li className="hotplug-step">
                    <strong>Check the USB cable.</strong> Unplug and re-plug the reader; try a different port or cable.
                  </li>
                  <li className="hotplug-step">
                    <strong>Verify pcscd is running.</strong>{" "}
                    <code>systemctl status pcscd</code> — start it with{" "}
                    <code>sudo systemctl start pcscd</code> if stopped.
                  </li>
                  <li className="hotplug-step">
                    <strong>Check USB device permissions.</strong>{" "}
                    <code>ls -l /dev/bus/usb/**/*</code> — pcscd must be able to open the device. Check udev rules if access is denied.
                  </li>
                  <li className="hotplug-step">
                    <strong>Scan for readers from the terminal.</strong>{" "}
                    Run <code>pcsc_scan</code> (install via <code>sudo apt install pcscd pcsc-tools</code>). It should list your reader within a few seconds of plugging in.
                  </li>
                  <li className="hotplug-step">
                    <strong>Try "Run diagnostics"</strong> above after attaching the reader to refresh the status here.
                  </li>
                </ol>
              ) : null}
            </div>
          ) : null}
          {diagnostics.length > 0 ? (
            <>
              <h3>Diagnostics</h3>
              <ul className="diagnostics-list">
                {diagnostics.map((line, index) => (
                  <li key={`${line}-${index}`}>{line}</li>
                ))}
              </ul>
            </>
          ) : null}
          {ipcDiagnostics ? (
            <div className="ipc-diagnostics">
              <h3>IPC Metrics</h3>
              <div className="metrics-grid">
                <div className="metric">
                  <span className="metric-label">Requests</span>
                  <span className="metric-value">{ipcDiagnostics.requestCount}</span>
                </div>
                <div className="metric">
                  <span className="metric-label">Errors</span>
                  <span className="metric-value error">{ipcDiagnostics.errorCount}</span>
                </div>
                <div className="metric">
                  <span className="metric-label">Validation Rejections</span>
                  <span className="metric-value error">{ipcDiagnostics.validationRejections}</span>
                </div>
                <div className="metric">
                  <span className="metric-label">Connection Failures</span>
                  <span className="metric-value error">{ipcDiagnostics.connectionFailures}</span>
                </div>
              </div>
            </div>
          ) : null}
        </section>

        <section className="policy-panel">
          <h2>Origin Policy</h2>
          <p className="subtitle">Edit trusted relying-party origins and domain suffixes.</p>

          <label className="field-label" htmlFor="exact-origins">Allowed exact origins</label>
          <textarea
            id="exact-origins"
            value={exactOriginsInput}
            onChange={(event) => setExactOriginsInput(event.target.value)}
            placeholder="https://service.example.de"
          />

          <label className="field-label" htmlFor="domain-suffixes">Allowed domain suffixes</label>
          <textarea
            id="domain-suffixes"
            value={suffixesInput}
            onChange={(event) => setSuffixesInput(event.target.value)}
            placeholder=".bundid.de"
          />

          <div className="actions">
            <button onClick={savePolicy}>Save policy</button>
          </div>

          <p className="subtitle">{policyState}</p>
        </section>

        {runtimeContext ? (
          <section className="runtime-panel">
            <h2>Desktop Runtime</h2>
            <p className="subtitle">
              Desktop: {runtimeContext.desktopEnv ?? "unknown"} | Session: {runtimeContext.sessionType ?? "unknown"}
            </p>
            <p className="subtitle">Tray strategy: {runtimeContext.trayStrategy}</p>
            {runtimeContext.notes.length > 0 ? (
              <ul className="diagnostics-list">
                {runtimeContext.notes.map((note, index) => (
                  <li key={`${note}-${index}`}>{note}</li>
                ))}
              </ul>
            ) : null}
          </section>
        ) : null}
      </section>

      {showPinModal ? (
        <div className="modal-backdrop" role="presentation">
          <section className="pin-modal" role="dialog" aria-modal="true" aria-label="Enter eID PIN">
            <h2>Enter eID PIN</h2>
            <p className="subtitle">Session {sessionUpdate.sessionId}</p>
            <label className="field-label" htmlFor="pin-input">PIN (6 digits)</label>
            <input
              id="pin-input"
              className="pin-input"
              type="password"
              value={pinInput}
              inputMode="numeric"
              maxLength={6}
              autoFocus
              onChange={(event) => setPinInput(event.target.value.replace(/\D+/g, ""))}
              placeholder="******"
            />
            {pinError ? <p className="reader-error">{pinError}</p> : null}
            <div className="actions">
              <button onClick={handleSubmitPin} disabled={submitPinBusy || pinInput.length !== 6}>
                {submitPinBusy ? "Submitting..." : "Submit PIN"}
              </button>
              <button className="secondary" onClick={handleCancelActiveSession} disabled={submitPinBusy}>
                Cancel
              </button>
            </div>
          </section>
        </div>
      ) : null}
    </main>
  );
}
