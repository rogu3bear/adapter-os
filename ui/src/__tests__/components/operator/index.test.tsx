/**
 * Operator Component Index Tests
 *
 * Tests for operator component exports to ensure all components
 * are properly exported from the index file.
 */

import { describe, it, expect } from 'vitest';
import * as OperatorComponents from '@/components/operator';

describe('Operator Component Exports', () => {
  it('exports OperatorChatLayout component', () => {
    expect(OperatorComponents.OperatorChatLayout).toBeDefined();
    expect(typeof OperatorComponents.OperatorChatLayout).toBe('function');
  });

  it('exports ModelStatusBar component', () => {
    expect(OperatorComponents.ModelStatusBar).toBeDefined();
    expect(typeof OperatorComponents.ModelStatusBar).toBe('function');
  });

  it('exports all expected components', () => {
    const expectedExports = ['OperatorChatLayout', 'ModelStatusBar'];
    const actualExports = Object.keys(OperatorComponents);

    expectedExports.forEach((exportName) => {
      expect(actualExports).toContain(exportName);
    });
  });

  it('does not export unexpected components', () => {
    const actualExports = Object.keys(OperatorComponents);
    const expectedExports = ['OperatorChatLayout', 'ModelStatusBar'];

    // All actual exports should be in expected exports
    actualExports.forEach((exportName) => {
      expect(expectedExports).toContain(exportName);
    });
  });
});
