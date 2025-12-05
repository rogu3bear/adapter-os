export type TelemetryTab = 'event-stream' | 'viewer' | 'alerts' | 'exports' | 'filters';

export const telemetryTabOrder: TelemetryTab[] = ['event-stream', 'viewer', 'alerts', 'exports', 'filters'];

export function resolveTelemetryTab(pathname: string, hash: string): TelemetryTab {
  const normalizedHash = hash.toLowerCase();
  if (pathname.includes('/telemetry/viewer')) return 'viewer';
  if (normalizedHash.includes('alerts')) return 'alerts';
  if (normalizedHash.includes('exports')) return 'exports';
  if (normalizedHash.includes('filters')) return 'filters';
  return 'event-stream';
}

export function telemetryTabToPath(tab: TelemetryTab): string {
  switch (tab) {
    case 'event-stream':
      return '/telemetry';
    case 'viewer':
      return '/telemetry/viewer';
    case 'alerts':
      return '/telemetry#alerts';
    case 'exports':
      return '/telemetry#exports';
    case 'filters':
      return '/telemetry#filters';
    default:
      return '/telemetry';
  }
}

