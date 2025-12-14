/**
 * @deprecated Use `useTelemetryTabRouter()` from '@/hooks/navigation/useTabRouter' instead.
 * This file is retained for backward compatibility only and will be removed in a future version.
 * The new hook provides path-based routing (deep-linkable URLs) instead of hash-based routing.
 *
 * Migration example:
 * ```tsx
 * // OLD:
 * import { resolveTelemetryTab, telemetryTabToPath } from '@/pages/Telemetry/tabs';
 * const activeTab = resolveTelemetryTab(location.pathname, location.hash);
 * const path = telemetryTabToPath(activeTab);
 *
 * // NEW:
 * import { useTelemetryTabRouter } from '@/hooks/navigation/useTabRouter';
 * const { activeTab, setActiveTab, availableTabs, getTabPath } = useTelemetryTabRouter();
 * ```
 */
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

