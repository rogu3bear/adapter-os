/**
 * loadingCoordinator - Prevent concurrent loading operations
 *
 * Implements a locking mechanism to ensure only one loading operation
 * per key can run at a time. If a second caller tries to load the same
 * resource, they will wait for the existing operation to complete rather
 * than starting a duplicate request.
 *
 * This prevents race conditions when multiple components try to load
 * the same model/adapter simultaneously.
 *
 * @example
 * ```ts
 * const coordinator = new LoadingCoordinator();
 *
 * // Both calls will share the same operation
 * const promise1 = coordinator.withLock('adapter-123', () => loadAdapter('adapter-123'));
 * const promise2 = coordinator.withLock('adapter-123', () => loadAdapter('adapter-123'));
 *
 * // Only one API call is made, both promises resolve with the same result
 * await Promise.all([promise1, promise2]);
 * ```
 */

import { logger } from '@/utils/logger';

/**
 * Represents a loading operation in progress
 */
interface LoadingOperation<T> {
  /** The promise representing the operation */
  promise: Promise<T>;
  /** Timestamp when the operation started (for debugging) */
  startedAt: number;
}

/**
 * Loading state for a specific key
 */
export interface LoadingState {
  /** Whether an operation is currently in progress */
  isLoading: boolean;
  /** When the operation started (ms since epoch) */
  startedAt?: number;
  /** How long the operation has been running (ms) */
  duration?: number;
}

/**
 * LoadingCoordinator prevents concurrent loading operations for the same resource.
 *
 * Features:
 * - Deduplicates concurrent calls to the same operation
 * - Returns shared promise when operation is already in progress
 * - Automatically cleans up completed operations
 * - Thread-safe (single-threaded JS, but handles async properly)
 * - Provides loading state introspection for debugging
 */
export class LoadingCoordinator {
  /**
   * Map of operation keys to in-progress loading operations
   */
  private operations = new Map<string, LoadingOperation<unknown>>();

  /**
   * Execute an operation with a lock to prevent concurrent duplicate calls.
   *
   * If an operation with the same key is already in progress, this will
   * return the existing promise instead of starting a new operation.
   *
   * @param key - Unique identifier for the operation (e.g., 'adapter:abc123', 'model:qwen7b')
   * @param operation - The async operation to execute
   * @returns Promise that resolves with the operation result
   *
   * @example
   * ```ts
   * // Load adapter with deduplication
   * const result = await coordinator.withLock(
   *   `adapter:${adapterId}`,
   *   () => apiClient.loadAdapter(adapterId)
   * );
   * ```
   */
  async withLock<T>(key: string, operation: () => Promise<T>): Promise<T> {
    // Check if operation is already in progress
    const existingOp = this.operations.get(key);

    if (existingOp) {
      logger.debug('Loading operation already in progress, waiting for existing operation', {
        component: 'LoadingCoordinator',
        operation: 'withLock',
        key,
        startedAt: existingOp.startedAt,
        waitingMs: Date.now() - existingOp.startedAt,
      });

      // Return the existing promise (type assertion safe because we control the map)
      return existingOp.promise as Promise<T>;
    }

    // Start new operation
    logger.debug('Starting new loading operation', {
      component: 'LoadingCoordinator',
      operation: 'withLock',
      key,
    });

    const startedAt = Date.now();

    // Create the operation promise
    const promise = (async () => {
      try {
        const result = await operation();

        logger.debug('Loading operation completed successfully', {
          component: 'LoadingCoordinator',
          operation: 'withLock',
          key,
          durationMs: Date.now() - startedAt,
        });

        return result;
      } catch (error) {
        logger.error('Loading operation failed', {
          component: 'LoadingCoordinator',
          operation: 'withLock',
          key,
          durationMs: Date.now() - startedAt,
          error: error instanceof Error ? error.message : String(error),
        });

        throw error;
      } finally {
        // Clean up completed operation
        this.operations.delete(key);

        logger.debug('Removed completed operation from coordinator', {
          component: 'LoadingCoordinator',
          operation: 'withLock',
          key,
          remainingOperations: this.operations.size,
        });
      }
    })();

    // Store the operation
    this.operations.set(key, { promise, startedAt });

    return promise as Promise<T>;
  }

  /**
   * Check if an operation is currently in progress for a given key.
   *
   * @param key - The operation key to check
   * @returns true if the operation is in progress, false otherwise
   *
   * @example
   * ```ts
   * if (coordinator.isLoading('adapter:abc123')) {
   *   console.log('Adapter is already loading');
   * }
   * ```
   */
  isLoading(key: string): boolean {
    return this.operations.has(key);
  }

  /**
   * Get the loading state for a specific key.
   *
   * @param key - The operation key to check
   * @returns Loading state information
   *
   * @example
   * ```ts
   * const state = coordinator.getLoadingState('adapter:abc123');
   * if (state.isLoading) {
   *   console.log(`Loading for ${state.duration}ms`);
   * }
   * ```
   */
  getLoadingState(key: string): LoadingState {
    const operation = this.operations.get(key);

    if (!operation) {
      return { isLoading: false };
    }

    return {
      isLoading: true,
      startedAt: operation.startedAt,
      duration: Date.now() - operation.startedAt,
    };
  }

  /**
   * Get all active loading operations (for debugging).
   *
   * @returns Map of operation keys to loading states
   *
   * @example
   * ```ts
   * const active = coordinator.getActiveOperations();
   * console.log(`${active.size} operations in progress`);
   * ```
   */
  getActiveOperations(): Map<string, LoadingState> {
    const states = new Map<string, LoadingState>();

    for (const [key, operation] of this.operations.entries()) {
      states.set(key, {
        isLoading: true,
        startedAt: operation.startedAt,
        duration: Date.now() - operation.startedAt,
      });
    }

    return states;
  }

  /**
   * Cancel all in-progress operations (for cleanup on unmount).
   *
   * Note: This doesn't actually cancel the underlying promises,
   * it just clears the coordinator's tracking. The operations
   * will continue running but won't be deduplicated.
   *
   * @example
   * ```ts
   * useEffect(() => {
   *   return () => {
   *     coordinator.clear();
   *   };
   * }, []);
   * ```
   */
  clear(): void {
    const count = this.operations.size;

    if (count > 0) {
      logger.warn('Clearing all loading operations', {
        component: 'LoadingCoordinator',
        operation: 'clear',
        operationCount: count,
        keys: Array.from(this.operations.keys()),
      });
    }

    this.operations.clear();
  }

  /**
   * Get statistics about the coordinator (for monitoring).
   *
   * @returns Statistics object
   */
  getStats(): {
    activeOperations: number;
    oldestOperationMs?: number;
    keys: string[];
  } {
    const now = Date.now();
    let oldestOperationMs: number | undefined;

    for (const operation of this.operations.values()) {
      const duration = now - operation.startedAt;
      if (oldestOperationMs === undefined || duration > oldestOperationMs) {
        oldestOperationMs = duration;
      }
    }

    return {
      activeOperations: this.operations.size,
      oldestOperationMs,
      keys: Array.from(this.operations.keys()),
    };
  }
}

/**
 * Singleton instance of the loading coordinator.
 *
 * Use this shared instance across all hooks to ensure proper
 * deduplication across the entire application.
 *
 * @example
 * ```ts
 * import { loadingCoordinator } from '@/hooks/model-loading/internal/loadingCoordinator';
 *
 * export function useAdapterLoader() {
 *   const load = async (id: string) => {
 *     return loadingCoordinator.withLock(`adapter:${id}`, async () => {
 *       return apiClient.loadAdapter(id);
 *     });
 *   };
 *
 *   return { load };
 * }
 * ```
 */
export const loadingCoordinator = new LoadingCoordinator();

/**
 * Copyright JKCA | 2025 James KC Auchterlonie
 */
