import type { RepoVersionSummary } from '@/api/repo-types';

export function computeVersionGuards(version: RepoVersionSummary) {
  const serveable = version.serveable ?? false;
  const promoteDisabledReason =
    version.release_state !== 'ready'
      ? 'Only ready versions can be promoted'
      : serveable
      ? undefined
      : version.serveable_reason ?? 'Version is not serveable';

  const trainDisabledReason =
    version.release_state === 'failed'
      ? 'Cannot train from failed version'
      : serveable
      ? undefined
      : version.serveable_reason ?? 'Version is not serveable';

  return { promoteDisabledReason, trainDisabledReason };
}
