import { invoke } from "@tauri-apps/api/tauri";
import { useEffect, useState } from "react";

type DaemonStatus = {
  healthy: boolean;
  pcscAvailable: boolean;
  activeSessionCount: number;
};

type OriginPolicy = {
  allowedExactOrigins: string[];
  allowedSuffixes: string[];
};

export function App() {
  const [status, setStatus] = useState("Disconnected");
  const [details, setDetails] = useState("No probe executed yet");
  const [exactOriginsInput, setExactOriginsInput] = useState("");
  const [suffixesInput, setSuffixesInput] = useState("");
  const [policyState, setPolicyState] = useState("Policy not loaded");

  useEffect(() => {
    void loadPolicy();
  }, []);

  async function handleProbeDaemon() {
    try {
      const response = await invoke<DaemonStatus>("probe_daemon_status");
      setStatus(response.healthy ? "Connected" : "Unhealthy");
      setDetails(
        `PC/SC: ${response.pcscAvailable ? "available" : "unavailable"} | Sessions: ${response.activeSessionCount}`
      );
    } catch (error) {
      setStatus("Disconnected");
      setDetails(`Probe failed: ${String(error)}`);
    }
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
