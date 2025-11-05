//! Progress Operation Hook
//!
//! Manages progress tracking for long-running operations with real-time updates,
//! ETA calculations, and persistence across page refreshes.
//!
//! Citations:
//! - docs/Smashing Design Techniques.md L1-L50 - Trust-building UX patterns
//! - ui/src/hooks/usePolling.ts L1-L50 - Polling pattern reference

import { useState, useEffect, useCallback, useRef } from 'react';
import { usePolling } from './usePolling';
import apiClient from '../api/client';
import { logger } from '../utils/logger';

export interface ProgressState {
  progress: number; // 0-100
  status: string;
  eta?: string;
  confidence?: number;
  variant?: 'default' | 'success' | 'warning' | 'error';
  startTime?: number;
  estimatedDuration?: number;
}

export interface ProgressOperation {
  id: string;
  type: 'adapter_load' | 'adapter_unload' | 'training' | 'model_import' | 'file_upload' | 'inference';
  resourceId: string;
  tenantId: string;
  startTime: number;
  lastUpdate: number;
  state: ProgressState;
}

export interface UseProgressOperationReturn {
  operation: ProgressOperation | null;
  isActive: boolean;
  start: (type: ProgressOperation['type'], resourceId: string, tenantId: string) => string;
  update: (operationId: string, state: Partial<ProgressState>) => void;
  complete: (operationId: string, finalState?: Partial<ProgressState>) => void;
  cancel: (operationId: string) => void;
  getETA: (operationId: string) => string | undefined;
}

// Local storage key for persisting operations across refreshes
const OPERATIONS_STORAGE_KEY = 'adapteros_progress_operations';

// Calculate ETA based on historical data and current progress
function calculateETA(operation: ProgressOperation): string | undefined {
  const { startTime, state } = operation;
  const elapsed = Date.now() - startTime;

  if (state.progress <= 0 || state.progress >= 100) {
    return undefined;
  }

  // Use historical data for better ETA estimates
  const historicalData = getHistoricalOperationData(operation.type);
  if (historicalData.length > 0) {
    const avgDuration = historicalData.reduce((sum, op) => sum + op.duration, 0) / historicalData.length;
    const remainingProgress = (100 - state.progress) / 100;
    const estimatedRemaining = avgDuration * remainingProgress;
    const eta = new Date(Date.now() + estimatedRemaining);

    // Format as relative time
    const minutes = Math.round(estimatedRemaining / (1000 * 60));
    if (minutes < 1) return '< 1 min';
    if (minutes === 1) return '1 min';
    if (minutes < 60) return `${minutes} mins`;

    const hours = Math.floor(minutes / 60);
    const mins = minutes % 60;
    return `${hours}h ${mins}m`;
  }

  // Fallback: linear estimation based on current progress
  if (elapsed > 5000) { // Only if we've been running for at least 5 seconds
    const totalEstimated = (elapsed / state.progress) * 100;
    const remaining = totalEstimated - elapsed;
    const minutes = Math.round(remaining / (1000 * 60));
    if (minutes < 1) return '< 1 min';
    return `${minutes} min${minutes !== 1 ? 's' : ''}`;
  }

  return undefined;
}

// Get historical operation data for ETA calculations
function getHistoricalOperationData(type: ProgressOperation['type']): Array<{ duration: number }> {
  try {
    const stored = localStorage.getItem(`adapteros_operation_history_${type}`);
    if (!stored) return [];

    const history = JSON.parse(stored);
    return Array.isArray(history) ? history : [];
  } catch {
    return [];
  }
}

// Store operation data for historical analysis
function storeOperationData(type: ProgressOperation['type'], duration: number) {
  try {
    const history = getHistoricalOperationData(type);
    history.push({ duration, timestamp: Date.now() });

    // Keep only last 10 operations for each type
    if (history.length > 10) {
      history.shift();
    }

    localStorage.setItem(`adapteros_operation_history_${type}`, JSON.stringify(history));
  } catch (error) {
    logger.warn('Failed to store operation history', {
      component: 'useProgressOperation',
      operation: 'storeOperationData',
      type,
    }, error);
  }
}

// Generate unique operation ID
function generateOperationId(): string {
  return `op_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
}

// Load operations from localStorage
function loadOperations(): Record<string, ProgressOperation> {
  try {
    const stored = localStorage.getItem(OPERATIONS_STORAGE_KEY);
    if (!stored) return {};

    const operations = JSON.parse(stored);

    // Clean up stale operations (older than 1 hour)
    const cutoff = Date.now() - (60 * 60 * 1000);
    const cleaned: Record<string, ProgressOperation> = {};

    for (const [id, op] of Object.entries(operations)) {
      if (op.lastUpdate > cutoff) {
        cleaned[id] = op;
      }
    }

    return cleaned;
  } catch {
    return {};
  }
}

// Save operations to localStorage
function saveOperations(operations: Record<string, ProgressOperation>) {
  try {
    localStorage.setItem(OPERATIONS_STORAGE_KEY, JSON.stringify(operations));
  } catch (error) {
    logger.warn('Failed to save operations to localStorage', {
      component: 'useProgressOperation',
      operation: 'saveOperations',
    }, error);
  }
}

export function useProgressOperation(operationId?: string): UseProgressOperationReturn {
  const [operations, setOperations] = useState<Record<string, ProgressOperation>>(loadOperations);
  const [activeOperationId, setActiveOperationId] = useState<string | null>(operationId || null);
  const pollingRef = useRef<any>(null);

  const activeOperation = activeOperationId ? operations[activeOperationId] : null;

  // Poll for progress updates when operation is active
  usePolling(
    async () => {
      if (!activeOperation) return null;

      try {
        let progressData: any = null;

        switch (activeOperation.type) {
          case 'adapter_load':
          case 'adapter_unload':
            const adapter = await apiClient.getAdapter(activeOperation.resourceId);
            progressData = {
              progress: adapter.state === 'warm' ? 100 : adapter.state === 'loading' ? 50 : 0,
              status: adapter.state === 'warm' ? 'Loaded' : adapter.state === 'loading' ? 'Loading...' : 'Pending',
              variant: adapter.state === 'warm' ? 'success' : 'default'
            };
            break;

          case 'training':
            const job = await apiClient.getTrainingJob(activeOperation.resourceId);
            progressData = {
              progress: job.progress || 0,
              status: job.status === 'running' ? 'Training...' : job.status,
              variant: job.status === 'completed' ? 'success' : job.status === 'failed' ? 'error' : 'default'
            };
            break;

          case 'model_import':
            const importStatus = await apiClient.getModelImportStatus(activeOperation.resourceId);
            progressData = {
              progress: importStatus.progress || 0,
              status: importStatus.status === 'completed' ? 'Imported' : importStatus.status === 'failed' ? 'Failed' : 'Importing...',
              variant: importStatus.status === 'completed' ? 'success' : importStatus.status === 'failed' ? 'error' : 'default'
            };
            break;

          default:
            return null;
        }

        if (progressData) {
          update(activeOperation.id, progressData);

          // Check if operation completed
          if (progressData.progress >= 100 || progressData.variant === 'success' || progressData.variant === 'error') {
            complete(activeOperation.id, progressData);
          }
        }

        return progressData;
      } catch (error) {
        logger.warn('Failed to poll operation progress', {
          component: 'useProgressOperation',
          operation: 'polling',
          operationId: activeOperation?.id,
        }, error);
        return null;
      }
    },
    activeOperation ? 'fast' : 'off', // Fast polling when active
    {
      onError: (error) => {
        logger.error('Progress polling failed', {
          component: 'useProgressOperation',
          operation: 'polling',
          operationId: activeOperation?.id,
        }, error);
      }
    }
  );

  const start = useCallback((type: ProgressOperation['type'], resourceId: string, tenantId: string): string => {
    const operationId = generateOperationId();
    const operation: ProgressOperation = {
      id: operationId,
      type,
      resourceId,
      tenantId,
      startTime: Date.now(),
      lastUpdate: Date.now(),
      state: {
        progress: 0,
        status: 'Starting...',
        variant: 'default'
      }
    };

    setOperations(prev => {
      const updated = { ...prev, [operationId]: operation };
      saveOperations(updated);
      return updated;
    });

    setActiveOperationId(operationId);

    logger.info('Started progress operation', {
      component: 'useProgressOperation',
      operation: 'start',
      operationId,
      type,
      resourceId,
    });

    return operationId;
  }, []);

  const update = useCallback((operationId: string, state: Partial<ProgressState>) => {
    setOperations(prev => {
      const operation = prev[operationId];
      if (!operation) return prev;

      const updatedOperation = {
        ...operation,
        lastUpdate: Date.now(),
        state: {
          ...operation.state,
          ...state,
          eta: calculateETA({ ...operation, state: { ...operation.state, ...state } })
        }
      };

      const updated = { ...prev, [operationId]: updatedOperation };
      saveOperations(updated);
      return updated;
    });
  }, []);

  const complete = useCallback((operationId: string, finalState?: Partial<ProgressState>) => {
    setOperations(prev => {
      const operation = prev[operationId];
      if (!operation) return prev;

      const duration = Date.now() - operation.startTime;

      // Store for historical analysis
      storeOperationData(operation.type, duration);

      const completedOperation = {
        ...operation,
        lastUpdate: Date.now(),
        state: {
          ...operation.state,
          progress: 100,
          status: 'Completed',
          variant: 'success',
          ...finalState
        }
      };

      const updated = { ...prev, [operationId]: completedOperation };
      saveOperations(updated);
      return updated;
    });

    // Clear active operation after a delay
    setTimeout(() => {
      setActiveOperationId(null);
    }, 3000);

    logger.info('Completed progress operation', {
      component: 'useProgressOperation',
      operation: 'complete',
      operationId,
    });
  }, []);

  const cancel = useCallback((operationId: string) => {
    setOperations(prev => {
      const operation = prev[operationId];
      if (!operation) return prev;

      const cancelledOperation = {
        ...operation,
        lastUpdate: Date.now(),
        state: {
          ...operation.state,
          status: 'Cancelled',
          variant: 'warning'
        }
      };

      const updated = { ...prev, [operationId]: cancelledOperation };
      saveOperations(updated);
      return updated;
    });

    setActiveOperationId(null);

    logger.info('Cancelled progress operation', {
      component: 'useProgressOperation',
      operation: 'cancel',
      operationId,
    });
  }, []);

  const getETA = useCallback((operationId: string): string | undefined => {
    const operation = operations[operationId];
    return operation ? calculateETA(operation) : undefined;
  }, [operations]);

  // Cleanup on unmount
  useEffect(() => {
    const cancelPolling = pollingRef.current;
    return () => {
      cancelPolling?.();
    };
  }, []);

  return {
    operation: activeOperation,
    isActive: activeOperation !== null,
    start,
    update,
    complete,
    cancel,
    getETA
  };
}
