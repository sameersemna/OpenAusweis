const NATIVE_HOST = "org.openausweis.native";
const PROTOCOL_VERSION = 1;
const DEFAULT_ALLOWED_EXACT_ORIGINS = ["http://localhost", "https://localhost"];
const DEFAULT_ALLOWED_SUFFIXES = [".bundid.de", ".bund.de"];
const NATIVE_REQUEST_TIMEOUT_MS = 6000;
const NATIVE_IDLE_DISCONNECT_MS = 15000;
const WATCH_SESSIONS_INTERVAL_MS = 500;
const WATCH_RETRY_DELAY_MS = 200;
const WATCH_IDLE_DELAY_MS = 120;
const SESSION_COMPLETION_TIMEOUT_MS = 120000;
const SESSION_COMPLETION_TIMEOUT_MIN_MS = 5000;
const SESSION_COMPLETION_TIMEOUT_MAX_MS = 180000;
const MAX_ACTIVE_SESSION_WAITS = 32;

const activeSessionWaits = new Map();
const bridgeMetrics = {
  nativeDisconnects: 0,
  nativeTimeouts: 0,
  watchRetries: 0,
  daemonErrors: 0,
  sessionStarts: 0,
  sessionCompletions: 0,
  sessionGuardRejects: 0,
  waitAborts: 0,
  lastError: null,
  updatedAt: null,
};

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

const nativeClient = createNativeClient();

function connectNativeHost() {
  return EXT_API.runtime.connectNative(NATIVE_HOST);
}

function createRequestId() {
  if (globalThis.crypto && typeof globalThis.crypto.randomUUID === "function") {
    return globalThis.crypto.randomUUID();
  }

  const random = Math.random().toString(16).slice(2);
  return `fallback-${Date.now()}-${random}`;
}

function createAbortError(message) {
  const error = new Error(message);
  error.name = "AbortError";
  return error;
}

function clampSessionWaitTimeout(value) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return SESSION_COMPLETION_TIMEOUT_MS;
  }

  return Math.max(
    SESSION_COMPLETION_TIMEOUT_MIN_MS,
    Math.min(SESSION_COMPLETION_TIMEOUT_MAX_MS, Math.floor(parsed))
  );
}

function recordMetric(key, value = null) {
  if (Object.prototype.hasOwnProperty.call(bridgeMetrics, key)) {
    if (typeof bridgeMetrics[key] === "number") {
      bridgeMetrics[key] += 1;
    } else {
      bridgeMetrics[key] = value;
    }
  }

  bridgeMetrics.updatedAt = new Date().toISOString();
}

function setLastBridgeError(error) {
  bridgeMetrics.lastError = String(error);
  bridgeMetrics.updatedAt = new Date().toISOString();
}

function resetBridgeMetrics() {
  bridgeMetrics.nativeDisconnects = 0;
  bridgeMetrics.nativeTimeouts = 0;
  bridgeMetrics.watchRetries = 0;
  bridgeMetrics.daemonErrors = 0;
  bridgeMetrics.sessionStarts = 0;
  bridgeMetrics.sessionCompletions = 0;
  bridgeMetrics.sessionGuardRejects = 0;
  bridgeMetrics.waitAborts = 0;
  bridgeMetrics.lastError = null;
  bridgeMetrics.updatedAt = new Date().toISOString();
}

function createNativeClient() {
  let port = null;
  const pendingByRequestId = new Map();
  let idleDisconnectTimer = null;

  function clearIdleDisconnectTimer() {
    if (idleDisconnectTimer !== null) {
      clearTimeout(idleDisconnectTimer);
      idleDisconnectTimer = null;
    }
  }

  function scheduleIdleDisconnect() {
    clearIdleDisconnectTimer();
    if (!port || pendingByRequestId.size > 0) {
      return;
    }

    idleDisconnectTimer = setTimeout(() => {
      if (!port || pendingByRequestId.size > 0) {
        return;
      }

      try {
        port.disconnect();
      } catch (_) {}

      port = null;
      idleDisconnectTimer = null;
    }, NATIVE_IDLE_DISCONNECT_MS);
  }

  function rejectAllPending(error) {
    for (const pending of pendingByRequestId.values()) {
      clearTimeout(pending.timeoutHandle);
      pending.reject(error);
    }
    pendingByRequestId.clear();
  }

  function ensureConnected() {
    if (port) {
      return port;
    }

    port = connectNativeHost();

    port.onMessage.addListener((response) => {
      const requestId = response?.request_id;
      if (typeof requestId !== "string") {
        return;
      }

      const pending = pendingByRequestId.get(requestId);
      if (!pending) {
        return;
      }

      pendingByRequestId.delete(requestId);
      clearTimeout(pending.timeoutHandle);

      try {
        validateNativeResponse(response, pending.requestEnvelope);
        pending.resolve(response.payload);
      } catch (error) {
        pending.reject(error);
      }

      scheduleIdleDisconnect();
    });

    port.onDisconnect.addListener(() => {
      const runtimeError = EXT_API.runtime.lastError;
      const message = runtimeError?.message || "Native host disconnected";
      const error = new Error(message);
      recordMetric("nativeDisconnects");
      setLastBridgeError(error);
      clearIdleDisconnectTimer();
      rejectAllPending(error);
      port = null;
    });

    return port;
  }

  async function send(requestEnvelope, timeoutMs = NATIVE_REQUEST_TIMEOUT_MS) {
    return new Promise((resolve, reject) => {
      const currentPort = ensureConnected();
      clearIdleDisconnectTimer();

      const timeoutHandle = setTimeout(() => {
        pendingByRequestId.delete(requestEnvelope.request_id);
        recordMetric("nativeTimeouts");
        reject(new Error("Native host timed out"));
        scheduleIdleDisconnect();
      }, timeoutMs);

      pendingByRequestId.set(requestEnvelope.request_id, {
        requestEnvelope,
        resolve,
        reject,
        timeoutHandle,
      });

      try {
        currentPort.postMessage(requestEnvelope);
      } catch (error) {
        pendingByRequestId.delete(requestEnvelope.request_id);
        clearTimeout(timeoutHandle);
        reject(error);
        scheduleIdleDisconnect();
      }
    });
  }

  function disconnect() {
    clearIdleDisconnectTimer();
    if (!port) {
      return;
    }

    try {
      port.disconnect();
    } catch (_) {}
    port = null;
  }

  return {
    send,
    disconnect,
  };
}

function parseOrigin(urlLike) {
  try {
    return new URL(String(urlLike)).origin;
  } catch (_) {
    return null;
  }
}

function isAllowedOrigin(origin, policy) {
  if (typeof origin !== "string" || origin.length === 0) {
    return false;
  }

  if (policy.allowedExactOrigins.has(origin)) {
    return true;
  }

  let parsed;
  try {
    parsed = new URL(origin);
  } catch (_) {
    return false;
  }

  if (parsed.hostname === "localhost") {
    return true;
  }

  if (parsed.protocol !== "https:") {
    return false;
  }

  return policy.allowedSuffixes.some((suffix) => parsed.hostname.endsWith(suffix));
}

async function getOriginPolicy() {
  const stored = await EXT_API.storage.local.get(["allowedExactOrigins", "allowedSuffixes"]);
  const exactOrigins = Array.isArray(stored.allowedExactOrigins)
    ? stored.allowedExactOrigins.filter((value) => typeof value === "string")
    : DEFAULT_ALLOWED_EXACT_ORIGINS;
  const suffixes = Array.isArray(stored.allowedSuffixes)
    ? stored.allowedSuffixes.filter((value) => typeof value === "string")
    : DEFAULT_ALLOWED_SUFFIXES;

  return {
    allowedExactOrigins: new Set(exactOrigins),
    allowedSuffixes: suffixes,
  };
}

async function validateStartSessionOrigin(message, sender) {
  if (message.type !== "START_SESSION") {
    return;
  }

  const senderOrigin = parseOrigin(sender?.url);
  if (!senderOrigin) {
    throw new Error("Unable to determine sender origin");
  }

  if (message.relying_party !== senderOrigin) {
    throw new Error("START_SESSION relying_party must match sender origin");
  }

  const policy = await getOriginPolicy();
  if (!isAllowedOrigin(senderOrigin, policy)) {
    throw new Error(`Origin not allowed by policy: ${senderOrigin}`);
  }
}

function buildPayload(message) {
  if (!message || typeof message !== "object" || typeof message.type !== "string") {
    throw new Error("Invalid browser bridge message");
  }

  switch (message.type) {
    case "GET_STATUS":
      return { type: "GET_STATUS" };
    case "WATCH_SESSIONS": {
      const intervalMs = Number(message.interval_ms);
      if (!Number.isFinite(intervalMs) || intervalMs <= 0) {
        throw new Error("WATCH_SESSIONS requires positive interval_ms");
      }

      return {
        type: "WATCH_SESSIONS",
        data: { interval_ms: Math.floor(intervalMs) },
      };
    }
    case "START_SESSION": {
      const relyingParty = message.relying_party;
      const handoffId = message.handoff_id;
      if (typeof relyingParty !== "string" || relyingParty.length === 0) {
        throw new Error("START_SESSION requires relying_party");
      }

      if (handoffId !== undefined && (typeof handoffId !== "string" || handoffId.length === 0)) {
        throw new Error("START_SESSION handoff_id must be a non-empty string when provided");
      }

      return {
        type: "START_SESSION",
        data: {
          relying_party: relyingParty,
          handoff_id: handoffId,
        },
      };
    }
    case "SUBMIT_PIN": {
      const sessionId = message.session_id;
      const pin = message.pin;
      if (typeof sessionId !== "string" || sessionId.length === 0) {
        throw new Error("SUBMIT_PIN requires session_id");
      }
      if (typeof pin !== "string" || pin.length === 0) {
        throw new Error("SUBMIT_PIN requires pin");
      }

      return {
        type: "SUBMIT_PIN",
        data: { session_id: sessionId, pin },
      };
    }
    case "CANCEL_SESSION": {
      const sessionId = message.session_id;
      if (typeof sessionId !== "string" || sessionId.length === 0) {
        throw new Error("CANCEL_SESSION requires session_id");
      }

      return {
        type: "CANCEL_SESSION",
        data: { session_id: sessionId },
      };
    }
    default:
      throw new Error(`Unsupported bridge message type: ${String(message.type)}`);
  }
}

function buildRequestEnvelope(message) {
  return {
    protocol_version: PROTOCOL_VERSION,
    request_id: createRequestId(),
    payload: buildPayload(message),
  };
}

function validateNativeResponse(response, requestEnvelope) {
  if (!response || typeof response !== "object") {
    throw new Error("Native host returned invalid envelope");
  }

  if (response.protocol_version !== PROTOCOL_VERSION) {
    throw new Error(
      `Protocol mismatch: expected ${PROTOCOL_VERSION}, got ${String(response.protocol_version)}`
    );
  }

  if (response.request_id !== requestEnvelope.request_id) {
    throw new Error(
      `Request correlation mismatch: expected ${requestEnvelope.request_id}, got ${String(response.request_id)}`
    );
  }

  if (!response.payload || typeof response.payload !== "object") {
    throw new Error("Native host payload is missing");
  }
}

function wait(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function sessionWaitKey(sender) {
  const tabId = sender?.tab?.id;
  if (Number.isInteger(tabId)) {
    return `tab:${tabId}`;
  }

  const origin = parseOrigin(sender?.url);
  if (origin) {
    return `origin:${origin}`;
  }

  return "unknown";
}

function maybeAbortWaitForCancelledSession(sessionId) {
  for (const waitState of activeSessionWaits.values()) {
    if (waitState.sessionId === sessionId) {
      recordMetric("waitAborts");
      waitState.abortController.abort();
    }
  }
}

function isRetryableWatchError(error) {
  const message = String(error);
  return (
    message.includes("timed out") ||
    message.includes("session stream event") ||
    message.includes("Native host timed out") ||
    message.includes("DAEMON_UNAVAILABLE")
  );
}

function parseDaemonPayloadError(payload) {
  recordMetric("daemonErrors");
  const code = payload?.data?.code;
  const message = payload?.data?.message;
  const codeText = typeof code === "string" ? code : "UNKNOWN";
  const messageText = typeof message === "string" ? message : "daemon error";
  const error = new Error(`Daemon error ${codeText}: ${messageText}`);
  setLastBridgeError(error);
  return error;
}

async function waitForSessionCompletion(sessionId, options = {}) {
  const timeoutMs = clampSessionWaitTimeout(options.timeoutMs);
  const abortSignal = options.abortSignal;
  const expectedHandoffId =
    typeof options.expectedHandoffId === "string" && options.expectedHandoffId.length > 0
      ? options.expectedHandoffId
      : null;
  const deadline = Date.now() + timeoutMs;

  while (Date.now() < deadline) {
    if (abortSignal?.aborted) {
      throw createAbortError("Session wait aborted");
    }

    try {
      const payload = await nativeClient.send(
        buildRequestEnvelope({ type: "WATCH_SESSIONS", interval_ms: WATCH_SESSIONS_INTERVAL_MS }),
        NATIVE_REQUEST_TIMEOUT_MS
      );

      if (payload?.type === "SESSION_UPDATED") {
        const updateSessionId = payload?.data?.session_id;
        if (updateSessionId !== sessionId) {
          continue;
        }

        if (expectedHandoffId !== null) {
          const updateHandoffId = payload?.data?.handoff_id;
          if (typeof updateHandoffId === "string" && updateHandoffId !== expectedHandoffId) {
            throw new Error(
              `Session handoff mismatch: expected ${expectedHandoffId}, got ${updateHandoffId}`
            );
          }
        }

        const state = payload?.data?.state;
        if (state === "COMPLETED") {
          return payload;
        }

        if (state === "ERROR") {
          const message = payload?.data?.error ?? "Session failed";
          throw new Error(String(message));
        }
      }

      if (payload?.type === "SESSION_CANCELLED") {
        if (payload?.data?.session_id === sessionId) {
          return payload;
        }
      }

      if (payload?.type === "ERROR") {
        throw parseDaemonPayloadError(payload);
      }
    } catch (error) {
      if (abortSignal?.aborted) {
        throw createAbortError("Session wait aborted");
      }

      if (isRetryableWatchError(error)) {
        recordMetric("watchRetries");
        await wait(WATCH_RETRY_DELAY_MS);
        continue;
      }

      setLastBridgeError(error);
      throw error;
    }

    await wait(WATCH_IDLE_DELAY_MS);
  }

  throw new Error("Session timed out while waiting for completion");
}

// Named exports for unit testing — pure/stateless functions only.
// background.js is declared as `"type": "module"` in the manifest so these
// exports are harmless at runtime and fully tree-shakeable by any bundler.
export {
  clampSessionWaitTimeout,
  parseOrigin,
  isAllowedOrigin,
  buildPayload,
  isRetryableWatchError,
  sessionWaitKey,
};

EXT_API.runtime.onMessage.addListener((message, sender, sendResponse) => {
  (async () => {
    if (message?.type === "GET_BRIDGE_DIAGNOSTICS") {
      sendResponse({ ok: true, diagnostics: { ...bridgeMetrics } });
      return;
    }

    if (message?.type === "CLEAR_BRIDGE_DIAGNOSTICS") {
      resetBridgeMetrics();
      sendResponse({ ok: true, diagnostics: { ...bridgeMetrics } });
      return;
    }

    await validateStartSessionOrigin(message, sender);

    if (message?.type === "CANCEL_SESSION") {
      maybeAbortWaitForCancelledSession(message?.session_id);
    }

    if (message?.type === "START_SESSION") {
      const startMessage = {
        ...message,
        handoff_id:
          typeof message.handoff_id === "string" && message.handoff_id.length > 0
            ? message.handoff_id
            : `ext-${createRequestId()}`,
      };

      const key = sessionWaitKey(sender);
      if (activeSessionWaits.has(key)) {
        recordMetric("sessionGuardRejects");
        throw new Error("A session is already in progress for this browser context");
      }

      if (activeSessionWaits.size >= MAX_ACTIVE_SESSION_WAITS) {
        recordMetric("sessionGuardRejects");
        throw new Error("Too many concurrent OpenAusweis session waits");
      }

      const waitState = {
        abortController: new AbortController(),
        sessionId: null,
      };
      activeSessionWaits.set(key, waitState);

      try {
        recordMetric("sessionStarts");
        const requestEnvelope = buildRequestEnvelope(startMessage);
        const response = await nativeClient.send(requestEnvelope, NATIVE_REQUEST_TIMEOUT_MS);

        if (response?.type !== "SESSION_STARTED") {
          if (response?.type === "ERROR") {
            throw parseDaemonPayloadError(response);
          }

          sendResponse({ ok: true, response });
          return;
        }

        const sessionId = response?.data?.session_id;
        const responseHandoffId = response?.data?.handoff_id;
        if (typeof sessionId !== "string" || sessionId.length === 0) {
          throw new Error("SESSION_STARTED did not provide a session_id");
        }

        if (
          typeof responseHandoffId === "string" &&
          responseHandoffId.length > 0 &&
          responseHandoffId !== startMessage.handoff_id
        ) {
          throw new Error(
            `SESSION_STARTED handoff mismatch: expected ${startMessage.handoff_id}, got ${responseHandoffId}`
          );
        }

        waitState.sessionId = sessionId;

        const completionResponse = await waitForSessionCompletion(sessionId, {
          timeoutMs: SESSION_COMPLETION_TIMEOUT_MS,
          abortSignal: waitState.abortController.signal,
          expectedHandoffId: startMessage.handoff_id,
        });

        sendResponse({
          ok: true,
          response: completionResponse,
          sessionStarted: response,
        });
        recordMetric("sessionCompletions");
        return;
      } finally {
        activeSessionWaits.delete(key);
      }
    }

    const requestEnvelope = buildRequestEnvelope(message);
    const response = await nativeClient.send(requestEnvelope, NATIVE_REQUEST_TIMEOUT_MS);

    sendResponse({ ok: true, response });
  })().catch((error) => {
    setLastBridgeError(error);
    sendResponse({ ok: false, error: String(error) });
  });

  return true;
});
