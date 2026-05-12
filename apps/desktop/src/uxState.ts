export type UxMetrics = {
  onboardingCompletedCount: number;
  authStartedCount: number;
  pinPromptCount: number;
  authCompletedCount: number;
  authFailedCount: number;
  authCancelledCount: number;
  lastAuthStartedAt: string | null;
  lastAuthCompletedAt: string | null;
  lastAuthFailedAt: string | null;
};

export type AuthTimelineEntry = {
  id: string;
  at: string;
  stage: "STARTED" | "PIN" | "COMPLETED" | "FAILED" | "CANCELLED";
  message: string;
  handoffId?: string | null;
  sessionId?: string | null;
};

export function defaultUxMetrics(): UxMetrics {
  return {
    onboardingCompletedCount: 0,
    authStartedCount: 0,
    pinPromptCount: 0,
    authCompletedCount: 0,
    authFailedCount: 0,
    authCancelledCount: 0,
    lastAuthStartedAt: null,
    lastAuthCompletedAt: null,
    lastAuthFailedAt: null,
  };
}

export function parseUxMetrics(value: string | null): UxMetrics {
  if (!value) {
    return defaultUxMetrics();
  }

  try {
    const parsed = JSON.parse(value) as Partial<UxMetrics>;
    return {
      ...defaultUxMetrics(),
      ...parsed,
    };
  } catch {
    return defaultUxMetrics();
  }
}

export function parseAuthTimeline(value: string | null): AuthTimelineEntry[] {
  if (!value) {
    return [];
  }

  try {
    const parsed = JSON.parse(value) as AuthTimelineEntry[];
    if (!Array.isArray(parsed)) {
      return [];
    }

    return parsed.filter((entry) => typeof entry?.id === "string" && typeof entry?.at === "string");
  } catch {
    return [];
  }
}

export function appendTimelineEntry(
  timeline: AuthTimelineEntry[],
  entry: Omit<AuthTimelineEntry, "id" | "at">,
  nowIso: string,
  maxItems = 20
): AuthTimelineEntry[] {
  const next: AuthTimelineEntry = {
    ...entry,
    id: `${nowIso}-${Math.random().toString(16).slice(2, 8)}`,
    at: nowIso,
  };

  return [next, ...timeline].slice(0, maxItems);
}

export function metricsAfterStateTransition(
  metrics: UxMetrics,
  nextState: string | null | undefined,
  nowIso: string
): UxMetrics {
  if (nextState === "COMPLETED") {
    return {
      ...metrics,
      authCompletedCount: metrics.authCompletedCount + 1,
      lastAuthCompletedAt: nowIso,
    };
  }

  if (nextState === "ERROR") {
    return {
      ...metrics,
      authFailedCount: metrics.authFailedCount + 1,
      lastAuthFailedAt: nowIso,
    };
  }

  return metrics;
}

export function pinPromptTransition(
  metrics: UxMetrics,
  state: string | null | undefined,
  sessionId: string | null | undefined,
  lastPinPromptSessionId: string | null
): { metrics: UxMetrics; nextLastPinPromptSessionId: string | null; changed: boolean } {
  if (state !== "PIN_ENTRY" || !sessionId || sessionId === lastPinPromptSessionId) {
    return {
      metrics,
      nextLastPinPromptSessionId: lastPinPromptSessionId,
      changed: false,
    };
  }

  return {
    metrics: {
      ...metrics,
      pinPromptCount: metrics.pinPromptCount + 1,
    },
    nextLastPinPromptSessionId: sessionId,
    changed: true,
  };
}

export function handoffStatusLabelFromState(
  hasSessionId: boolean,
  state: string | null | undefined
): string {
  if (!hasSessionId) {
    return "Waiting for browser sign-in";
  }

  switch (state) {
    case "PIN_ENTRY":
      return "PIN confirmation needed";
    case "CARD_INTERACTION":
      return "Verifying in progress";
    case "COMPLETED":
      return "Return to browser to finish";
    case "ERROR":
      return "Ready to start again";
    default:
      return "Sign-in preparing";
  }
}

export function preferredRelyingPartyFromOrigins(origins: string[]): string {
  const normalized = origins
    .map((origin) => origin.trim())
    .filter((origin) => origin.length > 0 && /^https?:\/\//i.test(origin));

  const firstSecureNonLocalhost = normalized.find((origin) => {
    try {
      const url = new URL(origin);
      return url.protocol === "https:" && url.hostname !== "localhost";
    } catch {
      return false;
    }
  });

  if (firstSecureNonLocalhost) {
    return firstSecureNonLocalhost;
  }

  return normalized[0] ?? "https://localhost";
}
