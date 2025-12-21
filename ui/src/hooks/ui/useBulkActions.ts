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
import { logger, toError } from '@/utils/logger';
import type {
  BulkActionStatus,
  BulkActionError,
  BulkActionProgress,
  BulkActionResult,
  BulkActionOptions,
  UseBulkActionsOptions,
  UseBulkActionsReturn,
} from '@/types/hooks';

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
