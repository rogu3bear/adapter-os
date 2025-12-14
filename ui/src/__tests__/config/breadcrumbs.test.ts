/**
 * Breadcrumb Navigation Tests
 * Tests parameterized route resolution in breadcrumb chains
 */

import { describe, it, expect } from 'vitest';
import { getBreadcrumbs } from '@/config/routes';

describe('getBreadcrumbs - Parameterized Route Resolution', () => {
  it('should resolve breadcrumbs for /adapters/:adapterId/lineage', () => {
    const breadcrumbs = getBreadcrumbs('/adapters/abc-123/lineage', { adapterId: 'abc-123' });

    expect(breadcrumbs).toHaveLength(3);
    expect(breadcrumbs[0]).toEqual({ path: '/adapters', label: 'Adapters' });
    expect(breadcrumbs[1]).toEqual({ path: '/adapters/abc-123', label: 'Adapter Detail' });
    expect(breadcrumbs[2]).toEqual({ path: '/adapters/abc-123/lineage', label: 'Lineage' });

    // Verify no :param literals in paths
    breadcrumbs.forEach(crumb => {
      expect(crumb.path).not.toContain(':');
    });
  });

  it('should resolve breadcrumbs for /training/jobs/:jobId/chat', () => {
    const breadcrumbs = getBreadcrumbs('/training/jobs/job-456/chat', { jobId: 'job-456' });

    expect(breadcrumbs).toHaveLength(4);
    expect(breadcrumbs[0]).toEqual({ path: '/training', label: 'Training' });
    expect(breadcrumbs[1]).toEqual({ path: '/training/jobs', label: 'Jobs' });
    expect(breadcrumbs[2]).toEqual({ path: '/training/jobs/job-456', label: 'Job Detail' });
    expect(breadcrumbs[3]).toEqual({ path: '/training/jobs/job-456/chat', label: 'Result Chat' });

    // Verify no :param literals in paths
    breadcrumbs.forEach(crumb => {
      expect(crumb.path).not.toContain(':');
    });
  });

  it('should resolve breadcrumbs for /repos/:repoId/versions/:versionId', () => {
    const breadcrumbs = getBreadcrumbs('/repos/repo-789/versions/v1.2.3', {
      repoId: 'repo-789',
      versionId: 'v1.2.3',
    });

    expect(breadcrumbs).toHaveLength(3);
    expect(breadcrumbs[0]).toEqual({ path: '/repos', label: 'Repositories' });
    expect(breadcrumbs[1]).toEqual({ path: '/repos/repo-789', label: 'Repository Detail' });
    expect(breadcrumbs[2]).toEqual({
      path: '/repos/repo-789/versions/v1.2.3',
      label: 'Version Detail',
    });

    // Verify no :param literals in paths
    breadcrumbs.forEach(crumb => {
      expect(crumb.path).not.toContain(':');
    });
  });

  it('should resolve breadcrumbs for /adapters/:adapterId (single param)', () => {
    const breadcrumbs = getBreadcrumbs('/adapters/test-adapter', { adapterId: 'test-adapter' });

    expect(breadcrumbs).toHaveLength(2);
    expect(breadcrumbs[0]).toEqual({ path: '/adapters', label: 'Adapters' });
    expect(breadcrumbs[1]).toEqual({ path: '/adapters/test-adapter', label: 'Adapter Detail' });

    // Verify no :param literals in paths
    breadcrumbs.forEach(crumb => {
      expect(crumb.path).not.toContain(':');
    });
  });

  it('should handle non-parameterized routes correctly', () => {
    const breadcrumbs = getBreadcrumbs('/adapters', {});

    expect(breadcrumbs).toHaveLength(1);
    expect(breadcrumbs[0]).toEqual({ path: '/adapters', label: 'Adapters' });
  });

  it('should extract params from pathname when not provided', () => {
    // Test auto-extraction of params
    const breadcrumbs = getBreadcrumbs('/adapters/auto-extract/lineage');

    expect(breadcrumbs).toHaveLength(3);
    expect(breadcrumbs[0]).toEqual({ path: '/adapters', label: 'Adapters' });
    expect(breadcrumbs[1]).toEqual({ path: '/adapters/auto-extract', label: 'Adapter Detail' });
    expect(breadcrumbs[2]).toEqual({
      path: '/adapters/auto-extract/lineage',
      label: 'Lineage',
    });

    // Verify no :param literals in paths
    breadcrumbs.forEach(crumb => {
      expect(crumb.path).not.toContain(':');
    });
  });

  it('should handle complex nested parameterized routes', () => {
    const breadcrumbs = getBreadcrumbs('/training/datasets/ds-123/chat', {
      datasetId: 'ds-123',
    });

    // Verify all paths are resolved
    breadcrumbs.forEach(crumb => {
      expect(crumb.path).not.toContain(':');
    });

    // Verify paths are clickable (no param literals)
    expect(breadcrumbs.some(crumb => crumb.path.includes('ds-123'))).toBe(true);
  });

  it('should return empty array for unknown routes', () => {
    const breadcrumbs = getBreadcrumbs('/unknown/route/path', {});

    expect(breadcrumbs).toHaveLength(0);
  });
});
