/**
 * useModelLoader - Hook for loading base models and adapters with retry logic
 *
 * Provides coordinated loading of base models and adapters from a stack,
 * with cancellation support, retry logic for failed adapters, and race prevention.
 *
 * @example
 * ```tsx
 * const {
 *   loadModels,
 *   cancelLoading,
 *   retryFailed,
 *   isLoadingBaseModel,
 *   isLoadingAdapters,
 *   error,
 *   clearError,
 * } = useModelLoader();
 *
 * // Load base model and all adapters from a stack
 * await loadModels('my-stack-id');
 *
 * // Cancel in-progress loading
 * cancelLoading();
 *
 * // Retry specific failed adapters
 * await retryFailed(['adapter-1', 'adapter-2']);
 * ```
 */

import { useState, useCallback, useRef } from 'react';
import apiClient from '@/api/client';
import { logger, toError } from '@/utils/logger';
import { retryWithBackoff, DEFAULT_RETRY_CONFIG } from '@/utils/retry';
import { toast } from 'sonner';
import type { AdapterStack } from '@/api/types';
import { loadingCoordinator } from './internal/loadingCoordinator';

// ============================================================================
// Types
// ============================================================================

export interface ModelLoaderError {
  message: string;
  code: 'STACK_NOT_FOUND' | 'BASE_MODEL_LOAD_FAILED' | 'ADAPTERS_LOAD_FAILED' | 'CANCELLED' | 'UNKNOWN';
  details?: Record<string, unknown>;
  failedAdapters?: Array<{ adapterId: string; error: string }>;
}

export interface UseModelLoaderResult {
  // State
  /** True if base model is currently loading */
  isLoadingBaseModel: boolean;
  /** True if adapters are currently loading */
  isLoadingAdapters: boolean;
  /** True if any loading operation is in progress */
  isLoading: boolean;
  /** Error details if loading failed */
  error: ModelLoaderError | null;
  /** List of adapter IDs that failed to load (for targeted retry) */
  failedAdapterIds: string[];

  // Actions
  /** Load base model (if needed) and all adapters from a stack */
  loadModels: (stackId: string) => Promise<void>;
  /** Cancel in-progress loading operation */
  cancelLoading: () => void;
  /** Retry loading specific failed adapters, or all failed if no IDs provided */
  retryFailed: (adapterIds?: string[]) => Promise<void>;
  /** Clear error state */
  clearError: () => void;
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * Hook for coordinated loading of base models and adapters
 *
 * Features:
 * - Sequential loading: base model first, then adapters
 * - Race prevention: prevents concurrent loading operations (via LoadingCoordinator)
 * - Cancellation: supports aborting in-progress loads
 * - Retry logic: exponential backoff for transient failures
 * - Error tracking: tracks which adapters failed for targeted retry
 */
export function useModelLoader(): UseModelLoaderResult {
  // State
  const [isLoadingBaseModel, setIsLoadingBaseModel] = useState(false);
  const [isLoadingAdapters, setIsLoadingAdapters] = useState(false);
  const [error, setError] = useState<ModelLoaderError | null>(null);
  const [failedAdapterIds, setFailedAdapterIds] = useState<string[]>([]);

  // Refs
  const currentStackRef = useRef<AdapterStack | null>(null);
  const abortControllerRef = useRef<AbortController | null>(null);
  const coordinatorRef = useRef(loadingCoordinator);

  // Clear error state
  const clearError = useCallback(() => {
    setError(null);
    setFailedAdapterIds([]);
  }, []);

  // Cancel loading
  const cancelLoading = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }

    setIsLoadingBaseModel(false);
    setIsLoadingAdapters(false);

    const cancelError: ModelLoaderError = {
      message: 'Loading operation was cancelled',
      code: 'CANCELLED',
    };
    setError(cancelError);

    logger.info('Loading operation cancelled by user', {
      component: 'useModelLoader',
      operation: 'cancelLoading',
    });
  }, []);

  // Load base model (if needed)
  const loadBaseModel = useCallback(async (stack: AdapterStack, abortSignal?: AbortSignal): Promise<boolean> => {
    setIsLoadingBaseModel(true);

    try {
      // Check if a base model is already loaded
      // Note: We need to determine if a model load is needed. For now, we'll attempt to load
      // the first available model. In a real implementation, you'd check model status first.

      // Get list of available models
      const models = await apiClient.listModels();

      if (abortSignal?.aborted) {
        return false;
      }

      if (!Array.isArray(models) || models.length === 0) {
        throw new Error('No base models available. Please import a model first.');
      }

      // Check if a model is already loaded
      const loadedModel = models.find((m) => m.import_status === 'loaded');
      if (loadedModel) {
        logger.info('Base model already loaded, skipping', {
          component: 'useModelLoader',
          modelId: loadedModel.id,
          modelName: loadedModel.name,
        });
        return true;
      }

      // Find the first available model to load
      const availableModel = models.find(
        (m) => m.import_status === 'available' || m.import_status === 'ready'
      );

      if (!availableModel) {
        // Try loading the first model regardless of status
        const firstModel = models[0];
        if (!firstModel) {
          throw new Error('No models found to load');
        }

        logger.info('Loading base model', {
          component: 'useModelLoader',
          modelId: firstModel.id,
          modelName: firstModel.name,
        });

        const result = await retryWithBackoff(
          () => apiClient.loadBaseModel(firstModel.id),
          DEFAULT_RETRY_CONFIG,
          undefined,
          `load base model ${firstModel.name || firstModel.id}`
        );

        if (!result.success) {
          throw (result as { success: false; error: unknown }).error;
        }

        toast.success(`Base model "${firstModel.name || firstModel.id}" loaded`);
        return true;
      }

      logger.info('Loading available base model', {
        component: 'useModelLoader',
        modelId: availableModel.id,
        modelName: availableModel.name,
      });

      const result = await retryWithBackoff(
        () => apiClient.loadBaseModel(availableModel.id),
        DEFAULT_RETRY_CONFIG,
        undefined,
        `load base model ${availableModel.name || availableModel.id}`
      );

      if (!result.success) {
        throw (result as { success: false; error: unknown }).error;
      }

      toast.success(`Base model "${availableModel.name || availableModel.id}" loaded`);
      return true;
    } catch (err) {
      if (abortSignal?.aborted) {
        return false;
      }

      const error = toError(err);
      logger.error('Failed to load base model', {
        component: 'useModelLoader',
        stackId: stack.id,
      }, error);

      throw error;
    } finally {
      setIsLoadingBaseModel(false);
    }
  }, []);

  // Load adapters from stack
  const loadAdapters = useCallback(async (stack: AdapterStack, adapterIds?: string[], abortSignal?: AbortSignal): Promise<string[]> => {
    setIsLoadingAdapters(true);
    const failedAdapters: Array<{ adapterId: string; error: string }> = [];

    try {
      // Determine which adapters to load
      const targetAdapterIds = adapterIds || stack.adapter_ids || [];

      if (targetAdapterIds.length === 0) {
        logger.warn('No adapters to load in stack', {
          component: 'useModelLoader',
          stackId: stack.id,
        });
        return [];
      }

      logger.info('Loading adapters from stack', {
        component: 'useModelLoader',
        stackId: stack.id,
        adapterCount: targetAdapterIds.length,
        adapterIds: targetAdapterIds,
      });

      // Load each adapter sequentially (to avoid overwhelming the system)
      for (const adapterId of targetAdapterIds) {
        if (abortSignal?.aborted) {
          break;
        }

        try {
          logger.debug('Loading adapter', {
            component: 'useModelLoader',
            adapterId,
          });

          const result = await retryWithBackoff(
            () => apiClient.loadAdapter(adapterId),
            DEFAULT_RETRY_CONFIG,
            undefined,
            `load adapter ${adapterId}`
          );

          if (!result.success) {
            throw (result as { success: false; error: unknown }).error;
          }

          logger.info('Adapter loaded successfully', {
            component: 'useModelLoader',
            adapterId,
          });
        } catch (err) {
          if (abortSignal?.aborted) {
            break;
          }

          const error = toError(err);
          logger.error('Failed to load adapter', {
            component: 'useModelLoader',
            adapterId,
          }, error);

          failedAdapters.push({
            adapterId,
            error: error.message || 'Unknown error',
          });
        }
      }

      return failedAdapters.map((fa) => fa.adapterId);
    } finally {
      setIsLoadingAdapters(false);

      // Update failed adapter tracking
      if (failedAdapters.length > 0) {
        const failedIds = failedAdapters.map((fa) => fa.adapterId);
        setFailedAdapterIds(failedIds);

        const loadError: ModelLoaderError = {
          message: `Failed to load ${failedAdapters.length} adapter(s)`,
          code: 'ADAPTERS_LOAD_FAILED',
          failedAdapters,
        };
        setError(loadError);

        toast.error(`${failedAdapters.length} adapter(s) failed to load. Use retry to try again.`);
      }
    }
  }, []);

  // Load models (base model + adapters)
  const loadModels = useCallback(async (stackId: string): Promise<void> => {
    // Use loading coordinator to prevent concurrent operations on the same stack
    return loadingCoordinator.withLock(`stack:${stackId}`, async () => {
      clearError();

      // Create abort controller for this operation
      abortControllerRef.current = new AbortController();
      const abortSignal = abortControllerRef.current.signal;

      try {
        // Fetch stack details
        logger.info('Fetching adapter stack', {
          component: 'useModelLoader',
          stackId,
        });

        const stack = await apiClient.getAdapterStack(stackId);
        currentStackRef.current = stack;

        if (abortSignal.aborted) {
          return;
        }

        logger.info('Loaded stack details', {
          component: 'useModelLoader',
          stackId: stack.id,
          stackName: stack.name,
          adapterCount: stack.adapter_ids?.length || 0,
        });

        // Step 1: Load base model (if needed)
        const baseModelLoaded = await loadBaseModel(stack, abortSignal);

        if (!baseModelLoaded || abortSignal.aborted) {
          return;
        }

        // Step 2: Load adapters
        const failedIds = await loadAdapters(stack, undefined, abortSignal);

        if (abortSignal.aborted) {
          return;
        }

        // Success if no adapters failed
        if (failedIds.length === 0) {
          toast.success('All models loaded successfully');
          logger.info('All models loaded successfully', {
            component: 'useModelLoader',
            stackId: stack.id,
          });
        }
      } catch (err) {
        // Check if operation was cancelled
        if (abortSignal.aborted) {
          return;
        }

        const error = toError(err);
        logger.error('Failed to load models', {
          component: 'useModelLoader',
          stackId,
        }, error);

        // Check error type
        let errorCode: ModelLoaderError['code'] = 'UNKNOWN';
        if (error.message.includes('not found') || error.message.includes('404')) {
          errorCode = 'STACK_NOT_FOUND';
        } else if (error.message.includes('base model') || error.message.includes('model')) {
          errorCode = 'BASE_MODEL_LOAD_FAILED';
        }

        const modelError: ModelLoaderError = {
          message: error.message || 'Failed to load models',
          code: errorCode,
          details: {
            stackId,
          },
        };
        setError(modelError);

        toast.error(`Failed to load models: ${error.message}`);
      } finally {
        abortControllerRef.current = null;
      }
    });
  }, [clearError, loadBaseModel, loadAdapters]);

  // Retry failed adapters
  const retryFailed = useCallback(async (adapterIds?: string[]): Promise<void> => {
    const stack = currentStackRef.current;
    if (!stack) {
      logger.warn('Cannot retry, no stack loaded', {
        component: 'useModelLoader',
      });
      toast.error('No stack loaded. Please load models first.');
      return;
    }

    // Use provided adapter IDs or fall back to tracked failed adapters
    const targetAdapterIds = adapterIds || failedAdapterIds;

    if (targetAdapterIds.length === 0) {
      logger.warn('No failed adapters to retry', {
        component: 'useModelLoader',
      });
      toast.info('No failed adapters to retry');
      return;
    }

    // Use loading coordinator to prevent concurrent retry operations
    return loadingCoordinator.withLock(`retry:${stack.id}`, async () => {
      clearError();

      // Create abort controller for this operation
      abortControllerRef.current = new AbortController();
      const abortSignal = abortControllerRef.current.signal;

      try {
        logger.info('Retrying failed adapters', {
          component: 'useModelLoader',
          stackId: stack.id,
          adapterIds: targetAdapterIds,
        });

        toast.info(`Retrying ${targetAdapterIds.length} adapter(s)...`);

        const newFailedIds = await loadAdapters(stack, targetAdapterIds, abortSignal);

        if (abortSignal.aborted) {
          return;
        }

        if (newFailedIds.length === 0) {
          toast.success('All adapters loaded successfully');
          logger.info('Retry successful, all adapters loaded', {
            component: 'useModelLoader',
            stackId: stack.id,
          });
        } else {
          toast.warning(`${newFailedIds.length} adapter(s) still failed to load`);
        }
      } catch (err) {
        // Check if operation was cancelled
        if (abortSignal.aborted) {
          return;
        }

        const error = toError(err);
        logger.error('Retry failed', {
          component: 'useModelLoader',
          stackId: stack.id,
        }, error);

        toast.error(`Retry failed: ${error.message}`);
      } finally {
        abortControllerRef.current = null;
      }
    });
  }, [failedAdapterIds, clearError, loadAdapters]);

  return {
    // State
    isLoadingBaseModel,
    isLoadingAdapters,
    isLoading: isLoadingBaseModel || isLoadingAdapters,
    error,
    failedAdapterIds,

    // Actions
    loadModels,
    cancelLoading,
    retryFailed,
    clearError,
  };
}
