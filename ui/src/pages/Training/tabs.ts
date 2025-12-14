/**
 * @deprecated Use `useTrainingTabRouter()` from '@/hooks/navigation/useTabRouter' instead.
 * This file is retained for backward compatibility only and will be removed in a future version.
 * The new hook provides path-based routing (deep-linkable URLs) instead of hash-based routing.
 *
 * Migration example:
 * ```tsx
 * // OLD:
 * import { resolveTrainingTab, trainingTabToPath } from '@/pages/Training/tabs';
 * const activeTab = resolveTrainingTab(location.pathname, location.hash, { jobId });
 * const path = trainingTabToPath(activeTab);
 *
 * // NEW:
 * import { useTrainingTabRouter } from '@/hooks/navigation/useTabRouter';
 * const { activeTab, setActiveTab, availableTabs, getTabPath } = useTrainingTabRouter();
 * ```
 */
export type TrainingTab = 'overview' | 'jobs' | 'datasets' | 'templates' | 'artifacts' | 'settings';

export const trainingTabOrder: TrainingTab[] = ['overview', 'jobs', 'datasets', 'templates', 'artifacts', 'settings'];

export function resolveTrainingTab(
  pathname: string,
  hash: string,
  params?: { jobId?: string; datasetId?: string },
): TrainingTab {
  const normalizedHash = hash.toLowerCase();
  const { jobId, datasetId } = params || {};

  if (jobId || pathname.includes('/training/jobs')) return 'jobs';
  if (datasetId || pathname.includes('/training/datasets')) return 'datasets';
  if (pathname.includes('/training/templates')) return 'templates';
  if (normalizedHash.includes('artifacts')) return 'artifacts';
  if (normalizedHash.includes('settings')) return 'settings';
  return 'overview';
}

export function trainingTabToPath(tab: TrainingTab): string {
  switch (tab) {
    case 'overview':
      return '/training';
    case 'jobs':
      return '/training/jobs';
    case 'datasets':
      return '/training/datasets';
    case 'templates':
      return '/training/templates';
    case 'artifacts':
      return '/training#artifacts';
    case 'settings':
      return '/training#settings';
    default:
      return '/training';
  }
}

