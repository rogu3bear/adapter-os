import { describe, it, expect } from 'vitest';
import { estimateAdapterMemory, calculateTotalMemory } from '@/utils/memoryEstimation';
import type { Adapter } from '@/api/adapter-types';

describe('memoryEstimation', () => {
  describe('estimateAdapterMemory', () => {
    it('should return memory_bytes when available', () => {
      const adapter = {
        id: 'test-adapter',
        adapter_id: 'test-adapter',
        name: 'Test Adapter',
        memory_bytes: 10_000_000,
        rank: 16,
        tier: 'warm',
      } as Adapter;

      expect(estimateAdapterMemory(adapter)).toBe(10_000_000);
    });

    it('should estimate memory based on rank and tier when memory_bytes is not available', () => {
      const adapter = {
        id: 'test-adapter',
        adapter_id: 'test-adapter',
        name: 'Test Adapter',
        rank: 16,
        tier: 'warm',
      } as Adapter;

      // Expected: 16 * 4MB * 0.8 = 51,380,224 bytes
      const expected = Math.round(16 * 4 * 1024 * 1024 * 0.8);
      expect(estimateAdapterMemory(adapter)).toBe(expected);
    });

    it('should estimate memory based on rank and tier when memory_bytes is 0', () => {
      const adapter = {
        id: 'test-adapter',
        adapter_id: 'test-adapter',
        name: 'Test Adapter',
        memory_bytes: 0,
        rank: 16,
        tier: 'warm',
      } as Adapter;

      // Expected: 16 * 4MB * 0.8 = 51,380,224 bytes
      const expected = Math.round(16 * 4 * 1024 * 1024 * 0.8);
      expect(estimateAdapterMemory(adapter)).toBe(expected);
    });

    it('should use default rank when not provided', () => {
      const adapter = {
        id: 'test-adapter',
        adapter_id: 'test-adapter',
        name: 'Test Adapter',
        tier: 'warm',
      } as Adapter;

      // Expected: 16 (default) * 4MB * 0.8 = 51,380,224 bytes
      const expected = Math.round(16 * 4 * 1024 * 1024 * 0.8);
      expect(estimateAdapterMemory(adapter)).toBe(expected);
    });

    it('should use default tier when not provided', () => {
      const adapter = {
        id: 'test-adapter',
        adapter_id: 'test-adapter',
        name: 'Test Adapter',
        rank: 16,
      } as Adapter;

      // Expected: 16 * 4MB * 0.8 (warm tier) = 51,380,224 bytes
      const expected = Math.round(16 * 4 * 1024 * 1024 * 0.8);
      expect(estimateAdapterMemory(adapter)).toBe(expected);
    });

    it('should apply persistent tier multiplier correctly', () => {
      const adapter = {
        id: 'test-adapter',
        adapter_id: 'test-adapter',
        name: 'Test Adapter',
        rank: 16,
        tier: 'persistent',
      } as Adapter;

      // Expected: 16 * 4MB * 1.0 = 67,108,864 bytes
      const expected = Math.round(16 * 4 * 1024 * 1024 * 1.0);
      expect(estimateAdapterMemory(adapter)).toBe(expected);
    });

    it('should apply ephemeral tier multiplier correctly', () => {
      const adapter = {
        id: 'test-adapter',
        adapter_id: 'test-adapter',
        name: 'Test Adapter',
        rank: 16,
        tier: 'ephemeral',
      } as Adapter;

      // Expected: 16 * 4MB * 0.6 = 40,265,318 bytes
      const expected = Math.round(16 * 4 * 1024 * 1024 * 0.6);
      expect(estimateAdapterMemory(adapter)).toBe(expected);
    });

    it('should handle different rank values', () => {
      const adapter8 = {
        id: 'test-adapter',
        adapter_id: 'test-adapter',
        name: 'Test Adapter',
        rank: 8,
        tier: 'warm',
      } as Adapter;

      const adapter32 = {
        id: 'test-adapter',
        adapter_id: 'test-adapter',
        name: 'Test Adapter',
        rank: 32,
        tier: 'warm',
      } as Adapter;

      const expected8 = Math.round(8 * 4 * 1024 * 1024 * 0.8);
      const expected32 = Math.round(32 * 4 * 1024 * 1024 * 0.8);

      expect(estimateAdapterMemory(adapter8)).toBe(expected8);
      expect(estimateAdapterMemory(adapter32)).toBe(expected32);
    });
  });

  describe('calculateTotalMemory', () => {
    const adapters: Adapter[] = [
      {
        id: 'adapter-1',
        adapter_id: 'adapter-1',
        name: 'Adapter 1',
        memory_bytes: 10_000_000,
        rank: 16,
        tier: 'warm',
      } as Adapter,
      {
        id: 'adapter-2',
        adapter_id: 'adapter-2',
        name: 'Adapter 2',
        rank: 16,
        tier: 'warm',
      } as Adapter,
      {
        id: 'adapter-3',
        adapter_id: 'adapter-3',
        name: 'Adapter 3',
        memory_bytes: 20_000_000,
        rank: 32,
        tier: 'persistent',
      } as Adapter,
    ];

    it('should calculate total memory for all adapters', () => {
      const adapterIds = ['adapter-1', 'adapter-2', 'adapter-3'];
      const result = calculateTotalMemory(adapterIds, adapters);

      const expected2 = Math.round(16 * 4 * 1024 * 1024 * 0.8);
      const expectedTotal = 10_000_000 + expected2 + 20_000_000;

      expect(result.totalBytes).toBe(expectedTotal);
      expect(result.estimated).toBe(true); // adapter-2 is estimated
      expect(result.missing).toEqual([]);
    });

    it('should mark as estimated when any adapter uses estimation', () => {
      const adapterIds = ['adapter-1', 'adapter-3'];
      const result = calculateTotalMemory(adapterIds, adapters);

      expect(result.totalBytes).toBe(30_000_000);
      expect(result.estimated).toBe(false); // All have memory_bytes
      expect(result.missing).toEqual([]);
    });

    it('should track missing adapter IDs', () => {
      const adapterIds = ['adapter-1', 'adapter-missing', 'adapter-3'];
      const result = calculateTotalMemory(adapterIds, adapters);

      expect(result.missing).toEqual(['adapter-missing']);
      expect(result.totalBytes).toBe(30_000_000);
    });

    it('should handle empty adapter list', () => {
      const adapterIds: string[] = [];
      const result = calculateTotalMemory(adapterIds, adapters);

      expect(result.totalBytes).toBe(0);
      expect(result.estimated).toBe(false);
      expect(result.missing).toEqual([]);
    });

    it('should handle all missing adapters', () => {
      const adapterIds = ['missing-1', 'missing-2'];
      const result = calculateTotalMemory(adapterIds, adapters);

      expect(result.totalBytes).toBe(0);
      expect(result.estimated).toBe(false);
      expect(result.missing).toEqual(['missing-1', 'missing-2']);
    });

    it('should match adapters by adapter_id field', () => {
      const adapterIds = ['adapter-1'];
      const result = calculateTotalMemory(adapterIds, adapters);

      expect(result.totalBytes).toBe(10_000_000);
      expect(result.missing).toEqual([]);
    });

    it('should handle zero memory_bytes as estimated', () => {
      const adaptersWithZero: Adapter[] = [
        {
          id: 'adapter-zero',
          adapter_id: 'adapter-zero',
          name: 'Zero Memory',
          memory_bytes: 0,
          rank: 16,
          tier: 'warm',
        } as Adapter,
      ];

      const result = calculateTotalMemory(['adapter-zero'], adaptersWithZero);

      expect(result.estimated).toBe(true);
      expect(result.totalBytes).toBeGreaterThan(0);
    });
  });
});
