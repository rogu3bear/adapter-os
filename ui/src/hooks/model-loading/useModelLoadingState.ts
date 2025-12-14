/**
 * useModelLoadingState - Unified model + adapter readiness tracking
 *
 * Primary composition hook for checking if chat inference is ready.
 * Combines base model status and adapter states into a single interface
 * with computed readiness, progress, and time estimates.
 *
 * @example
 * ```tsx
 * const {
 *   baseModelReady,
 *   allAdaptersReady,
 *   overallReady,
 *   isLoading,
 *   adapterStates,
 *   refreshAll,
 * } = useModelLoadingState({
 *   stackId: 'my-stack',
 *   tenantId: 'default',
 * });
 *
 * // Before sending chat message
 * if (!overallReady) {
 *   await refreshAll();
 * }
 * ```
 */

import { useMemo, useCallback, useRef, useEffect, useState } from 'react';
import { useChatAdapterState } from '@/hooks/chat/useChatAdapterState';
import { useModelStatus } from './useModelStatus';
import { useSSE } from '@/hooks/realtime/useSSE';
import type { BootProgressEvent } from '@/api/streaming-types';
import type {
  UseModelLoadingStateOptions,
  UseModelLoadingStateResult,
  AdapterLoadingItem,
  ChatLoadingError,
} from './types';
import { createChatLoadingError, ModelLoadingErrorCode } from './types';

// ============================================================================
// Constants
// ============================================================================

/** Model contributes 30% to overall progress */
const MODEL_WEIGHT = 0.3;

/** Adapters contribute 70% to overall progress */
const ADAPTER_WEIGHT = 0.7;

/** Estimated seconds to load a single adapter (conservative estimate) */
const SECONDS_PER_ADAPTER = 8;

/** Estimated seconds to load base model (conservative estimate) */
const SECONDS_FOR_MODEL = 30;

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Convert adapter readiness state to AdapterLoadingItem
 */
function toAdapterLoadingItem(
  adapterId: string,
  name: string,
  state: string,
  isLoading: boolean,
  error?: string
): AdapterLoadingItem {
  const lifecycleState = state as any; // Type assertion for lifecycle state
  const isReady = lifecycleState === 'warm' || lifecycleState === 'hot' || lifecycleState === 'resident';

  return {
    adapterId,
    name,
    state: lifecycleState,
    isLoading,
    hasError: !!error,
    errorMessage: error,
    isReady,
    lastUpdated: Date.now(),
  };
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * Unified model + adapter loading state
 *
 * This is the primary hook agents should use to check if chat is ready.
 * It composes useModelStatus and useChatAdapterState into a single interface
 * matching the expected UseModelLoadingStateResult interface.
 *
 * Features:
 * - Base model status tracking
 * - Adapter state monitoring via SSE
 * - Computed readiness flags
 * - Error aggregation
 * - Refresh controls
 */
export function useModelLoadingState(
  options: UseModelLoadingStateOptions = {}
): UseModelLoadingStateResult {
  const {
    stackId,
    tenantId = 'default',
    enabled = true,
    pollingInterval = 5000,
    onAdapterStateChange,
    onBaseModelChange,
    onReadinessChange,
  } = options;

  // Compose underlying hooks
  const modelStatus = useModelStatus(tenantId, pollingInterval);
  const adapterState = useChatAdapterState({
    stackId,
    enabled,
    onAdapterStateChange,
  });

  // Boot progress SSE subscription for real-time model loading progress
  const [modelLoadProgress, setModelLoadProgress] = useState(0);
  const baseModelReady = modelStatus.isReady;

  const { connected: bootSSEConnected } = useSSE<BootProgressEvent>(
    '/v1/stream/boot-progress',
    {
      enabled: enabled && !baseModelReady,
      onMessage: (event) => {
        if (event.event_type === 'LoadProgress') {
          setModelLoadProgress(event.progress_pct);
        } else if (event.event_type === 'ModelReady' || event.event_type === 'FullyReady') {
          setModelLoadProgress(100);
        }
      },
    }
  );

  // Convert adapter states to AdapterLoadingItem map
  const adapterStates = useMemo(() => {
    const converted = new Map<string, AdapterLoadingItem>();
    adapterState.adapterStates.forEach((adapter, id) => {
      converted.set(
        id,
        toAdapterLoadingItem(
          adapter.adapterId,
          adapter.name,
          adapter.state,
          adapter.isLoading,
          adapter.error
        )
      );
    });
    return converted;
  }, [adapterState.adapterStates]);

  // Compute base model readiness (baseModelReady defined above for SSE condition)
  const adaptersPresent = adapterStates.size > 0;
  const allAdaptersReady = adaptersPresent ? adapterState.allAdaptersReady : true;
  const overallReady = baseModelReady && allAdaptersReady;

  // Compute loading state
  const isModelLoading = modelStatus.status === 'loading';
  const isAdaptersLoading = adapterState.isCheckingAdapters;
  const hasAdaptersLoading = Array.from(adapterStates.values()).some(
    (adapter) => adapter.isLoading
  );
  const isLoading = isModelLoading || isAdaptersLoading || hasAdaptersLoading;

  // Compute adapter arrays
  const loadingAdapters = useMemo(
    () => Array.from(adapterStates.values()).filter((a) => a.isLoading),
    [adapterStates]
  );

  const readyAdapters = useMemo(
    () => Array.from(adapterStates.values()).filter((a) => a.isReady),
    [adapterStates]
  );

  const failedAdapters = useMemo(
    () => Array.from(adapterStates.values()).filter((a) => a.hasError),
    [adapterStates]
  );

  // Count adapter states
  const loadingAdapterCount = loadingAdapters.length;
  const errorAdapterCount = failedAdapters.length;

  // Progress calculation (PRD spec: 0-100)
  const progress = useMemo(() => {
    // Use real model load progress from SSE if available
    const modelProgress = baseModelReady
      ? 100
      : modelLoadProgress > 0
        ? modelLoadProgress
        : isModelLoading
          ? 50
          : 0;

    const totalAdapters = adapterStates.size;
    const readyCount = readyAdapters.length;
    const adapterProgress = totalAdapters === 0 ? 100 : (readyCount / totalAdapters) * 100;

    return Math.round(modelProgress * MODEL_WEIGHT + adapterProgress * ADAPTER_WEIGHT);
  }, [baseModelReady, modelLoadProgress, isModelLoading, adapterStates.size, readyAdapters.length]);

  // ETA calculation (PRD spec: seconds or null)
  const estimatedTimeRemaining = useMemo((): number | null => {
    if (!isLoading) return null;

    let eta = 0;
    if (!baseModelReady) {
      // Estimate remaining model load time based on progress
      const remainingPct = 100 - modelLoadProgress;
      eta += Math.ceil((remainingPct / 100) * SECONDS_FOR_MODEL);
    }

    const loadingCount = loadingAdapters.length +
      Array.from(adapterStates.values()).filter((a) => !a.isReady && !a.isLoading && !a.hasError).length;
    eta += loadingCount * SECONDS_PER_ADAPTER;

    return eta > 0 ? eta : null;
  }, [isLoading, baseModelReady, modelLoadProgress, loadingAdapters.length, adapterStates]);

  // Compute combined error
  const error = useMemo((): ChatLoadingError | null => {
    if (modelStatus.status === 'error') {
      return createChatLoadingError(
        modelStatus.errorMessage || 'Base model failed to load',
        ModelLoadingErrorCode.BASE_MODEL_LOAD_FAILED,
        {
          retryable: true,
          suggestion: 'Try refreshing or reloading the base model',
        }
      );
    }

    if (errorAdapterCount > 0) {
      const failedAdapter = Array.from(adapterStates.values()).find(
        (adapter) => adapter.hasError
      );
      return createChatLoadingError(
        failedAdapter?.errorMessage || 'Adapter failed to load',
        ModelLoadingErrorCode.ADAPTER_LOAD_FAILED,
        {
          retryable: true,
          suggestion: 'Try loading the adapter again',
          details: { adapterId: failedAdapter?.adapterId },
        }
      );
    }

    return null;
  }, [modelStatus.status, modelStatus.errorMessage, errorAdapterCount, adapterStates]);

  // Refresh actions
  const refreshBaseModel = useCallback(async () => {
    await modelStatus.refetch();
  }, [modelStatus]);

  const refreshAdapters = useCallback(async () => {
    // Reset model load progress to allow re-tracking on next load
    setModelLoadProgress(0);
    // Adapter states are updated via SSE automatically
    // Triggering a readiness check can help sync state
    adapterState.checkAdapterReadiness();
  }, [adapterState]);

  const refreshAll = useCallback(async () => {
    await Promise.all([refreshBaseModel(), refreshAdapters()]);
  }, [refreshBaseModel, refreshAdapters]);

  // Trigger callbacks when state changes
  const prevOverallReadyRef = useRef(overallReady);
  const prevModelStatusRef = useRef(modelStatus.status);

  useEffect(() => {
    if (onReadinessChange && prevOverallReadyRef.current !== overallReady) {
      onReadinessChange(overallReady);
      prevOverallReadyRef.current = overallReady;
    }
  }, [overallReady, onReadinessChange]);

  useEffect(() => {
    if (onBaseModelChange && prevModelStatusRef.current !== modelStatus.status) {
      onBaseModelChange(modelStatus.status);
      prevModelStatusRef.current = modelStatus.status;
    }
  }, [modelStatus.status, onBaseModelChange]);

  // Compute SSE connected status (either adapter SSE or boot SSE)
  const sseConnected = adapterState.sseConnected || bootSSEConnected;

  return {
    // Primary properties
    isReady: overallReady,
    isLoading,
    loadingAdapters,
    readyAdapters,
    failedAdapters,
    progress,
    estimatedTimeRemaining,

    // Base model info (grouped per PRD)
    baseModel: {
      status: modelStatus.status,
      modelName: modelStatus.modelName,
      modelId: modelStatus.modelId,
      memoryUsageMb: modelStatus.memoryUsageMb,
      errorMessage: modelStatus.errorMessage,
    },

    // SSE status
    sseConnected,

    // Error
    error,

    // Single refresh action (PRD spec)
    refetch: refreshAll,

    // Backwards compatibility properties (deprecated)
    overallReady,
    baseModelReady,
    allAdaptersReady,
    adapterStates,
    unreadyAdapters: adapterState.unreadyAdapters,
    loadingAdapterCount,
    errorAdapterCount,
    baseModelStatus: modelStatus.status,
    baseModelId: modelStatus.modelId,
    baseModelName: modelStatus.modelName,
    baseModelMemoryMb: modelStatus.memoryUsageMb,
    baseModelError: modelStatus.errorMessage,
    isConnected: sseConnected,
    refreshBaseModel,
    refreshAdapters,
    refreshAll,
  };
}

export default useModelLoadingState;
