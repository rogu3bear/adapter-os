import { describe, expect, it } from 'vitest';
import { computeVersionGuards } from '@/pages/Repositories/versionGuards';
import type { RepoVersionSummary } from '@/api/repo-types';

const baseVersion: RepoVersionSummary = {
  id: 'v1',
  version: '1.0.0',
  branch: 'main',
  release_state: 'ready',
  created_at: '2025-01-01T00:00:00Z',
};

describe('computeVersionGuards', () => {
  it('disables promote when not ready', () => {
    const result = computeVersionGuards({ ...baseVersion, release_state: 'draft' });
    expect(result.promoteDisabledReason).toContain('ready');
  });

  it('disables both when not serveable', () => {
    const result = computeVersionGuards({ ...baseVersion, serveable: false, serveable_reason: 'blocked' });
    expect(result.promoteDisabledReason).toBe('blocked');
    expect(result.trainDisabledReason).toBe('blocked');
  });

  it('allows when ready and serveable', () => {
    const result = computeVersionGuards({ ...baseVersion, serveable: true });
    expect(result.promoteDisabledReason).toBeUndefined();
    expect(result.trainDisabledReason).toBeUndefined();
  });
});
