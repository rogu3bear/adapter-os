/**
 * useModelStatus - Hook for tracking base model loading status
 *
 * Provides real-time model status for the global status indicator.
 * Shows whether a model is loaded, loading, or no model is configured.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import apiClient from '@/api/client';
import { BaseModelStatus } from '@/api/api-types';
import { logger } from '@/utils/logger';

export type ModelStatusState = 
  | 'no-model'      // No model configured/imported
  | 'loading'       // Model is loading into memory
  | 'ready'         // Model is loaded and ready
  | 'unloading'     // Model is being unloaded
  | 'error'         // Model failed to load
  | 'checking';     // Initial status check in progress

export interface UseModelStatusReturn {
  status: ModelStatusState;
  modelName: string | null;
  modelId: string | null;
  modelPath: string | null;
  memoryUsageMb: number | null;
  errorMessage: string | null;
  isReady: boolean;
  refresh: () => Promise<void>;
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
  const isMountedRef = useRef(true);
  const intervalRef = useRef<NodeJS.Timeout | null>(null);

  const fetchStatus = useCallback(async () => {
    if (!isMountedRef.current) return;

    try {
      const response = await apiClient.getBaseModelStatus(tenantId);
      
      if (!isMountedRef.current) return;

      if (!response || !response.model_id || response.model_id === 'none') {
        setStatus('no-model');
        setModelName(null);
        setModelId(null);
        setModelPath(null);
        setMemoryUsageMb(null);
        setErrorMessage(null);
        return;
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

    } catch (err) {
      if (!isMountedRef.current) return;

      // Check if this is a 401 Unauthorized error (user not logged in)
      const is401 = err instanceof Error &&
        (err.message.includes('401') ||
         err.message.includes('Unauthorized') ||
         err.message.includes('authentication'));

      // API error likely means no model status available
      setStatus('no-model');
      setModelName(null);
      setModelId(null);
      setModelPath(null);
      setMemoryUsageMb(null);
      setErrorMessage(null);

      // Only log at debug level - this is expected when no model is configured or not authenticated
      // Don't log 401 errors at all - they're expected before login
      if (!is401) {
        logger.debug('Model status check failed (no model configured)', {
          component: 'useModelStatus',
          operation: 'fetchStatus',
          tenantId,
        });
      }
    }
  }, [tenantId]);

  // Initial fetch and polling setup
  useEffect(() => {
    isMountedRef.current = true;
    
    // Initial fetch
    fetchStatus();

    // Set up polling
    intervalRef.current = setInterval(fetchStatus, pollingInterval);

    return () => {
      isMountedRef.current = false;
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [fetchStatus, pollingInterval]);

  return {
    status,
    modelName,
    modelId,
    modelPath,
    memoryUsageMb,
    errorMessage,
    isReady: status === 'ready',
    refresh: fetchStatus,
  };
}

export default useModelStatus;

