import { describe, expect, it } from "vitest";
import {
  defaultUxMetrics,
  handoffStatusLabelFromState,
  metricsAfterStateTransition,
  pinPromptTransition,
  preferredRelyingPartyFromOrigins,
} from "./uxState";

describe("uxState", () => {
  it("prefers secure non-localhost relying party when available", () => {
    const preferred = preferredRelyingPartyFromOrigins([
      "http://localhost",
      "https://localhost",
      "https://service.bundid.de",
    ]);

    expect(preferred).toBe("https://service.bundid.de");
  });

  it("maps handoff status labels for active states", () => {
    expect(handoffStatusLabelFromState(false, "IDLE")).toBe("Waiting to start");
    expect(handoffStatusLabelFromState(true, "PIN_ENTRY")).toBe("PIN confirmation needed");
    expect(handoffStatusLabelFromState(true, "COMPLETED")).toBe("Browser can finish sign-in");
  });

  it("increments completion and failure counters on transitions", () => {
    const base = defaultUxMetrics();
    const completed = metricsAfterStateTransition(base, "COMPLETED", "2026-05-10T10:00:00.000Z");
    expect(completed.authCompletedCount).toBe(1);
    expect(completed.lastAuthCompletedAt).toBe("2026-05-10T10:00:00.000Z");

    const failed = metricsAfterStateTransition(completed, "ERROR", "2026-05-10T10:05:00.000Z");
    expect(failed.authFailedCount).toBe(1);
    expect(failed.lastAuthFailedAt).toBe("2026-05-10T10:05:00.000Z");
  });

  it("counts PIN prompts once per session", () => {
    const base = defaultUxMetrics();
    const first = pinPromptTransition(base, "PIN_ENTRY", "session-1", null);
    expect(first.changed).toBe(true);
    expect(first.metrics.pinPromptCount).toBe(1);

    const duplicate = pinPromptTransition(
      first.metrics,
      "PIN_ENTRY",
      "session-1",
      first.nextLastPinPromptSessionId
    );
    expect(duplicate.changed).toBe(false);
    expect(duplicate.metrics.pinPromptCount).toBe(1);
  });
});
