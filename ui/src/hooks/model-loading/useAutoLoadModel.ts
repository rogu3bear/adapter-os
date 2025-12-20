/**
 * useAutoLoadModel - Hook for automatically loading a model when none is present
 *
 * Used by the operator dashboard to ensure a model is loaded when operators log in.
 * Respects user preference stored in localStorage.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { useModelStatus } from './useModelStatus';
import { apiClient } from '@/api/services';
import { logger } from '@/utils/logger';
import { toast } from 'sonner';

const AUTO_LOAD_KEY = 'aos-operator-auto-load';
const MAX_RETRIES = 3;
const RETRY_DELAYS = [1000, 2000, 4000]; // Exponential backoff
const LOAD_TIMEOUT_MS = 60000; // 60 second timeout for model loading

export interface AutoLoadError {
  message: string;
  code: 'NO_MODELS' | 'LOAD_FAILED' | 'NETWORK_ERROR' | 'TIMEOUT' | 'OUT_OF_MEMORY' | 'ALREADY_LOADING' | 'UNKNOWN';
  retryCount: number;
  canRetry: boolean;
}

// Safe localStorage access (handles private browsing, disabled storage)
function getStoredPreference(): boolean {
  try {
    const stored = localStorage.getItem(AUTO_LOAD_KEY);
    return stored === null ? true : stored === 'true';
  } catch {
    // localStorage not available (private browsing, etc.)
    return true;
  }
}

function setStoredPreference(value: boolean): void {
  try {
    localStorage.setItem(AUTO_LOAD_KEY, String(value));
  } catch {
    // Silently fail if localStorage not available
  }
}

export interface UseAutoLoadModelReturn {
  /** True while auto-load is in progress */
  isAutoLoading: boolean;
  /** Error details if auto-load failed */
  error: AutoLoadError | null;
  /** Whether there's an error */
  isError: boolean;
  /** Whether auto-load is enabled (user preference) */
  autoLoadEnabled: boolean;
  /** Disable auto-load for future sessions */
  disableAutoLoad: () => void;
  /** Enable auto-load for future sessions */
  enableAutoLoad: () => void;
  /** Toggle auto-load preference */
  toggleAutoLoad: () => void;
  /** Manually trigger model load */
  loadModel: () => Promise<void>;
  /** Retry after error */
  retry: () => Promise<void>;
  /** Clear error state */
  clearError: () => void;
}

/**
 * Hook for automatically loading a model when none is present
 *
 * @param tenantId - Current tenant ID
 * @param enabled - Whether to attempt auto-load (default: true)
 */
// Helper to classify error types
function classifyError(err: unknown): { code: AutoLoadError['code']; message: string; canRetry: boolean } {
  if (err instanceof Error) {
    const message = err.message.toLowerCase();

    // Network errors - retryable
    if (message.includes('network') || message.includes('fetch') || message.includes('connection') || message.includes('econnrefused')) {
      return { code: 'NETWORK_ERROR', message: err.message, canRetry: true };
    }

    // Timeout - retryable
    if (message.includes('timeout') || message.includes('timed out')) {
      return { code: 'TIMEOUT', message: 'Model loading timed out. The model may be too large or the server is busy.', canRetry: true };
    }

    // Out of memory - NOT retryable (need to free resources first)
    if (message.includes('memory') || message.includes('oom') || message.includes('out of memory') || message.includes('insufficient')) {
      return { code: 'OUT_OF_MEMORY', message: 'Not enough memory to load the model. Try unloading other models or closing applications.', canRetry: false };
    }

    // Already loading - NOT retryable (wait for current load)
    if (message.includes('already loading') || message.includes('in progress')) {
      return { code: 'ALREADY_LOADING', message: 'A model is already being loaded. Please wait.', canRetry: false };
    }

    // No models - NOT retryable (need admin action)
    if (message.includes('not found') || message.includes('no model') || message.includes('does not exist')) {
      return { code: 'NO_MODELS', message: err.message, canRetry: false };
    }

    // Generic load failure - retryable
    return { code: 'LOAD_FAILED', message: err.message, canRetry: true };
  }
  return { code: 'UNKNOWN', message: 'An unknown error occurred', canRetry: true };
}

// Promise wrapper with timeout
function withTimeout<T>(promise: Promise<T>, ms: number, message: string): Promise<T> {
  let timeoutId: NodeJS.Timeout;
  const timeoutPromise = new Promise<never>((_, reject) => {
    timeoutId = setTimeout(() => reject(new Error(message)), ms);
  });

  return Promise.race([promise, timeoutPromise]).finally(() => clearTimeout(timeoutId));
}

export function useAutoLoadModel(
  tenantId: string = 'default',
  enabled: boolean = true
): UseAutoLoadModelReturn {
  const { status, refetch: refresh } = useModelStatus(tenantId);
  const [isAutoLoading, setIsAutoLoading] = useState(false);
  const [error, setError] = useState<AutoLoadError | null>(null);
  const [autoLoadEnabled, setAutoLoadEnabled] = useState(getStoredPreference);

  const hasAttemptedRef = useRef(false);
  const isMountedRef = useRef(true);
  const retryCountRef = useRef(0);
  const loadingAbortRef = useRef<AbortController | null>(null);

  // Clear error state
  const clearError = useCallback(() => {
    setError(null);
    retryCountRef.current = 0;
  }, []);

  // Persist preference to localStorage (with safe fallback)
  const updatePreference = useCallback((value: boolean) => {
    setAutoLoadEnabled(value);
    setStoredPreference(value);
  }, []);

  const disableAutoLoad = useCallback(() => updatePreference(false), [updatePreference]);
  const enableAutoLoad = useCallback(() => updatePreference(true), [updatePreference]);
  const toggleAutoLoad = useCallback(() => updatePreference(!autoLoadEnabled), [autoLoadEnabled, updatePreference]);

  // Set error with proper structure
  const setAutoLoadError = useCallback((code: AutoLoadError['code'], message: string, errorCanRetry: boolean = true) => {
    const canRetry = errorCanRetry && retryCountRef.current < MAX_RETRIES;
    setError({
      code,
      message,
      retryCount: retryCountRef.current,
      canRetry,
    });
  }, []);

  // Load the first available model
  const loadModel = useCallback(async () => {
    // Prevent concurrent loads
    if (isAutoLoading) {
      logger.debug('Load already in progress, skipping', { component: 'useAutoLoadModel' });
      return;
    }

    // Cancel any pending load
    if (loadingAbortRef.current) {
      loadingAbortRef.current.abort();
    }
    loadingAbortRef.current = new AbortController();

    setIsAutoLoading(true);
    setError(null);

    try {
      // Fetch available models with timeout
      const models = await withTimeout(
        apiClient.listModels(),
        10000,
        'Timed out fetching model list'
      );

      if (!isMountedRef.current) return;

      // Validate response format
      if (!Array.isArray(models)) {
        throw new Error('Invalid response from server: expected model list');
      }

      if (models.length === 0) {
        setAutoLoadError('NO_MODELS', 'No models available. Please import a model first.', false);
        logger.warn('No models available for auto-load', {
          component: 'useAutoLoadModel',
          tenantId,
        });
        return;
      }

      // Check if a model is currently loading (another session/tab may be loading)
      const loadingModel = models.find((m) => m.import_status === 'loading');
      if (loadingModel) {
        setAutoLoadError('ALREADY_LOADING', `Model "${loadingModel.name || loadingModel.id}" is already loading. Please wait.`, false);
        logger.info('Model already loading, skipping auto-load', {
          component: 'useAutoLoadModel',
          modelId: loadingModel.id,
        });
        return;
      }

      // Find the first model that's available (not already loaded/loading)
      const availableModel = models.find(
        (m) => m.import_status === 'available' || m.import_status === 'ready'
      );

      if (!availableModel) {
        // If no available models, check if one is already loaded
        const loadedModel = models.find((m) => m.import_status === 'loaded');
        if (loadedModel) {
          logger.info('Model already loaded, skipping auto-load', {
            component: 'useAutoLoadModel',
            modelId: loadedModel.id,
          });
          clearError();
          return;
        }

        // Try to load the first model regardless of status
        const firstModel = models[0];
        if (firstModel) {
          logger.info('Auto-loading first model', {
            component: 'useAutoLoadModel',
            modelId: firstModel.id,
            modelName: firstModel.name,
          });

          await withTimeout(
            apiClient.loadBaseModel(firstModel.id),
            LOAD_TIMEOUT_MS,
            'Model loading timed out'
          );

          if (isMountedRef.current) {
            toast.success(`Model "${firstModel.name || firstModel.id}" loaded`);
            clearError();
            await refresh();
          }
        }
        return;
      }

      logger.info('Auto-loading available model', {
        component: 'useAutoLoadModel',
        modelId: availableModel.id,
        modelName: availableModel.name,
      });

      await withTimeout(
        apiClient.loadBaseModel(availableModel.id),
        LOAD_TIMEOUT_MS,
        'Model loading timed out'
      );

      if (isMountedRef.current) {
        toast.success(`Model "${availableModel.name || availableModel.id}" loaded`);
        clearError();
        await refresh();
      }
    } catch (err) {
      if (!isMountedRef.current) return;

      // Ignore abort errors (user cancelled or component unmounted)
      if (err instanceof Error && err.name === 'AbortError') {
        return;
      }

      const { code, message, canRetry } = classifyError(err);
      setAutoLoadError(code, message, canRetry);

      logger.error('Auto-load model failed', {
        component: 'useAutoLoadModel',
        tenantId,
        errorCode: code,
        error: message,
        retryCount: retryCountRef.current,
      });

      toast.error(`Failed to load model: ${message}`);
    } finally {
      if (isMountedRef.current) {
        setIsAutoLoading(false);
      }
    }
  }, [isAutoLoading, tenantId, refresh, setAutoLoadError, clearError]);

  // Retry with exponential backoff
  const retry = useCallback(async () => {
    if (!error?.canRetry || isAutoLoading) return;

    retryCountRef.current += 1;
    const delay = RETRY_DELAYS[Math.min(retryCountRef.current - 1, RETRY_DELAYS.length - 1)];

    logger.info('Retrying model auto-load', {
      component: 'useAutoLoadModel',
      retryCount: retryCountRef.current,
      delay,
    });

    toast.info(`Retrying in ${delay / 1000}s... (attempt ${retryCountRef.current}/${MAX_RETRIES})`);

    await new Promise((resolve) => setTimeout(resolve, delay));

    if (isMountedRef.current) {
      await loadModel();
    }
  }, [error?.canRetry, isAutoLoading, loadModel]);

  // Auto-load on mount if enabled and no model loaded
  useEffect(() => {
    isMountedRef.current = true;

    // Only attempt auto-load once per mount
    if (
      enabled &&
      autoLoadEnabled &&
      status === 'no-model' &&
      !hasAttemptedRef.current &&
      !isAutoLoading
    ) {
      hasAttemptedRef.current = true;
      loadModel();
    }

    return () => {
      isMountedRef.current = false;
      // Cancel any pending load on unmount
      if (loadingAbortRef.current) {
        loadingAbortRef.current.abort();
        loadingAbortRef.current = null;
      }
    };
  }, [enabled, autoLoadEnabled, status, isAutoLoading, loadModel]);

  // Reset attempt flag when status changes back to no-model (e.g., after unload)
  useEffect(() => {
    if (status !== 'no-model') {
      hasAttemptedRef.current = false;
    }
  }, [status]);

  return {
    isAutoLoading,
    error,
    isError: error !== null,
    autoLoadEnabled,
    disableAutoLoad,
    enableAutoLoad,
    toggleAutoLoad,
    loadModel,
    retry,
    clearError,
  };
}

export default useAutoLoadModel;
