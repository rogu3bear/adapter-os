/**
 * Navigation link builders for common routes.
 * Updated to use path-based routing instead of hash-based routing.
 */

export function buildReplayRunsLink(sessionId?: string): string {
  if (sessionId && sessionId.trim().length > 0) {
    return `/replay?sessionId=${encodeURIComponent(sessionId)}`;
  }
  return '/replay';
}

export function buildReplayCompareLink(): string {
  return '/replay/compare';
}

export function buildTelemetryFiltersLink(): string {
  return '/telemetry/filters';
}


