import { useMemo } from 'react';
import type { RepoVersionSummary } from '@/api/repo-types';

export interface VersionGuards {
  promoteDisabledReason?: string;
  trainDisabledReason?: string;
}

/**
 * Computes guard states for repository version actions.
 * This is the core logic function that can be used standalone or within the hook.
 *
 * @param version - Repository version summary
 * @returns Guard states indicating whether actions are allowed and why they might be disabled
 */
export function computeVersionGuards(version: RepoVersionSummary): VersionGuards {
  const serveable = version.serveable ?? false;
  const state = version.release_state as string;

  const promoteDisabledReason =
    state !== 'ready'
      ? 'Only ready versions can be promoted'
      : serveable
      ? undefined
      : version.serveable_reason ?? 'Version is not serveable';

  const trainDisabledReason =
    state === 'failed'
      ? 'Cannot train from failed version'
      : serveable
      ? undefined
      : version.serveable_reason ?? 'Version is not serveable';

  return { promoteDisabledReason, trainDisabledReason };
}

/**
 * Hook to compute guard states for repository version actions.
 * Uses memoization to avoid recalculating when version properties haven't changed.
 *
 * @param version - Repository version summary
 * @returns Guard states indicating whether actions are allowed and why they might be disabled
 *
 * @example
 * ```tsx
 * const { promoteDisabledReason, trainDisabledReason } = useVersionGuards(version);
 *
 * <Button
 *   disabled={Boolean(promoteDisabledReason)}
 *   title={promoteDisabledReason}
 * >
 *   Promote
 * </Button>
 * ```
 */
export function useVersionGuards(version: RepoVersionSummary): VersionGuards {
  return useMemo(
    () => computeVersionGuards(version),
    [version.serveable, version.release_state, version.serveable_reason]
  );
}
