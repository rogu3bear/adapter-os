/**
 * Bulk Actions Hook
 *
 * Manages bulk operation state and execution for multi-select scenarios.
 * Provides progress tracking, error handling, and rollback support.
 *
 * Usage:
 * ```tsx
 * const bulkActions = useBulkActions({
 *   onSuccess: () => refetchAdapters(),
 *   onError: (errors) => showErrorDialog(errors),
 * });
 *
 * const handleBulkDelete = async (selectedIds: string[]) => {
 *   await bulkActions.execute(
 *     selectedIds,
 *     async (id) => await deleteAdapter(id),
 *     { confirmationRequired: true }
 *   );
 * };
 * ```
 *
 * Citations:
 * - docs/UI_INTEGRATION.md - Bulk operation patterns
 */

import { useState, useCallback, useRef } from 'react';
import { logger, toError } from '../utils/logger';

export type BulkActionStatus = 'idle' | 'pending' | 'executing' | 'completed' | 'failed' | 'cancelled';

export interface BulkActionError<K extends string | number = string> {
  /** ID of the item that failed */
  itemId: K;
  /** Error that occurred */
  error: Error;
  /** Index in the batch */
  index: number;
}

export interface BulkActionProgress {
  /** Total number of items to process */
  total: number;
  /** Number of completed items (success + failed) */
  completed: number;
  /** Number of successful operations */
  successful: number;
  /** Number of failed operations */
  failed: number;
  /** Current progress percentage (0-100) */
  percentage: number;
  /** ID of the item currently being processed */
  currentItemId: string | number | null;
}

export interface BulkActionResult<K extends string | number = string> {
  /** Items that were successfully processed */
  successfulIds: K[];
  /** Items that failed with their errors */
  failedItems: BulkActionError<K>[];
  /** Whether the operation was cancelled */
  wasCancelled: boolean;
  /** Total execution time in milliseconds */
  executionTimeMs: number;
}

export interface BulkActionOptions {
  /** Stop execution on first error */
  stopOnError?: boolean;
  /** Maximum concurrent operations (default: 1 for sequential) */
  concurrency?: number;
  /** Delay between operations in ms (default: 0) */
  delayBetweenOps?: number;
  /** Confirmation required before execution */
  confirmationRequired?: boolean;
  /** Custom confirmation message */
  confirmationMessage?: string;
  /** Operation name for logging */
  operationName?: string;
}

export interface UseBulkActionsOptions<K extends string | number = string> {
  /** Callback on successful completion */
  onSuccess?: (result: BulkActionResult<K>) => void;
  /** Callback on error (called for each error and on completion if any errors) */
  onError?: (errors: BulkActionError<K>[]) => void;
  /** Callback on progress update */
  onProgress?: (progress: BulkActionProgress) => void;
  /** Callback when execution starts */
  onStart?: (itemCount: number) => void;
  /** Callback on cancellation */
  onCancel?: () => void;
  /** Component name for logging */
  componentName?: string;
}

export interface UseBulkActionsReturn<K extends string | number = string> {
  /** Current status of bulk operation */
  status: BulkActionStatus;
  /** Current progress */
  progress: BulkActionProgress;
  /** Whether operation is currently executing */
  isExecuting: boolean;
  /** Most recent result */
  result: BulkActionResult<K> | null;
  /** Execute bulk operation */
  execute: <T = void>(
    itemIds: K[],
    operation: (id: K, index: number) => Promise<T>,
    options?: BulkActionOptions
  ) => Promise<BulkActionResult<K>>;
  /** Cancel ongoing operation */
  cancel: () => void;
  /** Reset state to idle */
  reset: () => void;
  /** Check if operation can be cancelled */
  canCancel: boolean;
  /** Errors from the most recent operation */
  errors: BulkActionError<K>[];
}

const initialProgress: BulkActionProgress = {
  total: 0,
  completed: 0,
  successful: 0,
  failed: 0,
  percentage: 0,
  currentItemId: null,
};

/**
 * Delay execution for specified milliseconds.
 */
function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Hook for managing bulk operations with progress tracking.
 *
 * @param options - Bulk action configuration options
 * @returns Bulk action state and control functions
 */
export function useBulkActions<K extends string | number = string>(
  options: UseBulkActionsOptions<K> = {}
): UseBulkActionsReturn<K> {
  const {
    onSuccess,
    onError,
    onProgress,
    onStart,
    onCancel,
    componentName = 'useBulkActions',
  } = options;

  const [status, setStatus] = useState<BulkActionStatus>('idle');
  const [progress, setProgress] = useState<BulkActionProgress>(initialProgress);
  const [result, setResult] = useState<BulkActionResult<K> | null>(null);
  const [errors, setErrors] = useState<BulkActionError<K>[]>([]);

  // Cancellation token
  const cancelledRef = useRef(false);
  const executingRef = useRef(false);

  const updateProgress = useCallback(
    (updates: Partial<BulkActionProgress>) => {
      setProgress((prev) => {
        const newProgress = { ...prev, ...updates };
        newProgress.percentage =
          newProgress.total > 0
            ? Math.round((newProgress.completed / newProgress.total) * 100)
            : 0;
        onProgress?.(newProgress);
        return newProgress;
      });
    },
    [onProgress]
  );

  const execute = useCallback(
    async <T = void>(
      itemIds: K[],
      operation: (id: K, index: number) => Promise<T>,
      actionOptions: BulkActionOptions = {}
    ): Promise<BulkActionResult<K>> => {
      const {
        stopOnError = false,
        concurrency = 1,
        delayBetweenOps = 0,
        operationName = 'bulk operation',
      } = actionOptions;

      if (executingRef.current) {
        const error = new Error('Bulk operation already in progress');
        logger.warn('Attempted to start bulk operation while another is running', {
          component: componentName,
          operation: operationName,
        });
        return {
          successfulIds: [],
          failedItems: [],
          wasCancelled: false,
          executionTimeMs: 0,
        };
      }

      const startTime = Date.now();
      cancelledRef.current = false;
      executingRef.current = true;

      setStatus('executing');
      setErrors([]);
      setProgress({
        total: itemIds.length,
        completed: 0,
        successful: 0,
        failed: 0,
        percentage: 0,
        currentItemId: null,
      });

      onStart?.(itemIds.length);

      logger.info(`Starting ${operationName}`, {
        component: componentName,
        operation: operationName,
        itemCount: itemIds.length,
        concurrency,
      });

      const successfulIds: K[] = [];
      const failedItems: BulkActionError<K>[] = [];

      // Process items based on concurrency
      if (concurrency <= 1) {
        // Sequential execution
        for (let i = 0; i < itemIds.length; i++) {
          if (cancelledRef.current) break;

          const id = itemIds[i];
          updateProgress({ currentItemId: id });

          try {
            await operation(id, i);
            successfulIds.push(id);
            updateProgress({
              completed: i + 1,
              successful: successfulIds.length,
            });
          } catch (err) {
            const error: BulkActionError<K> = {
              itemId: id,
              error: err instanceof Error ? err : new Error(String(err)),
              index: i,
            };
            failedItems.push(error);
            setErrors((prev) => [...prev, error]);

            logger.error(`${operationName} failed for item`, {
              component: componentName,
              operation: operationName,
              itemId: String(id),
              index: i,
            }, toError(err));

            updateProgress({
              completed: i + 1,
              failed: failedItems.length,
            });

            if (stopOnError) {
              break;
            }
          }

          // Delay between operations if specified
          if (delayBetweenOps > 0 && i < itemIds.length - 1) {
            await delay(delayBetweenOps);
          }
        }
      } else {
        // Concurrent execution
        const chunks: K[][] = [];
        for (let i = 0; i < itemIds.length; i += concurrency) {
          chunks.push(itemIds.slice(i, i + concurrency));
        }

        let completedCount = 0;
        for (const chunk of chunks) {
          if (cancelledRef.current) break;

          const results = await Promise.allSettled(
            chunk.map((id, chunkIndex) => {
              const globalIndex = completedCount + chunkIndex;
              return operation(id, globalIndex);
            })
          );

          results.forEach((res, chunkIndex) => {
            const globalIndex = completedCount + chunkIndex;
            const id = chunk[chunkIndex];

            if (res.status === 'fulfilled') {
              successfulIds.push(id);
            } else {
              const error: BulkActionError<K> = {
                itemId: id,
                error: res.reason instanceof Error ? res.reason : new Error(String(res.reason)),
                index: globalIndex,
              };
              failedItems.push(error);
              setErrors((prev) => [...prev, error]);
            }
          });

          completedCount += chunk.length;
          updateProgress({
            completed: completedCount,
            successful: successfulIds.length,
            failed: failedItems.length,
            currentItemId: null,
          });

          if (stopOnError && failedItems.length > 0) {
            break;
          }

          // Delay between chunks if specified
          if (delayBetweenOps > 0 && completedCount < itemIds.length) {
            await delay(delayBetweenOps);
          }
        }
      }

      const executionTimeMs = Date.now() - startTime;
      const wasCancelled = cancelledRef.current;

      const finalResult: BulkActionResult<K> = {
        successfulIds,
        failedItems,
        wasCancelled,
        executionTimeMs,
      };

      setResult(finalResult);
      executingRef.current = false;

      if (wasCancelled) {
        setStatus('cancelled');
        onCancel?.();
        logger.info(`${operationName} cancelled`, {
          component: componentName,
          operation: operationName,
          successfulCount: successfulIds.length,
          failedCount: failedItems.length,
          executionTimeMs,
        });
      } else if (failedItems.length > 0) {
        setStatus('failed');
        onError?.(failedItems);
        logger.warn(`${operationName} completed with errors`, {
          component: componentName,
          operation: operationName,
          successfulCount: successfulIds.length,
          failedCount: failedItems.length,
          executionTimeMs,
        });
      } else {
        setStatus('completed');
        onSuccess?.(finalResult);
        logger.info(`${operationName} completed successfully`, {
          component: componentName,
          operation: operationName,
          successfulCount: successfulIds.length,
          executionTimeMs,
        });
      }

      return finalResult;
    },
    [componentName, onStart, onCancel, onError, onSuccess, updateProgress]
  );

  const cancel = useCallback(() => {
    if (executingRef.current) {
      cancelledRef.current = true;
      logger.info('Bulk operation cancellation requested', {
        component: componentName,
      });
    }
  }, [componentName]);

  const reset = useCallback(() => {
    if (!executingRef.current) {
      setStatus('idle');
      setProgress(initialProgress);
      setResult(null);
      setErrors([]);
      cancelledRef.current = false;
    }
  }, []);

  return {
    status,
    progress,
    isExecuting: status === 'executing',
    result,
    execute,
    cancel,
    reset,
    canCancel: status === 'executing',
    errors,
  };
}

export default useBulkActions;
