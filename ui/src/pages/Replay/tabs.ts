/**
 * @deprecated Use `useReplayTabRouter()` from '@/hooks/navigation/useTabRouter' instead.
 * This file is retained for backward compatibility only and will be removed in a future version.
 * The new hook provides path-based routing (deep-linkable URLs) instead of hash-based routing.
 *
 * Migration example:
 * ```tsx
 * // OLD:
 * import { resolveReplayTab, replayTabToPath } from '@/pages/Replay/tabs';
 * const activeTab = resolveReplayTab(location.pathname, location.hash);
 * const path = replayTabToPath(activeTab);
 *
 * // NEW:
 * import { useReplayTabRouter } from '@/hooks/navigation/useTabRouter';
 * const { activeTab, setActiveTab, availableTabs, getTabPath } = useReplayTabRouter();
 * ```
 */
export type ReplayTab = 'runs' | 'decision-trace' | 'evidence' | 'compare' | 'export';

export const replayTabOrder: ReplayTab[] = ['runs', 'decision-trace', 'evidence', 'compare', 'export'];

export function resolveReplayTab(pathname: string, hash: string): ReplayTab {
  const normalizedHash = hash.toLowerCase();
  if (normalizedHash.includes('decision-trace')) return 'decision-trace';
  if (normalizedHash.includes('evidence')) return 'evidence';
  if (normalizedHash.includes('compare')) return 'compare';
  if (normalizedHash.includes('export')) return 'export';
  return 'runs';
}

export function replayTabToPath(tab: ReplayTab): string {
  switch (tab) {
    case 'runs':
      return '/replay';
    case 'decision-trace':
      return '/replay#decision-trace';
    case 'evidence':
      return '/replay#evidence';
    case 'compare':
      return '/replay#compare';
    case 'export':
      return '/replay#export';
    default:
      return '/replay';
  }
}

