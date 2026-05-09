import { jsx as _jsx, jsxs as _jsxs } from "react/jsx-runtime";
import { invoke } from "@tauri-apps/api/tauri";
import { useEffect, useState } from "react";
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
            const response = await invoke("probe_daemon_status");
            setStatus(response.healthy ? "Connected" : "Unhealthy");
            setDetails(`PC/SC: ${response.pcscAvailable ? "available" : "unavailable"} | Sessions: ${response.activeSessionCount}`);
        }
        catch (error) {
            setStatus("Disconnected");
            setDetails(`Probe failed: ${String(error)}`);
        }
    }
    async function loadPolicy() {
        try {
            const policy = await invoke("get_origin_policy");
            setExactOriginsInput(policy.allowedExactOrigins.join("\n"));
            setSuffixesInput(policy.allowedSuffixes.join("\n"));
            setPolicyState("Policy loaded");
        }
        catch (error) {
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
        }
        catch (error) {
            setPolicyState(`Save failed: ${String(error)}`);
        }
    }
    return (_jsx("main", { className: "app-shell", children: _jsxs("section", { className: "card", children: [_jsx("h1", { children: "OpenAusweis" }), _jsx("p", { className: "subtitle", children: "Linux-native German eID desktop companion" }), _jsxs("div", { className: "status-row", children: [_jsx("span", { className: "label", children: "Daemon" }), _jsx("span", { className: "value", children: status })] }), _jsx("p", { className: "subtitle", children: details }), _jsxs("div", { className: "actions", children: [_jsx("button", { onClick: handleProbeDaemon, children: "Probe daemon" }), _jsx("button", { className: "secondary", onClick: loadPolicy, children: "Reload policy" })] }), _jsxs("section", { className: "policy-panel", children: [_jsx("h2", { children: "Origin Policy" }), _jsx("p", { className: "subtitle", children: "Edit trusted relying-party origins and domain suffixes." }), _jsx("label", { className: "field-label", htmlFor: "exact-origins", children: "Allowed exact origins" }), _jsx("textarea", { id: "exact-origins", value: exactOriginsInput, onChange: (event) => setExactOriginsInput(event.target.value), placeholder: "https://service.example.de" }), _jsx("label", { className: "field-label", htmlFor: "domain-suffixes", children: "Allowed domain suffixes" }), _jsx("textarea", { id: "domain-suffixes", value: suffixesInput, onChange: (event) => setSuffixesInput(event.target.value), placeholder: ".bundid.de" }), _jsx("div", { className: "actions", children: _jsx("button", { onClick: savePolicy, children: "Save policy" }) }), _jsx("p", { className: "subtitle", children: policyState })] })] }) }));
}
