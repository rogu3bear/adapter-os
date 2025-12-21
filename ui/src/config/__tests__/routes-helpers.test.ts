import { describe, expect, it } from 'vitest';
import { formatClusterPrefixedLabel } from '@/config/routes';

describe('formatClusterPrefixedLabel', () => {
  it('prefixes label with cluster when route is known', () => {
    expect(formatClusterPrefixedLabel('/training', 'Training')).toBe('Build / Training');
  });

  it('falls back to label when route cluster is unknown', () => {
    expect(formatClusterPrefixedLabel('/not-a-route', 'Custom')).toBe('Custom');
  });
});


