/**
 * useModelStatus - Hook for tracking base model loading status
 *
 * Provides real-time model status for the global status indicator.
 * Shows whether a model is loaded, loading, or no model is configured.
 */

// @ts-nocheck
import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { apiClient } from '@/api/services';
import { logger } from '@/utils/logger';
import type { ModelStatusState as ModelStatusStateType } from './types';
import { useDemoMode } from '@/hooks/demo/DemoProvider';

export const MODEL_STATUS_EVENT = 'aos:model-status:refresh';

export interface ModelStatusEventDetail {
  tenantId?: string;
  status?: ModelStatusState;
  modelName?: string | null;
  modelId?: string | null;
  modelPath?: string | null;
  memoryUsageMb?: number | null;
  errorMessage?: string | null;
}

export type ModelStatusState = ModelStatusStateType;

export interface UseModelStatusReturn {
  status: ModelStatusState;
  modelName: string | null;
  modelId: string | null;
  modelPath: string | null;
  memoryUsageMb: number | null;
  errorMessage: string | null;
  isReady: boolean;
  /** Timestamp of last successful status poll (null if never polled) */
  lastPolledAt: number | null;
  /** True when status is based on failed API fetch (not actual no-model state) */
  isFetchError: boolean;
  /** Error from last fetch attempt (null if fetch succeeded) */
  fetchError: Error | null;
  refetch: () => Promise<void>;
}

/**
 * Hook for monitoring base model status
 * 
 * @param tenantId - Current tenant ID
 * @param pollingInterval - Polling interval in ms (default: 5000)
 */
export function useModelStatus(
  tenantId: string = 'default',
  pollingInterval: number = 5000
): UseModelStatusReturn {
  const [status, setStatus] = useState<ModelStatusState>('checking');
  const [modelName, setModelName] = useState<string | null>(null);
  const [modelId, setModelId] = useState<string | null>(null);
  const [modelPath, setModelPath] = useState<string | null>(null);
  const [memoryUsageMb, setMemoryUsageMb] = useState<number | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [lastPolledAt, setLastPolledAt] = useState<number | null>(null);
  const [fetchError, setFetchError] = useState<Error | null>(null);
  const isMountedRef = useRef(true);
  const intervalRef = useRef<NodeJS.Timeout | null>(null);
  const { enabled: demoMode, activeModel } = useDemoMode();

  // Exponential backoff state for error handling
  const errorCountRef = useRef(0);
  const currentIntervalRef = useRef(pollingInterval);
  const MAX_BACKOFF_MULTIPLIER = 4; // Max 4x the base interval (e.g., 5s -> 20s max)

  const fetchStatus = useCallback(async (): Promise<boolean> => {
    if (!isMountedRef.current) return false;

    try {
      const response = await apiClient.getBaseModelStatus(tenantId);

      if (!isMountedRef.current) return false;

      // Clear fetch error on successful API call
      setFetchError(null);

      // Reset error count on success
      errorCountRef.current = 0;
      currentIntervalRef.current = pollingInterval;

      if (!response || !response.model_id || response.model_id === 'none') {
        setStatus('no-model');
        setModelName(null);
        setModelId(null);
        setModelPath(null);
        setMemoryUsageMb(null);
        setErrorMessage(null);
        return true;
      }

      // Map backend status to our state
      switch (response.status) {
        case 'ready':
          setStatus('ready');
          break;
        case 'loading':
          setStatus('loading');
          break;
        case 'unloading':
          setStatus('unloading');
          break;
        case 'error':
          setStatus('error');
          break;
        case 'no-model':
          setStatus('no-model');
          break;
        case 'checking':
          setStatus('checking');
          break;
        default:
          setStatus('no-model');
      }

      setModelName(response.model_name || null);
      setModelId(response.model_id || null);
      setModelPath(response.model_path || null);
      setMemoryUsageMb(response.memory_usage_mb || null);
      setErrorMessage(response.error_message || null);

      // Mark successful poll completion for state source coordination
      setLastPolledAt(Date.now());
      return true;

    } catch (err) {
      if (!isMountedRef.current) return false;

      const error = err instanceof Error ? err : new Error(String(err));

      // Check if this is a 401 Unauthorized error (user not logged in)
      const is401 = error.message.includes('401') ||
        error.message.includes('Unauthorized') ||
        error.message.includes('authentication');

      // Track fetch error so components can distinguish API failure from actual no-model
      setFetchError(error);

      // Increment error count for backoff (cap at 3 to limit max backoff)
      errorCountRef.current = Math.min(errorCountRef.current + 1, 3);
      currentIntervalRef.current = Math.min(
        pollingInterval * Math.pow(2, errorCountRef.current),
        pollingInterval * MAX_BACKOFF_MULTIPLIER
      );

      // API error - keep status as 'checking' to indicate unknown state
      // Only set 'no-model' if we're certain there's no model
      setStatus('checking');
      setModelName(null);
      setModelId(null);
      setModelPath(null);
      setMemoryUsageMb(null);
      setErrorMessage(is401 ? 'Authentication required' : 'Unable to fetch model status');

      // Only log at debug level - this is expected when no model is configured or not authenticated
      // Don't log 401 errors at all - they're expected before login
      if (!is401) {
        logger.debug('Model status check failed (no model configured)', {
          component: 'useModelStatus',
          operation: 'fetchStatus',
          tenantId,
          errorCount: errorCountRef.current,
          nextIntervalMs: currentIntervalRef.current,
        });
      }
      return false;
    }
  }, [tenantId, pollingInterval]);

  // Initial fetch and polling setup with dynamic backoff
  useEffect(() => {
    isMountedRef.current = true;
    let timeoutId: NodeJS.Timeout | null = null;

    const scheduleNextPoll = () => {
      if (!isMountedRef.current) return;

      timeoutId = setTimeout(async () => {
        await fetchStatus();
        // Schedule next poll with current interval (may have changed due to errors)
        scheduleNextPoll();
      }, currentIntervalRef.current);
    };

    // Initial fetch
    fetchStatus().then(() => {
      // Start polling after initial fetch
      scheduleNextPoll();
    });

    return () => {
      isMountedRef.current = false;
      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [fetchStatus, pollingInterval]);

  const demoOverride = useMemo(() => {
    if (!demoMode || !activeModel) return null;
    return {
      status: 'ready' as ModelStatusState,
      modelName: activeModel.name,
      modelId: activeModel.id,
      modelPath: activeModel.backend ?? activeModel.format ?? null,
      memoryUsageMb: activeModel.memoryUsageMb ?? memoryUsageMb,
      errorMessage: null,
    };
  }, [activeModel, demoMode, memoryUsageMb]);

  // Allow external triggers (e.g., model management UI) to push status updates immediately
  useEffect(() => {
    const handleModelStatusEvent = (event: Event) => {
      const detail = (event as CustomEvent<ModelStatusEventDetail>).detail || {};
      if (detail.tenantId && detail.tenantId !== tenantId) {
        return;
      }

      if (detail.status) setStatus(detail.status);
      if ('modelName' in detail) setModelName(detail.modelName ?? null);
      if ('modelId' in detail) setModelId(detail.modelId ?? null);
      if ('modelPath' in detail) setModelPath(detail.modelPath ?? null);
      if ('memoryUsageMb' in detail) setMemoryUsageMb(detail.memoryUsageMb ?? null);
      if ('errorMessage' in detail) setErrorMessage(detail.errorMessage ?? null);

      void fetchStatus();
    };

    window.addEventListener(MODEL_STATUS_EVENT, handleModelStatusEvent as EventListener);
    return () => window.removeEventListener(MODEL_STATUS_EVENT, handleModelStatusEvent as EventListener);
  }, [fetchStatus, tenantId]);

  const effectiveStatus = demoOverride?.status ?? status;
  const effectiveModelName = demoOverride?.modelName ?? modelName;
  const effectiveModelId = demoOverride?.modelId ?? modelId;
  const effectiveModelPath = demoOverride?.modelPath ?? modelPath;
  const effectiveMemoryUsage = demoOverride?.memoryUsageMb ?? memoryUsageMb;
  const effectiveError = demoOverride?.errorMessage ?? errorMessage;

  return {
    status: effectiveStatus,
    modelName: effectiveModelName,
    modelId: effectiveModelId,
    modelPath: effectiveModelPath,
    memoryUsageMb: effectiveMemoryUsage,
    errorMessage: effectiveError,
    isReady: effectiveStatus === 'ready',
    lastPolledAt,
    isFetchError: fetchError !== null,
    fetchError,
    refetch: fetchStatus,
  };
}

export default useModelStatus;
