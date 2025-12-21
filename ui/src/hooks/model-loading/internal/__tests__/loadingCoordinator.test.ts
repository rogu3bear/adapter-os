/**
 * LoadingCoordinator tests
 *
 * Verifies race condition prevention and proper cleanup.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { LoadingCoordinator } from '@/hooks/model-loading/internal/loadingCoordinator';

describe('LoadingCoordinator', () => {
  let coordinator: LoadingCoordinator;

  beforeEach(() => {
    coordinator = new LoadingCoordinator();
  });

  describe('withLock', () => {
    it('executes operation and returns result', async () => {
      const operation = vi.fn().mockResolvedValue('result');

      const result = await coordinator.withLock('key1', operation);

      expect(result).toBe('result');
      expect(operation).toHaveBeenCalledTimes(1);
    });

    it('deduplicates concurrent operations with same key', async () => {
      const operation = vi.fn().mockImplementation(() => {
        return new Promise((resolve) => {
          setTimeout(() => resolve('result'), 100);
        });
      });

      // Start two concurrent operations with same key
      const promise1 = coordinator.withLock('key1', operation);
      const promise2 = coordinator.withLock('key1', operation);

      // Both should return the same result
      const [result1, result2] = await Promise.all([promise1, promise2]);

      expect(result1).toBe('result');
      expect(result2).toBe('result');
      // Operation should only be called once (deduplicated)
      expect(operation).toHaveBeenCalledTimes(1);
    });

    it('allows concurrent operations with different keys', async () => {
      const operation1 = vi.fn().mockResolvedValue('result1');
      const operation2 = vi.fn().mockResolvedValue('result2');

      const [result1, result2] = await Promise.all([
        coordinator.withLock('key1', operation1),
        coordinator.withLock('key2', operation2),
      ]);

      expect(result1).toBe('result1');
      expect(result2).toBe('result2');
      expect(operation1).toHaveBeenCalledTimes(1);
      expect(operation2).toHaveBeenCalledTimes(1);
    });

    it('cleans up operation after completion', async () => {
      const operation = vi.fn().mockResolvedValue('result');

      await coordinator.withLock('key1', operation);

      // Operation should be cleaned up
      expect(coordinator.isLoading('key1')).toBe(false);
      expect(coordinator.getStats().activeOperations).toBe(0);
    });

    it('cleans up operation after error', async () => {
      const operation = vi.fn().mockRejectedValue(new Error('test error'));

      await expect(coordinator.withLock('key1', operation)).rejects.toThrow('test error');

      // Operation should be cleaned up even on error
      expect(coordinator.isLoading('key1')).toBe(false);
      expect(coordinator.getStats().activeOperations).toBe(0);
    });

    it('propagates errors to all waiting callers', async () => {
      const operation = vi.fn().mockImplementation(() => {
        return new Promise((_, reject) => {
          setTimeout(() => reject(new Error('test error')), 100);
        });
      });

      const promise1 = coordinator.withLock('key1', operation);
      const promise2 = coordinator.withLock('key1', operation);

      await expect(promise1).rejects.toThrow('test error');
      await expect(promise2).rejects.toThrow('test error');
      expect(operation).toHaveBeenCalledTimes(1);
    });
  });

  describe('isLoading', () => {
    it('returns true when operation is in progress', async () => {
      const operation = vi.fn().mockImplementation(() => {
        return new Promise((resolve) => {
          setTimeout(() => resolve('result'), 100);
        });
      });

      const promise = coordinator.withLock('key1', operation);

      expect(coordinator.isLoading('key1')).toBe(true);

      await promise;

      expect(coordinator.isLoading('key1')).toBe(false);
    });

    it('returns false when no operation is in progress', () => {
      expect(coordinator.isLoading('key1')).toBe(false);
    });
  });

  describe('getLoadingState', () => {
    it('returns loading state for active operation', async () => {
      const operation = vi.fn().mockImplementation(() => {
        return new Promise((resolve) => {
          setTimeout(() => resolve('result'), 100);
        });
      });

      const promise = coordinator.withLock('key1', operation);

      const state = coordinator.getLoadingState('key1');
      expect(state.isLoading).toBe(true);
      expect(state.startedAt).toBeDefined();
      expect(state.duration).toBeGreaterThanOrEqual(0);

      await promise;

      const stateAfter = coordinator.getLoadingState('key1');
      expect(stateAfter.isLoading).toBe(false);
    });

    it('returns not loading state for inactive operation', () => {
      const state = coordinator.getLoadingState('key1');
      expect(state.isLoading).toBe(false);
      expect(state.startedAt).toBeUndefined();
      expect(state.duration).toBeUndefined();
    });
  });

  describe('getActiveOperations', () => {
    it('returns all active operations', async () => {
      const operation1 = vi.fn().mockImplementation(() => {
        return new Promise((resolve) => {
          setTimeout(() => resolve('result1'), 100);
        });
      });

      const operation2 = vi.fn().mockImplementation(() => {
        return new Promise((resolve) => {
          setTimeout(() => resolve('result2'), 100);
        });
      });

      const promise1 = coordinator.withLock('key1', operation1);
      const promise2 = coordinator.withLock('key2', operation2);

      const active = coordinator.getActiveOperations();
      expect(active.size).toBe(2);
      expect(active.has('key1')).toBe(true);
      expect(active.has('key2')).toBe(true);

      await Promise.all([promise1, promise2]);

      const activeAfter = coordinator.getActiveOperations();
      expect(activeAfter.size).toBe(0);
    });
  });

  describe('clear', () => {
    it('clears all active operations', async () => {
      const operation = vi.fn().mockImplementation(() => {
        return new Promise((resolve) => {
          setTimeout(() => resolve('result'), 100);
        });
      });

      coordinator.withLock('key1', operation);
      coordinator.withLock('key2', operation);

      expect(coordinator.getStats().activeOperations).toBe(2);

      coordinator.clear();

      expect(coordinator.getStats().activeOperations).toBe(0);
    });
  });

  describe('getStats', () => {
    it('returns correct statistics', async () => {
      const operation = vi.fn().mockImplementation(() => {
        return new Promise((resolve) => {
          setTimeout(() => resolve('result'), 100);
        });
      });

      const promise1 = coordinator.withLock('key1', operation);
      const promise2 = coordinator.withLock('key2', operation);

      const stats = coordinator.getStats();
      expect(stats.activeOperations).toBe(2);
      expect(stats.keys).toEqual(['key1', 'key2']);
      expect(stats.oldestOperationMs).toBeGreaterThanOrEqual(0);

      await Promise.all([promise1, promise2]);

      const statsAfter = coordinator.getStats();
      expect(statsAfter.activeOperations).toBe(0);
      expect(statsAfter.keys).toEqual([]);
      expect(statsAfter.oldestOperationMs).toBeUndefined();
    });
  });

  describe('race condition scenarios', () => {
    it('handles rapid concurrent calls', async () => {
      let callCount = 0;
      const operation = vi.fn().mockImplementation(() => {
        callCount++;
        return new Promise((resolve) => {
          setTimeout(() => resolve(`result-${callCount}`), 50);
        });
      });

      // Start 10 concurrent operations with same key
      const promises = Array.from({ length: 10 }, () =>
        coordinator.withLock('key1', operation)
      );

      const results = await Promise.all(promises);

      // All should get the same result from the single operation
      expect(new Set(results).size).toBe(1);
      expect(operation).toHaveBeenCalledTimes(1);
    });

    it('handles sequential operations correctly', async () => {
      const operation = vi.fn().mockResolvedValue('result');

      // Execute operations sequentially
      await coordinator.withLock('key1', operation);
      await coordinator.withLock('key1', operation);
      await coordinator.withLock('key1', operation);

      // Each should start a new operation (not concurrent)
      expect(operation).toHaveBeenCalledTimes(3);
    });
  });
});

/**
 * Copyright JKCA | 2025 James KC Auchterlonie
 */
