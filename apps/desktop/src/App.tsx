import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import {
  appendTimelineEntry,
  defaultUxMetrics,
  handoffStatusLabelFromState,
  metricsAfterStateTransition,
  parseAuthTimeline,
  parseUxMetrics,
  pinPromptTransition,
  preferredRelyingPartyFromOrigins,
  type AuthTimelineEntry,
  type UxMetrics,
} from "./uxState";

const THEME_STORAGE_KEY = "openausweis-theme";
const ONBOARDING_STORAGE_KEY = "openausweis-onboarding-complete";
const CONTRAST_STORAGE_KEY = "openausweis-high-contrast";
const UX_METRICS_STORAGE_KEY = "openausweis-ux-metrics";
const PREFERRED_RELYING_PARTY_STORAGE_KEY = "openausweis-preferred-relying-party";
const AUTH_TIMELINE_STORAGE_KEY = "openausweis-auth-timeline";
const DEVELOPER_MODE_STORAGE_KEY = "openausweis-developer-mode";
const DIAGNOSTICS_DRAWER_OPEN_STORAGE_KEY = "openausweis-diagnostics-drawer-open";

type ThemePreference = "system" | "light" | "dark";
type ResolvedTheme = "light" | "dark";
type AppView = "home" | "advanced";

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
  handoffId?: string | null;
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
  const [activeView, setActiveView] = useState<AppView>("home");
  const [highContrast, setHighContrast] = useState(false);
  const [onboardingComplete, setOnboardingComplete] = useState(false);
  const [status, setStatus] = useState("Disconnected");
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
  const [lastSessionState, setLastSessionState] = useState<string>("IDLE");
  const [lastPinPromptSessionId, setLastPinPromptSessionId] = useState<string | null>(null);
  const [uxMetrics, setUxMetrics] = useState<UxMetrics>(defaultUxMetrics());
  const [preferredRelyingParty, setPreferredRelyingParty] = useState("https://localhost");
  const [authTimeline, setAuthTimeline] = useState<AuthTimelineEntry[]>([]);
  const [trayActionMessage, setTrayActionMessage] = useState<string | null>(null);
  const [uiAnnouncement, setUiAnnouncement] = useState<string>("OpenAusweis is ready.");
  const [developerModeEnabled, setDeveloperModeEnabled] = useState(false);
  const [diagnosticsDrawerOpen, setDiagnosticsDrawerOpen] = useState(false);

  useEffect(() => {
    const storedPreference = parseStoredTheme(window.localStorage.getItem(THEME_STORAGE_KEY));
    setThemePreference(storedPreference);
    setOnboardingComplete(window.localStorage.getItem(ONBOARDING_STORAGE_KEY) === "true");
    setHighContrast(window.localStorage.getItem(CONTRAST_STORAGE_KEY) === "true");
    setUxMetrics(parseUxMetrics(window.localStorage.getItem(UX_METRICS_STORAGE_KEY)));
    setPreferredRelyingParty(
      window.localStorage.getItem(PREFERRED_RELYING_PARTY_STORAGE_KEY) || "https://localhost"
    );
    setAuthTimeline(parseAuthTimeline(window.localStorage.getItem(AUTH_TIMELINE_STORAGE_KEY)));
    setDeveloperModeEnabled(window.localStorage.getItem(DEVELOPER_MODE_STORAGE_KEY) === "true");
    setDiagnosticsDrawerOpen(
      window.localStorage.getItem(DIAGNOSTICS_DRAWER_OPEN_STORAGE_KEY) === "true"
    );
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
    window.localStorage.setItem(ONBOARDING_STORAGE_KEY, onboardingComplete ? "true" : "false");
  }, [onboardingComplete]);

  useEffect(() => {
    window.localStorage.setItem(CONTRAST_STORAGE_KEY, highContrast ? "true" : "false");
  }, [highContrast]);

  useEffect(() => {
    document.documentElement.setAttribute("data-contrast", highContrast ? "high" : "default");
  }, [highContrast]);

  useEffect(() => {
    window.localStorage.setItem(UX_METRICS_STORAGE_KEY, JSON.stringify(uxMetrics));
  }, [uxMetrics]);

  useEffect(() => {
    window.localStorage.setItem(PREFERRED_RELYING_PARTY_STORAGE_KEY, preferredRelyingParty);
  }, [preferredRelyingParty]);

  useEffect(() => {
    window.localStorage.setItem(AUTH_TIMELINE_STORAGE_KEY, JSON.stringify(authTimeline));
  }, [authTimeline]);

  useEffect(() => {
    window.localStorage.setItem(DEVELOPER_MODE_STORAGE_KEY, developerModeEnabled ? "true" : "false");
  }, [developerModeEnabled]);

  useEffect(() => {
    window.localStorage.setItem(
      DIAGNOSTICS_DRAWER_OPEN_STORAGE_KEY,
      diagnosticsDrawerOpen ? "true" : "false"
    );
  }, [diagnosticsDrawerOpen]);

  // Auto-dismiss tray action confirmation after 5 s so it does not linger.
  useEffect(() => {
    if (!trayActionMessage) {
      return;
    }

    const timer = setTimeout(() => setTrayActionMessage(null), 5000);
    return () => clearTimeout(timer);
  }, [trayActionMessage]);

  useEffect(() => {
    void loadPolicy();
    void loadRuntimeContext();
  }, []);

  useEffect(() => {
    if (sessionUpdate.state === "COMPLETED") {
      setSessionResultMessage("Sign-in complete. Return to your browser to finish.");
      setSessionCompletedAt(new Date().toLocaleTimeString());
    }

    if (sessionUpdate.state === "ERROR") {
      setSessionResultMessage(
        sessionUpdate.error
          ? `Sign-in could not be completed: ${readableSessionError(sessionUpdate.error)}`
          : "Sign-in could not be completed."
      );
      setSessionCompletedAt(new Date().toLocaleTimeString());
    }
  }, [sessionUpdate.state, sessionUpdate.sessionId, sessionUpdate.error]);

  useEffect(() => {
    if (!sessionResultMessage) {
      return;
    }

    const timeoutMs = sessionUpdate.state === "ERROR" ? 12000 : 7000;
    const timer = setTimeout(() => {
      setSessionResultMessage(null);
      setSessionCompletedAt(null);
    }, timeoutMs);

    return () => clearTimeout(timer);
  }, [sessionResultMessage, sessionUpdate.state]);

  useEffect(() => {
    if (sessionUpdate.state === "PIN_ENTRY") {
      setUiAnnouncement("PIN required. Enter your 6-digit PIN in the secure prompt.");
      return;
    }

    if (sessionUpdate.state === "CARD_INTERACTION") {
      setUiAnnouncement("Card verification in progress. Keep your card inserted.");
      return;
    }

    if (sessionUpdate.state === "COMPLETED") {
      setUiAnnouncement("Sign-in completed. Return to your browser tab.");
      return;
    }

    if (sessionUpdate.state === "ERROR") {
      setUiAnnouncement("Sign-in failed. Start again to retry.");
      return;
    }

    if (status !== "Connected") {
      setUiAnnouncement("OpenAusweis is reconnecting.");
      return;
    }

    setUiAnnouncement("Ready for secure sign-in.");
  }, [sessionUpdate.state, status]);

  useEffect(() => {
    const transition = pinPromptTransition(
      uxMetrics,
      sessionUpdate.state,
      sessionUpdate.sessionId,
      lastPinPromptSessionId
    );
    if (transition.changed) {
      setLastPinPromptSessionId(transition.nextLastPinPromptSessionId);
      setUxMetrics(transition.metrics);
      setAuthTimeline((previous) =>
        appendTimelineEntry(
          previous,
          {
            stage: "PIN",
            message: "PIN requested by identity card",
            sessionId: sessionUpdate.sessionId,
            handoffId: sessionUpdate.handoffId,
          },
          new Date().toISOString()
        )
      );
    }
  }, [
    sessionUpdate.state,
    sessionUpdate.sessionId,
    sessionUpdate.handoffId,
    lastPinPromptSessionId,
    uxMetrics,
  ]);

  useEffect(() => {
    if (lastSessionState === sessionUpdate.state) {
      return;
    }

    setLastSessionState(sessionUpdate.state ?? "IDLE");

    const nowIso = new Date().toISOString();
    setUxMetrics((previous) => metricsAfterStateTransition(previous, sessionUpdate.state, nowIso));

    if (sessionUpdate.state === "COMPLETED") {
      setAuthTimeline((previous) =>
        appendTimelineEntry(
          previous,
          {
            stage: "COMPLETED",
            message: "Sign-in completed",
            sessionId: sessionUpdate.sessionId,
            handoffId: sessionUpdate.handoffId,
          },
          nowIso
        )
      );
    }

    if (sessionUpdate.state === "ERROR") {
      setAuthTimeline((previous) =>
        appendTimelineEntry(
          previous,
          {
            stage: "FAILED",
            message: sessionUpdate.error ? `Sign-in failed: ${sessionUpdate.error}` : "Sign-in failed",
            sessionId: sessionUpdate.sessionId,
            handoffId: sessionUpdate.handoffId,
          },
          nowIso
        )
      );
    }
  }, [
    lastSessionState,
    sessionUpdate.state,
    sessionUpdate.error,
    sessionUpdate.sessionId,
    sessionUpdate.handoffId,
  ]);

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
      if (!window.localStorage.getItem(PREFERRED_RELYING_PARTY_STORAGE_KEY)) {
        setPreferredRelyingParty(preferredRelyingPartyFromOrigins(policy.allowedExactOrigins));
      }
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
      if (!allowedExactOrigins.includes(preferredRelyingParty)) {
        setPreferredRelyingParty(preferredRelyingPartyFromOrigins(allowedExactOrigins));
      }
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
      setSessionResultMessage("Sign-in cancelled.");
      setUxMetrics((previous) => ({
        ...previous,
        authCancelledCount: previous.authCancelledCount + 1,
      }));
      setAuthTimeline((previous) =>
        appendTimelineEntry(
          previous,
          {
            stage: "CANCELLED",
            message: "Sign-in cancelled by user",
            sessionId: sessionUpdate.sessionId,
            handoffId: sessionUpdate.handoffId,
          },
          new Date().toISOString()
        )
      );
    } catch (error) {
      setSessionUpdate((previous) => ({
        ...previous,
        error: `Could not cancel authentication: ${String(error)}`,
      }));
    }
  }

  async function handleStartAgainFromRecovery() {
    if (sessionUpdate.sessionId) {
      try {
        await invoke("cancel_session", { sessionId: sessionUpdate.sessionId });
      } catch {
        // Continue with a fresh start attempt even if explicit cancel is no longer needed.
      }
    }

    await handleStartBrowserAuthentication();
  }

  function createDesktopHandoffId(): string {
    const random = Math.random().toString(16).slice(2, 10);
    return `desktop-${Date.now()}-${random}`;
  }

  async function handleStartBrowserAuthentication() {
    const handoffId = createDesktopHandoffId();
    try {
      const update = await invoke<SessionUpdate>("start_desktop_handoff_session", {
        handoffId,
        relyingParty: preferredRelyingParty,
      });
      setSessionUpdate(update);
      setPinError(null);
      setSessionResultMessage(null);
      setSessionCompletedAt(null);
      setLastPinPromptSessionId(null);
      setUxMetrics((previous) => ({
        ...previous,
        authStartedCount: previous.authStartedCount + 1,
        lastAuthStartedAt: new Date().toISOString(),
      }));
      setAuthTimeline((previous) =>
        appendTimelineEntry(
          previous,
          {
            stage: "STARTED",
            message: `Sign-in started for ${preferredRelyingParty}`,
            sessionId: update.sessionId,
            handoffId: update.handoffId ?? handoffId,
          },
          new Date().toISOString()
        )
      );
    } catch (error) {
      setSessionUpdate((previous) => ({
        ...previous,
        error: `Could not start sign-in: ${String(error)}`,
      }));
    }
  }

  async function handleHideWindowToTray() {
    try {
      await getCurrentWindow().hide();
      setTrayActionMessage("OpenAusweis is still running in your system tray.");
      setUiAnnouncement("Window hidden. OpenAusweis is still running in the system tray.");
    } catch (error) {
      setTrayActionMessage(`Could not hide window to tray: ${String(error)}`);
      setUiAnnouncement("Could not hide window to tray.");
    }
  }

  function readableSessionError(value: string): string {
    return value
      .replaceAll("daemon error", "service error")
      .replaceAll("daemon", "service")
        .replaceAll("SESSION_ALREADY_ACTIVE", "A sign-in is already active")
      .replaceAll("NOT_IMPLEMENTED", "not yet available");
  }

  function sessionStateHint(): string {
    if (!sessionUpdate.sessionId) {
      if (status !== "Connected") {
        return "OpenAusweis is reconnecting in the background.";
      }
      return "Waiting for sign-in request from your browser.";
    }

    switch (sessionUpdate.state) {
      case "PIN_ENTRY":
        return "PIN needed. Enter your card PIN to continue.";
      case "CARD_INTERACTION":
        return "Verification in progress. Keep your card inserted until your browser confirms completion.";
      case "COMPLETED":
        return "Sign-in complete. Return to your browser tab to finish.";
      case "ERROR":
        return "Sign-in could not be completed. Start again when you are ready.";
      case "IDLE":
      default:
        return "Waiting for sign-in request from your browser.";
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

  function currentRequestTitle(): string {
    if (!sessionUpdate.sessionId) {
      return "Waiting for sign-in";
    }

    switch (sessionUpdate.state) {
      case "PIN_ENTRY":
        return "PIN confirmation needed";
      case "CARD_INTERACTION":
        return "Card verification in progress";
      case "COMPLETED":
        return "Sign-in completed";
      case "ERROR":
        return "Sign-in needs attention";
      case "IDLE":
      default:
        return "Preparing sign-in";
    }
  }

  function currentRequestActionHint(): string {
    if (!sessionUpdate.sessionId) {
      return "Start sign-in from your browser when you are ready.";
    }

    switch (sessionUpdate.state) {
      case "PIN_ENTRY":
        return "Enter your 6-digit PIN here, then keep your card inserted.";
      case "CARD_INTERACTION":
        return "Verification in progress. Keep your card and reader connected.";
      case "COMPLETED":
        return "Go back to your browser tab to complete sign-in.";
      case "ERROR":
        return "Use Start again for a fresh sign-in attempt.";
      case "IDLE":
      default:
        return "Waiting for your browser sign-in request.";
    }
  }

  function linuxRuntimeHeadline(): string {
    if (!runtimeContext) {
      return "Linux environment loading";
    }

    if (runtimeContext.sessionType === "wayland") {
      return "Wayland session detected";
    }

    if (runtimeContext.sessionType === "x11") {
      return "X11 session detected";
    }

    return "Linux session detected";
  }

  function linuxRuntimeHint(): string {
    if (!runtimeContext) {
      return "Runtime details will appear when available.";
    }

    if (runtimeContext.sessionType === "wayland") {
      return "Tray behavior depends on your desktop shell. If tray visibility is limited, OpenAusweis uses notifications as fallback cues.";
    }

    if (runtimeContext.sessionType === "x11") {
      return "Tray behavior is fully supported in typical X11 desktop environments.";
    }

    return "Open Advanced for desktop-specific runtime notes.";
  }

  function primaryStatusLabel(): string {
    if (sessionUpdate.state === "PIN_ENTRY") {
      return "PIN required to continue";
    }
    if (sessionUpdate.state === "CARD_INTERACTION") {
      return "Sign-in in progress";
    }
    if (sessionUpdate.state === "COMPLETED") {
      return "Sign-in complete";
    }
    if (sessionUpdate.state === "ERROR") {
      return "Sign-in needs attention";
    }
    if (status !== "Connected") {
      return "OpenAusweis is reconnecting";
    }
    if (!pcscAvailable) {
      return "Card access unavailable";
    }
    if (readerStatus.length === 0) {
      return "Reader not detected";
    }
    if (!readerStatus.some((reader) => reader.cardPresent)) {
      return "Insert your ID card";
    }
    return "Ready for secure sign-in";
  }

  function primaryStatusTone(): "ok" | "warn" | "bad" {
    if (sessionUpdate.state === "ERROR") {
      return "bad";
    }
    if (sessionUpdate.state === "PIN_ENTRY" || sessionUpdate.state === "CARD_INTERACTION") {
      return "ok";
    }
    if (status !== "Connected" || !pcscAvailable) {
      return "bad";
    }
    if (readerStatus.length === 0 || !readerStatus.some((reader) => reader.cardPresent)) {
      return "warn";
    }
    return "ok";
  }

  function onboardingStepState(step: "service" | "reader" | "card" | "auth"): "done" | "todo" {
    if (step === "service") {
      return status === "Connected" && sessionUpdate.connected ? "done" : "todo";
    }
    if (step === "reader") {
      return pcscAvailable && readerStatus.length > 0 ? "done" : "todo";
    }
    if (step === "card") {
      return readerStatus.some((reader) => reader.cardPresent) ? "done" : "todo";
    }
    return sessionUpdate.state === "COMPLETED" ? "done" : "todo";
  }

  function onboardingStepHint(step: "service" | "reader" | "card" | "auth"): string {
    if (step === "service") {
      return status === "Connected" && sessionUpdate.connected
        ? "OpenAusweis is ready."
        : "OpenAusweis is reconnecting in the background.";
    }

    if (step === "reader") {
      return pcscAvailable && readerStatus.length > 0
        ? `${readerStatus.length} reader${readerStatus.length === 1 ? "" : "s"} detected.`
        : "Connect your USB card reader.";
    }

    if (step === "card") {
      return readerStatus.some((reader) => reader.cardPresent)
        ? "Card detected and ready."
        : "Insert your ID card into the reader.";
    }

    return sessionUpdate.state === "COMPLETED"
      ? "First sign-in completed."
      : "Start and complete one sign-in.";
  }

  function formatTimestamp(value: string | null): string {
    if (!value) {
      return "-";
    }

    const parsed = new Date(value);
    if (Number.isNaN(parsed.valueOf())) {
      return value;
    }

    return parsed.toLocaleString();
  }

  function clearUxMetrics() {
    setUxMetrics(defaultUxMetrics());
  }

  function clearAuthTimeline() {
    setAuthTimeline([]);
  }

  const hasReader = readerStatus.length > 0;
  const hasCardPresent = readerStatus.some((reader) => reader.cardPresent);
  const canStartAuthentication = status === "Connected" && sessionUpdate.connected;
  const latestAuthTimelineEntry = authTimeline[0] ?? null;
  const showPinModal = sessionUpdate.sessionId && sessionUpdate.state === "PIN_ENTRY";
  const authTone = primaryStatusTone();
  const onboardingVisible = !onboardingComplete;
  const onboardingChecklist: Array<{
    id: "service" | "reader" | "card" | "auth";
    label: string;
    done: boolean;
    hint: string;
  }> = [
    {
      id: "service",
      label: "OpenAusweis is ready",
      done: onboardingStepState("service") === "done",
      hint: onboardingStepHint("service"),
    },
    {
      id: "reader",
      label: "A card reader is detected",
      done: onboardingStepState("reader") === "done",
      hint: onboardingStepHint("reader"),
    },
    {
      id: "card",
      label: "Your ID card is inserted",
      done: onboardingStepState("card") === "done",
      hint: onboardingStepHint("card"),
    },
    {
      id: "auth",
      label: "Complete your first sign-in",
      done: onboardingStepState("auth") === "done",
      hint: onboardingStepHint("auth"),
    },
  ];
  const nextOnboardingStep = onboardingChecklist.find((step) => !step.done) ?? null;

  return (
    <main className="app-shell">
      <p className="sr-only" role="status" aria-live="polite" aria-atomic="true">
        {uiAnnouncement}
      </p>
      <section className="card" aria-label="OpenAusweis main application">
        <header className="top-row">
          <div>
            <p className="eyebrow">Secure identity companion</p>
            <h1>OpenAusweis</h1>
            <p className="subtitle">Calm, private sign-in with your German eID card.</p>
          </div>
        </header>

        <section className="trust-strip" aria-label="Security trust indicators">
          <span className="trust-badge">Local-only processing</span>
          <span className="trust-badge">No cloud storage</span>
          <span className="trust-badge">Official eID integration</span>
        </section>

        <section className="view-switch" role="tablist" aria-label="Primary and advanced views">
          <button
            type="button"
            role="tab"
            aria-selected={activeView === "home"}
            className={activeView === "home" ? "secondary nav-button active" : "secondary nav-button"}
            onClick={() => setActiveView("home")}
          >
            Home
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={activeView === "advanced"}
            className={activeView === "advanced" ? "secondary nav-button active" : "secondary nav-button"}
            onClick={() => setActiveView("advanced")}
          >
            Advanced
          </button>
        </section>

        {activeView === "home" ? (
          <section aria-label="Sign-in home">
            {onboardingVisible ? (
              <section className="onboarding-card" aria-label="First-run onboarding">
                <h2>Before your first sign-in</h2>
                <p className="subtitle">Complete these steps once. OpenAusweis stays available from the tray afterward.</p>
                <ul className="onboarding-list">
                  {onboardingChecklist.map((step) => (
                    <li key={step.id} className={step.done ? "onboarding-step done" : "onboarding-step"}>
                      <span className="onboarding-step-icon" aria-hidden="true">
                        {step.done ? "✓" : ""}
                      </span>
                      <div className="onboarding-step-body">
                        <p className="onboarding-step-row">{step.label}</p>
                        <p className="onboarding-hint">{step.hint}</p>
                      </div>
                    </li>
                  ))}
                </ul>
                {nextOnboardingStep ? (
                  <p className="onboarding-next" role="status" aria-live="polite">
                    Next step: {nextOnboardingStep.label}
                  </p>
                ) : null}
                <div className="actions">
                  <button className="secondary" onClick={handleProbeDaemon} disabled={probeInFlight}>
                    {probeInFlight ? <><span className="btn-spinner" aria-hidden="true" />Checking…</> : "Refresh setup status"}
                  </button>
                  {!sessionUpdate.sessionId && canStartAuthentication ? (
                    <button type="button" onClick={handleStartBrowserAuthentication}>
                      Start first sign-in
                    </button>
                  ) : null}
                  <button
                    type="button"
                    className="secondary"
                    onClick={() => {
                      setOnboardingComplete(true);
                      setUxMetrics((previous) => ({
                        ...previous,
                        onboardingCompletedCount: previous.onboardingCompletedCount + 1,
                      }));
                    }}
                  >
                    Hide onboarding
                  </button>
                </div>
              </section>
            ) : (
              <div className="onboarding-compact">
                <span>Onboarding hidden.</span>
                <button type="button" className="secondary" onClick={() => setOnboardingComplete(false)}>
                  Show setup checklist
                </button>
              </div>
            )}

            <section className={`status-hero tone-${authTone}`} aria-live="polite">
              <p className="status-kicker">Current status</p>
              <h2>{primaryStatusLabel()}</h2>
              <p className="subtitle">{sessionStateHint()}</p>
              <ul className="readiness-strip" aria-label="Readiness overview">
                <li className={status === "Connected" ? "readiness-chip ok" : "readiness-chip warn"}>
                  App: {status === "Connected" ? "Ready" : "Reconnecting"}
                </li>
                <li className={hasReader ? "readiness-chip ok" : "readiness-chip warn"}>
                  Reader: {hasReader ? "Detected" : "Not detected"}
                </li>
                <li className={hasCardPresent ? "readiness-chip ok" : "readiness-chip warn"}>
                  Card: {hasCardPresent ? "Inserted" : "Not inserted"}
                </li>
              </ul>
              <div className="actions hero-actions">
                {sessionUpdate.sessionId ? (
                  <button className="secondary" onClick={handleCancelActiveSession}>
                    Cancel sign-in
                  </button>
                ) : (
                  <button onClick={handleStartBrowserAuthentication} disabled={!canStartAuthentication}>
                    Start sign-in
                  </button>
                )}
                <button className="secondary" onClick={handleProbeDaemon} disabled={probeInFlight}>
                  {probeInFlight ? <><span className="btn-spinner" aria-hidden="true" />Checking…</> : "Refresh status"}
                </button>
              </div>
              {!canStartAuthentication ? (
                <p className="subtle-note">OpenAusweis is reconnecting. Sign-in will be available in a moment.</p>
              ) : null}
            </section>

            <section className="session-panel" data-session-state={sessionUpdate.state ?? "IDLE"}>
              <h2>Current sign-in</h2>
              <div className="request-card" role="status" aria-live="polite" aria-busy={sessionUpdate.state === "CARD_INTERACTION"}>
                <p className="request-title">
                  {sessionUpdate.state === "CARD_INTERACTION" ? (
                    <><span className="breathe-dot" aria-hidden="true" />{currentRequestTitle()}</>
                  ) : currentRequestTitle()}
                </p>
                <div className="status-row session-row">
                  <span className="label">Sign-in progress</span>
                  <span className="value">{handoffStatusLabelFromState(Boolean(sessionUpdate.sessionId), sessionUpdate.state)}</span>
                </div>
              </div>
              <div className="next-action-panel" role="status" aria-live="polite">
                <span className="label">What to do now</span>
                <p className="next-action-text">{currentRequestActionHint()}</p>
                {!sessionUpdate.sessionId ? (
                  <p className="subtitle">Select "Start sign-in" when your reader and card are ready.</p>
                ) : null}
                {sessionUpdate.state === "ERROR" ? (
                  <div className="actions error-recovery-actions">
                    <button
                      type="button"
                      onClick={handleStartAgainFromRecovery}
                      disabled={!canStartAuthentication}
                    >
                      Start again
                    </button>
                  </div>
                ) : null}
              </div>
              {sessionUpdate.state === "COMPLETED" ? (
                <div className="return-to-browser-panel" role="status" aria-live="polite">
                  Your browser is ready — switch back to complete sign-in.
                </div>
              ) : null}
              {sessionUpdate.error ? (
                <p className="reader-error" role="alert">{readableSessionError(sessionUpdate.error)}</p>
              ) : null}
              {sessionResultMessage ? (
                <p className={`session-result${sessionUpdate.state === "ERROR" ? " error" : ""}`}>
                  {sessionResultMessage}
                  {sessionCompletedAt ? ` · ${sessionCompletedAt}` : ""}
                </p>
              ) : null}

              <div className="recent-activity-compact" role="status" aria-live="polite">
                <span className="label">Last activity</span>
                {latestAuthTimelineEntry ? (
                  <span className="value">
                    {latestAuthTimelineEntry.stage} — {latestAuthTimelineEntry.message}
                    <span style={{ color: 'var(--muted)', fontWeight: 400 }}>
                      {" · "}{formatTimestamp(latestAuthTimelineEntry.at)}
                    </span>
                  </span>
                ) : (
                  <span className="subtitle">No sign-in activity yet.</span>
                )}
              </div>
            </section>

            <section className="status-grid" aria-label="Card and tray overview">
              <article className="status-tile">
                <h3>Card reader</h3>
                <p className="value">{hasReader ? "Detected" : "Not detected"}</p>
                <p className="subtitle">{hasReader ? `${readerStatus.length} reader(s) available` : "Connect your USB card reader"}</p>
              </article>
              <article className="status-tile">
                <h3>ID card</h3>
                <p className="value">{hasCardPresent ? "Inserted" : "Not inserted"}</p>
                <p className="subtitle">{hasCardPresent ? "Ready for sign-in" : "Insert card before starting sign-in"}</p>
              </article>
              <article className="status-tile">
                <h3>Tray mode</h3>
                <p className="value">Always on</p>
                <p className="subtitle">Closing this window keeps OpenAusweis available from the system tray.</p>
                <button type="button" className="secondary" onClick={handleHideWindowToTray}>
                  Hide window to tray
                </button>
                {trayActionMessage ? <p className="tray-note">{trayActionMessage}</p> : null}
              </article>
              <article className="status-tile">
                <h3>Linux environment</h3>
                <p className="value">{linuxRuntimeHeadline()}</p>
                <p className="subtitle runtime-note">{linuxRuntimeHint()}</p>
                <button type="button" className="secondary" onClick={() => setActiveView("advanced")}>
                  Open advanced runtime details
                </button>
              </article>
            </section>
          </section>
        ) : (
          <section className="advanced-panel" aria-label="Advanced tools and diagnostics">
            <h2>Advanced tools</h2>
            <p className="subtitle">Technical details and troubleshooting are available here to keep the Home view calm.</p>

            <section className="device-panel">
              <h3>Reader and card details</h3>
              <div className="actions diagnostics-actions">
                <button className="secondary" onClick={runDiagnostics}>Run diagnostics</button>
                <button
                  type="button"
                  className="secondary"
                  onClick={() => setDiagnosticsDrawerOpen((previous) => !previous)}
                  aria-expanded={diagnosticsDrawerOpen}
                >
                  {diagnosticsDrawerOpen ? "Hide diagnostics drawer" : "Show diagnostics drawer"}
                </button>
                <button
                  type="button"
                  className={developerModeEnabled ? "secondary active-toggle" : "secondary"}
                  onClick={() => setDeveloperModeEnabled((previous) => !previous)}
                  aria-pressed={developerModeEnabled}
                >
                  {developerModeEnabled ? "Developer mode: on" : "Developer mode: off"}
                </button>
              </div>
              {lastDiagnosticsRunAt ? <p className="subtitle subtle-note">Last diagnostics run: {lastDiagnosticsRunAt}</p> : null}
              {readerStatus.length === 0 ? (
                <p className="subtitle">No readers detected.</p>
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
                    {hotplugOpen ? "▾" : "▸"} Reader troubleshooting
                  </button>
                  {hotplugOpen ? (
                    <ol className="hotplug-checklist">
                      <li className="hotplug-step">
                        <strong>Check cable and USB port.</strong> Reconnect the reader and try another port.
                      </li>
                      <li className="hotplug-step">
                        <strong>Check smartcard service.</strong> Run <code>systemctl status pcscd</code> and start with <code>sudo systemctl start pcscd</code> if needed.
                      </li>
                      <li className="hotplug-step">
                        <strong>Run terminal scan.</strong> Use <code>pcsc_scan</code> to verify reader detection.
                      </li>
                      <li className="hotplug-step">
                        <strong>Refresh diagnostics.</strong> Use the button above after reconnecting devices.
                      </li>
                    </ol>
                  ) : null}
                </div>
              ) : null}

              {diagnosticsDrawerOpen ? (
                <div className="diagnostics-drawer" role="region" aria-label="Diagnostics drawer">
                  <h3>Diagnostics drawer</h3>
                  <p className="subtitle">
                    {developerModeEnabled
                      ? "Developer mode is active. Detailed diagnostic output is visible below."
                      : "Developer mode is off. Turn it on to see detailed diagnostic output and telemetry."}
                  </p>

                  {developerModeEnabled && diagnostics.length > 0 ? (
                    <>
                      <h4>Diagnostics output</h4>
                      <ul className="diagnostics-list">
                        {diagnostics.map((line, index) => (
                          <li key={`${line}-${index}`}>{line}</li>
                        ))}
                      </ul>
                    </>
                  ) : null}

                  {developerModeEnabled && ipcDiagnostics ? (
                    <div className="ipc-diagnostics">
                      <h4>Connection metrics</h4>
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
                          <span className="metric-label">Validation rejections</span>
                          <span className="metric-value error">{ipcDiagnostics.validationRejections}</span>
                        </div>
                        <div className="metric">
                          <span className="metric-label">Connection failures</span>
                          <span className="metric-value error">{ipcDiagnostics.connectionFailures}</span>
                        </div>
                      </div>
                    </div>
                  ) : null}
                </div>
              ) : null}
            </section>

            <section className="policy-panel">
              <h3>Trusted website policy</h3>
              <p className="subtitle">Manage allowed relying-party origins and domain suffixes.</p>
              <div className="actions">
                <button className="secondary" onClick={loadPolicy}>Reload policy</button>
              </div>

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

              <label className="field-label" htmlFor="preferred-relying-party">Preferred relying-party origin</label>
              <input
                id="preferred-relying-party"
                className="pin-input"
                type="text"
                value={preferredRelyingParty}
                onChange={(event) => setPreferredRelyingParty(event.target.value.trim())}
                placeholder="https://service.example.de"
              />

              <div className="actions">
                <button
                  type="button"
                  className="secondary"
                  onClick={() => {
                    const suggestions = exactOriginsInput
                      .split("\n")
                      .map((value) => value.trim())
                      .filter((value) => value.length > 0);
                    setPreferredRelyingParty(preferredRelyingPartyFromOrigins(suggestions));
                  }}
                >
                  Use recommended origin
                </button>
              </div>

              <div className="actions">
                <button onClick={savePolicy}>Save policy</button>
              </div>
              <p className="subtitle">{policyState}</p>
            </section>

            <section className="history-panel" aria-label="Sign-in history">
              <h3>Sign-in history</h3>
              <p className="subtitle">Recent sign-in events on this device.</p>
              {authTimeline.length === 0 ? (
                <p className="subtitle">No sign-in activity yet.</p>
              ) : (
                <ul className="diagnostics-list">
                  {authTimeline.map((entry) => (
                    <li key={entry.id}>
                      <strong>{entry.stage}</strong> - {entry.message} ({formatTimestamp(entry.at)})
                    </li>
                  ))}
                </ul>
              )}
              <div className="actions">
                <button type="button" className="secondary" onClick={clearAuthTimeline}>
                  Clear activity timeline
                </button>
              </div>
            </section>

            <section className="runtime-panel">
              <h3>Desktop behavior</h3>
              <div className="status-grid compact">
                <article className="status-tile">
                  <h4>Accessibility</h4>
                  <p className="subtitle">Use stronger contrast for readability.</p>
                  <button type="button" className="secondary" onClick={() => setHighContrast((prev) => !prev)}>
                    {highContrast ? "Disable high contrast" : "Enable high contrast"}
                  </button>
                </article>
                <article className="status-tile">
                  <h4>Theme</h4>
                  <p className="subtitle">Current theme: {themePreference}</p>
                  <p className="subtitle">Resolved mode: {resolvedTheme}</p>
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
                </article>
                {runtimeContext ? (
                  <article className="status-tile">
                    <h4>Linux runtime</h4>
                    <p className="subtitle">Desktop: {runtimeContext.desktopEnv ?? "unknown"}</p>
                    <p className="subtitle">Session type: {runtimeContext.sessionType ?? "unknown"}</p>
                    <p className="subtitle">Tray strategy: {runtimeContext.trayStrategy}</p>
                  </article>
                ) : null}
                <article className="status-tile">
                  <h4>UX insights (local only)</h4>
                  <p className="subtitle">Onboarding completed: {uxMetrics.onboardingCompletedCount}</p>
                  <p className="subtitle">Auth started: {uxMetrics.authStartedCount}</p>
                  <p className="subtitle">PIN prompts: {uxMetrics.pinPromptCount}</p>
                  <p className="subtitle">Auth completed: {uxMetrics.authCompletedCount}</p>
                  <p className="subtitle">Auth failed: {uxMetrics.authFailedCount}</p>
                  <p className="subtitle">Auth cancelled: {uxMetrics.authCancelledCount}</p>
                  <p className="subtitle">Last start: {formatTimestamp(uxMetrics.lastAuthStartedAt)}</p>
                  <p className="subtitle">Last complete: {formatTimestamp(uxMetrics.lastAuthCompletedAt)}</p>
                  <p className="subtitle">Last failure: {formatTimestamp(uxMetrics.lastAuthFailedAt)}</p>
                  <button type="button" className="secondary" onClick={clearUxMetrics}>
                    Clear local insights
                  </button>
                </article>
              </div>
              {runtimeContext?.notes.length ? (
                <ul className="diagnostics-list">
                  {runtimeContext.notes.map((note, index) => (
                    <li key={`${note}-${index}`}>{note}</li>
                  ))}
                </ul>
              ) : null}
            </section>
          </section>
        )}
      </section>

      {showPinModal ? (
        <div className="modal-backdrop" role="presentation">
          <section className="pin-modal" role="dialog" aria-modal="true" aria-label="Enter eID PIN">
            <h2>Enter eID PIN</h2>
            <p className="subtitle" id="pin-modal-context">Confirm your sign-in with your card PIN.</p>
            <p className="subtitle" id="pin-modal-help">Use your 6-digit card PIN.</p>
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void handleSubmitPin();
              }}
            >
              <label className="field-label" htmlFor="pin-input">PIN (6 digits)</label>
              <input
                id="pin-input"
                className="pin-input"
                type="password"
                value={pinInput}
                inputMode="numeric"
                maxLength={6}
                autoFocus
                aria-describedby={pinError ? "pin-modal-help pin-modal-error" : "pin-modal-help"}
                aria-invalid={pinError ? "true" : "false"}
                onChange={(event) => setPinInput(event.target.value.replace(/\D+/g, ""))}
                placeholder="******"
              />
              {pinError ? <p className="reader-error" id="pin-modal-error">{pinError}</p> : null}
              <div className="actions">
                <button type="submit" disabled={submitPinBusy || pinInput.length !== 6}>
                  {submitPinBusy ? <><span className="btn-spinner" aria-hidden="true" />Submitting…</> : "Submit PIN"}
                </button>
                <button type="button" className="secondary" onClick={handleCancelActiveSession} disabled={submitPinBusy}>
                  Cancel
                </button>
              </div>
            </form>
          </section>
        </div>
      ) : null}
    </main>
  );
}
