import { describe, expect, it } from 'vitest';
import { isAdapterFocused } from './RouterConfigPage';

describe('RouterConfigPage helpers', () => {
  it('detects focused adapter', () => {
    expect(isAdapterFocused('a1', 'a1')).toBe(true);
    expect(isAdapterFocused('a1', 'a2')).toBe(false);
    expect(isAdapterFocused('a1', undefined)).toBe(false);
  });
});

