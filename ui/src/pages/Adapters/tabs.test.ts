import { describe, expect, it } from 'vitest';
import { adapterTabToPath, resolveAdaptersTab } from '@/pages/Adapters/tabs';

describe('Adapters tab mapping', () => {
  it('resolves register tab for /adapters/new', () => {
    expect(resolveAdaptersTab('/adapters/new', '')).toBe('register');
  });

  it('resolves policies tab from hash', () => {
    expect(resolveAdaptersTab('/adapters/adapter-1', '#policies', 'adapter-1')).toBe('policies');
  });

  it('builds adapter-scoped paths', () => {
    expect(adapterTabToPath('activations', 'adapter-1')).toBe('/adapters/adapter-1/activations');
    expect(adapterTabToPath('register', 'adapter-1')).toBe('/adapters/new');
  });
});

