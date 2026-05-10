/**
 * Unit tests for background.js pure / stateless helper functions.
 *
 * The module-level `chrome.runtime.onMessage.addListener(...)` call runs on
 * import, so we stub the chrome global before the import happens via a Vitest
 * setup file declared in vitest.config.js.
 */
import { describe, it, expect, beforeAll, vi } from "vitest";

// Stub the chrome global before importing the module so that the top-level
// `chrome.runtime.onMessage.addListener(...)` call at the end of background.js
// does not throw.
beforeAll(() => {
  vi.stubGlobal("chrome", {
    runtime: {
      connectNative: vi.fn(),
      onMessage: { addListener: vi.fn() },
      lastError: null,
    },
    storage: {
      local: { get: vi.fn() },
    },
  });
});

// Dynamic import so that the chrome stub is in place before the module executes.
let clampSessionWaitTimeout;
let parseOrigin;
let isAllowedOrigin;
let buildPayload;
let isRetryableWatchError;
let sessionWaitKey;

beforeAll(async () => {
  const mod = await import("./background.js");
  clampSessionWaitTimeout = mod.clampSessionWaitTimeout;
  parseOrigin = mod.parseOrigin;
  isAllowedOrigin = mod.isAllowedOrigin;
  buildPayload = mod.buildPayload;
  isRetryableWatchError = mod.isRetryableWatchError;
  sessionWaitKey = mod.sessionWaitKey;
});

// ---------------------------------------------------------------------------
// clampSessionWaitTimeout
// ---------------------------------------------------------------------------
describe("clampSessionWaitTimeout", () => {
  it("returns default (120 000) for non-numeric input", () => {
    expect(clampSessionWaitTimeout("abc")).toBe(120_000);
    // null coerces to 0 via Number(null) which is finite, so it gets clamped to min
    expect(clampSessionWaitTimeout(null)).toBe(5_000);
    expect(clampSessionWaitTimeout(undefined)).toBe(120_000);
    expect(clampSessionWaitTimeout(NaN)).toBe(120_000);
    expect(clampSessionWaitTimeout(Infinity)).toBe(120_000);
  });

  it("clamps to minimum (5 000)", () => {
    expect(clampSessionWaitTimeout(0)).toBe(5_000);
    expect(clampSessionWaitTimeout(-1000)).toBe(5_000);
    expect(clampSessionWaitTimeout(100)).toBe(5_000);
    expect(clampSessionWaitTimeout(4999)).toBe(5_000);
  });

  it("clamps to maximum (180 000)", () => {
    expect(clampSessionWaitTimeout(999_999)).toBe(180_000);
    expect(clampSessionWaitTimeout(200_000)).toBe(180_000);
  });

  it("passes through in-range values", () => {
    expect(clampSessionWaitTimeout(60_000)).toBe(60_000);
    expect(clampSessionWaitTimeout(5_000)).toBe(5_000);
    expect(clampSessionWaitTimeout(180_000)).toBe(180_000);
  });

  it("floors floating-point values", () => {
    expect(clampSessionWaitTimeout(30_000.9)).toBe(30_000);
  });

  it("accepts numeric strings", () => {
    expect(clampSessionWaitTimeout("60000")).toBe(60_000);
    expect(clampSessionWaitTimeout("2000")).toBe(5_000); // clamped to min
  });
});

// ---------------------------------------------------------------------------
// parseOrigin
// ---------------------------------------------------------------------------
describe("parseOrigin", () => {
  it("returns origin for valid URLs", () => {
    expect(parseOrigin("https://example.de/path?q=1")).toBe("https://example.de");
    expect(parseOrigin("http://localhost:8080/foo")).toBe("http://localhost:8080");
    expect(parseOrigin("https://sub.bund.de")).toBe("https://sub.bund.de");
  });

  it("returns null for invalid input", () => {
    expect(parseOrigin("not-a-url")).toBeNull();
    expect(parseOrigin("")).toBeNull();
    expect(parseOrigin(null)).toBeNull();
    expect(parseOrigin(undefined)).toBeNull();
    expect(parseOrigin(42)).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// isAllowedOrigin
// ---------------------------------------------------------------------------
describe("isAllowedOrigin", () => {
  const policy = {
    allowedExactOrigins: new Set(["http://localhost", "https://localhost"]),
    allowedSuffixes: [".bundid.de", ".bund.de"],
  };

  it("allows exact-match origins", () => {
    expect(isAllowedOrigin("http://localhost", policy)).toBe(true);
    expect(isAllowedOrigin("https://localhost", policy)).toBe(true);
  });

  it("allows https domains matching allowed suffixes", () => {
    expect(isAllowedOrigin("https://www.bundid.de", policy)).toBe(true);
    expect(isAllowedOrigin("https://auth.bund.de", policy)).toBe(true);
    expect(isAllowedOrigin("https://deep.sub.bund.de", policy)).toBe(true);
  });

  it("allows localhost with any port (hostname match)", () => {
    expect(isAllowedOrigin("http://localhost:3000", policy)).toBe(true);
    expect(isAllowedOrigin("https://localhost:8443", policy)).toBe(true);
  });

  it("rejects non-https suffix matches", () => {
    expect(isAllowedOrigin("http://www.bund.de", policy)).toBe(false);
  });

  it("rejects unrecognised domains", () => {
    expect(isAllowedOrigin("https://evil.example.com", policy)).toBe(false);
  });

  it("rejects empty or non-string origin", () => {
    expect(isAllowedOrigin("", policy)).toBe(false);
    expect(isAllowedOrigin(null, policy)).toBe(false);
    expect(isAllowedOrigin(undefined, policy)).toBe(false);
  });

  it("rejects origin that merely contains an allowed suffix but is not a subdomain", () => {
    // "notbund.de" does not end with ".bund.de"
    expect(isAllowedOrigin("https://notbund.de", policy)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// buildPayload
// ---------------------------------------------------------------------------
describe("buildPayload", () => {
  it("builds GET_STATUS payload", () => {
    expect(buildPayload({ type: "GET_STATUS" })).toEqual({ type: "GET_STATUS" });
  });

  it("builds WATCH_SESSIONS payload with valid interval_ms", () => {
    const payload = buildPayload({ type: "WATCH_SESSIONS", interval_ms: 500 });
    expect(payload).toEqual({ type: "WATCH_SESSIONS", data: { interval_ms: 500 } });
  });

  it("floors WATCH_SESSIONS interval_ms to integer", () => {
    const payload = buildPayload({ type: "WATCH_SESSIONS", interval_ms: 500.9 });
    expect(payload.data.interval_ms).toBe(500);
  });

  it("throws for WATCH_SESSIONS with non-positive interval_ms", () => {
    expect(() => buildPayload({ type: "WATCH_SESSIONS", interval_ms: 0 })).toThrow();
    expect(() => buildPayload({ type: "WATCH_SESSIONS", interval_ms: -1 })).toThrow();
    expect(() => buildPayload({ type: "WATCH_SESSIONS", interval_ms: "abc" })).toThrow();
  });

  it("builds START_SESSION payload with relying_party", () => {
    const payload = buildPayload({
      type: "START_SESSION",
      relying_party: "https://example.bund.de",
    });
    expect(payload.type).toBe("START_SESSION");
    expect(payload.data.relying_party).toBe("https://example.bund.de");
    expect(payload.data.handoff_id).toBeUndefined();
  });

  it("builds START_SESSION payload with optional handoff_id", () => {
    const payload = buildPayload({
      type: "START_SESSION",
      relying_party: "https://example.bund.de",
      handoff_id: "hid-abc-123",
    });
    expect(payload.data.handoff_id).toBe("hid-abc-123");
  });

  it("throws for START_SESSION without relying_party", () => {
    expect(() => buildPayload({ type: "START_SESSION" })).toThrow("requires relying_party");
  });

  it("throws for START_SESSION with empty handoff_id", () => {
    expect(() =>
      buildPayload({ type: "START_SESSION", relying_party: "https://example.bund.de", handoff_id: "" })
    ).toThrow("handoff_id");
  });

  it("builds SUBMIT_PIN payload", () => {
    const payload = buildPayload({ type: "SUBMIT_PIN", session_id: "sess-1", pin: "123456" });
    expect(payload).toEqual({ type: "SUBMIT_PIN", data: { session_id: "sess-1", pin: "123456" } });
  });

  it("throws for SUBMIT_PIN missing pin", () => {
    expect(() => buildPayload({ type: "SUBMIT_PIN", session_id: "sess-1" })).toThrow("requires pin");
  });

  it("builds CANCEL_SESSION payload", () => {
    const payload = buildPayload({ type: "CANCEL_SESSION", session_id: "sess-1" });
    expect(payload).toEqual({ type: "CANCEL_SESSION", data: { session_id: "sess-1" } });
  });

  it("throws for unknown message type", () => {
    expect(() => buildPayload({ type: "UNKNOWN_TYPE" })).toThrow("Unsupported");
  });

  it("throws for null/missing message", () => {
    expect(() => buildPayload(null)).toThrow("Invalid");
    expect(() => buildPayload({})).toThrow("Invalid");
    expect(() => buildPayload({ type: 42 })).toThrow("Invalid");
  });
});

// ---------------------------------------------------------------------------
// isRetryableWatchError
// ---------------------------------------------------------------------------
describe("isRetryableWatchError", () => {
  it("recognises timed-out errors as retryable", () => {
    expect(isRetryableWatchError(new Error("Native host timed out"))).toBe(true);
    expect(isRetryableWatchError(new Error("connection timed out"))).toBe(true);
  });

  it("recognises DAEMON_UNAVAILABLE as retryable", () => {
    expect(isRetryableWatchError(new Error("Daemon error DAEMON_UNAVAILABLE: ..."))).toBe(true);
  });

  it("recognises session stream event errors as retryable", () => {
    expect(isRetryableWatchError(new Error("failed to read session stream event"))).toBe(true);
  });

  it("does NOT treat handoff mismatch errors as retryable", () => {
    expect(
      isRetryableWatchError(new Error("Session handoff mismatch: expected a, got b"))
    ).toBe(false);
  });

  it("does NOT treat generic errors as retryable", () => {
    expect(isRetryableWatchError(new Error("something went wrong"))).toBe(false);
  });

  it("treats 'timed out' in any error message as retryable", () => {
    // The outer-loop message "Session timed out while waiting for completion" also
    // contains "timed out" so isRetryableWatchError returns true for it.
    expect(isRetryableWatchError(new Error("Session timed out while waiting for completion"))).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// sessionWaitKey
// ---------------------------------------------------------------------------
describe("sessionWaitKey", () => {
  it("uses tab id when present", () => {
    expect(sessionWaitKey({ tab: { id: 42 }, url: "https://example.bund.de/page" })).toBe("tab:42");
  });

  it("falls back to origin when no tab id", () => {
    expect(sessionWaitKey({ url: "https://example.bund.de/page" })).toBe(
      "origin:https://example.bund.de"
    );
  });

  it("returns 'unknown' when no tab and invalid url", () => {
    expect(sessionWaitKey({})).toBe("unknown");
    expect(sessionWaitKey(null)).toBe("unknown");
    expect(sessionWaitKey(undefined)).toBe("unknown");
    expect(sessionWaitKey({ url: "not-a-url" })).toBe("unknown");
  });

  it("ignores non-integer tab ids", () => {
    expect(sessionWaitKey({ tab: { id: "abc" }, url: "https://example.bund.de/" })).toBe(
      "origin:https://example.bund.de"
    );
    expect(sessionWaitKey({ tab: { id: 1.5 }, url: "https://example.bund.de/" })).toBe(
      "origin:https://example.bund.de"
    );
    expect(sessionWaitKey({ tab: { id: NaN }, url: "https://example.bund.de/" })).toBe(
      "origin:https://example.bund.de"
    );
  });
});
