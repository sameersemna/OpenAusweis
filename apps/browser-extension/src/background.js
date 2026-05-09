const NATIVE_HOST = "org.openausweis.native";
const PROTOCOL_VERSION = 1;
const DEFAULT_ALLOWED_EXACT_ORIGINS = ["http://localhost", "https://localhost"];
const DEFAULT_ALLOWED_SUFFIXES = [".bundid.de", ".bund.de"];

function connectNativeHost() {
  return chrome.runtime.connectNative(NATIVE_HOST);
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
  const stored = await chrome.storage.local.get(["allowedExactOrigins", "allowedSuffixes"]);
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
    case "START_SESSION": {
      const relyingParty = message.relying_party;
      if (typeof relyingParty !== "string" || relyingParty.length === 0) {
        throw new Error("START_SESSION requires relying_party");
      }

      return {
        type: "START_SESSION",
        data: { relying_party: relyingParty },
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
    request_id: crypto.randomUUID(),
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

async function sendNative(requestEnvelope) {
  return new Promise((resolve, reject) => {
    const port = connectNativeHost();
    let settled = false;

    const timeout = setTimeout(() => {
      try {
        port.disconnect();
      } catch (_) {}
      settled = true;
      reject(new Error("Native host timed out"));
    }, 5000);

    port.onMessage.addListener((response) => {
      if (settled) {
        return;
      }

      settled = true;
      clearTimeout(timeout);
      try {
        validateNativeResponse(response, requestEnvelope);
        resolve(response.payload);
      } catch (error) {
        reject(error);
      }
      port.disconnect();
    });

    port.onDisconnect.addListener(() => {
      if (settled) {
        return;
      }

      const error = chrome.runtime.lastError;
      if (error) {
        settled = true;
        clearTimeout(timeout);
        reject(new Error(error.message));
      }
    });

    port.postMessage(requestEnvelope);
  });
}

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  (async () => {
    await validateStartSessionOrigin(message, sender);

    const requestEnvelope = buildRequestEnvelope(message);
    const response = await sendNative(requestEnvelope);
    sendResponse({ ok: true, response });
  })().catch((error) => sendResponse({ ok: false, error: String(error) }));

  return true;
});
