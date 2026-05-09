import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

type DaemonStatus = {
  healthy: boolean;
  pcscAvailable: boolean;
  activeSessionCount: number;
  readers: { name: string; cardPresent: boolean; error?: string | null }[];
  diagnostics: string[];
  lastError?: string | null;
};

type OriginPolicy = {
  allowedExactOrigins: string[];
  allowedSuffixes: string[];
};

export function App() {
  const [status, setStatus] = useState("Disconnected");
  const [details, setDetails] = useState("No probe executed yet");
  const [pcscAvailable, setPcscAvailable] = useState(false);
  const [readerStatus, setReaderStatus] = useState<DaemonStatus["readers"]>([]);
  const [diagnostics, setDiagnostics] = useState<string[]>([]);
  const [lastDiagnosticsRunAt, setLastDiagnosticsRunAt] = useState<string | null>(null);
  const [hotplugOpen, setHotplugOpen] = useState(false);
  const [exactOriginsInput, setExactOriginsInput] = useState("");
  const [suffixesInput, setSuffixesInput] = useState("");
  const [policyState, setPolicyState] = useState("Policy not loaded");

  useEffect(() => {
    void loadPolicy();
  }, []);

  useEffect(() => {
    void handleProbeDaemon();
    let unlisten: (() => void) | undefined;

    void listen<DaemonStatus>("daemon-status", (event) => {
      applyDaemonStatus(event.payload);
    }).then((dispose) => {
      unlisten = dispose;
    });

    return () => {
      if (unlisten) {
        unlisten();
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
    try {
      const response = await invoke<DaemonStatus>("probe_daemon_status");
      applyDaemonStatus(response);
    } catch (error) {
      setStatus("Disconnected");
      setDetails(`Probe failed: ${String(error)}`);
      setReaderStatus([]);
      setDiagnostics([`Probe failed: ${String(error)}`]);
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

  return (
    <main className="app-shell">
      <section className="card">
        <h1>OpenAusweis</h1>
        <p className="subtitle">Linux-native German eID desktop companion</p>

        <div className="status-row">
          <span className="label">Daemon</span>
          <span className="value">{status}</span>
        </div>
        <p className="subtitle">{details}</p>

        <div className="actions">
          <button onClick={handleProbeDaemon}>Probe daemon</button>
          <button className="secondary" onClick={loadPolicy}>Reload policy</button>
        </div>

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
      </section>
    </main>
  );
}
