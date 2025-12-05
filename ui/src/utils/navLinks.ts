export function buildReplayRunsLink(sessionId?: string): string {
  if (sessionId && sessionId.trim().length > 0) {
    return `/replay?sessionId=${encodeURIComponent(sessionId)}#runs`;
  }
  return '/replay#runs';
}

export function buildReplayCompareLink(): string {
  return '/replay#compare';
}

export function buildTelemetryFiltersLink(): string {
  return '/telemetry#filters';
}


