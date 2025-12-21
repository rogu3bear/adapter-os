import { describe, it, expect } from 'vitest';
import {
  isAdaptersTab,
  isAdapterCategory,
  isAdapterState,
  isAdapterHealthFlag,
  isFilterMode,
  type AdaptersTab,
  type FilterMode,
} from '@/utils/typeGuards';

describe('typeGuards', () => {
  describe('isAdaptersTab', () => {
    it('should return true for valid adapter tabs', () => {
      const validTabs: AdaptersTab[] = [
        'overview',
        'activations',
        'usage',
        'lineage',
        'manifest',
        'register',
        'policies',
      ];

      validTabs.forEach(tab => {
        expect(isAdaptersTab(tab)).toBe(true);
      });
    });

    it('should return false for invalid adapter tabs', () => {
      const invalidTabs = ['invalid', 'settings', 'config', '', 'OVERVIEW'];

      invalidTabs.forEach(tab => {
        expect(isAdaptersTab(tab)).toBe(false);
      });
    });
  });

  describe('isAdapterCategory', () => {
    it('should return true for valid adapter categories', () => {
      const validCategories = ['code', 'framework', 'codebase', 'ephemeral'];

      validCategories.forEach(category => {
        expect(isAdapterCategory(category)).toBe(true);
      });
    });

    it('should return false for invalid adapter categories', () => {
      const invalidCategories = ['invalid', 'plugin', 'module', ''];

      invalidCategories.forEach(category => {
        expect(isAdapterCategory(category)).toBe(false);
      });
    });

    it('should return false for null or undefined', () => {
      expect(isAdapterCategory(null)).toBe(false);
      expect(isAdapterCategory(undefined)).toBe(false);
    });
  });

  describe('isAdapterState', () => {
    it('should return true for valid adapter states', () => {
      const validStates = [
        'unloaded',
        'loading',
        'cold',
        'warm',
        'hot',
        'resident',
        'error',
      ];

      validStates.forEach(state => {
        expect(isAdapterState(state)).toBe(true);
      });
    });

    it('should return false for invalid adapter states', () => {
      const invalidStates = ['invalid', 'active', 'inactive', ''];

      invalidStates.forEach(state => {
        expect(isAdapterState(state)).toBe(false);
      });
    });

    it('should return false for null or undefined', () => {
      expect(isAdapterState(null)).toBe(false);
      expect(isAdapterState(undefined)).toBe(false);
    });
  });

  describe('isAdapterHealthFlag', () => {
    it('should return true for valid health flags', () => {
      const validFlags = ['healthy', 'degraded', 'unsafe', 'corrupt', 'unknown'];

      validFlags.forEach(flag => {
        expect(isAdapterHealthFlag(flag)).toBe(true);
      });
    });

    it('should return false for invalid health flags', () => {
      const invalidFlags = ['invalid', 'good', 'bad', ''];

      invalidFlags.forEach(flag => {
        expect(isAdapterHealthFlag(flag)).toBe(false);
      });
    });

    it('should return false for null or undefined', () => {
      expect(isAdapterHealthFlag(null)).toBe(false);
      expect(isAdapterHealthFlag(undefined)).toBe(false);
    });
  });

  describe('isFilterMode', () => {
    it('should return true for valid filter modes', () => {
      const validModes: FilterMode[] = [
        'all',
        'issues',
        'orphans',
        'duplicates',
        'hubs',
        'deprecated',
        'hidden',
      ];

      validModes.forEach(mode => {
        expect(isFilterMode(mode)).toBe(true);
      });
    });

    it('should return false for invalid filter modes', () => {
      const invalidModes = ['invalid', 'active', 'inactive', ''];

      invalidModes.forEach(mode => {
        expect(isFilterMode(mode)).toBe(false);
      });
    });
  });
});
