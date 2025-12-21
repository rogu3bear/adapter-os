/**
 * Model Loading Hooks - Type Definitions
 *
 * Strongly-typed interfaces for the model loading subsystem hooks.
 * These types support:
 * - Adapter state tracking and loading management
 * - Base model lifecycle monitoring
 * - Chat session adapter readiness checks
 * - Error handling with retry logic
 *
 * Copyright JKCA | 2025 James KC Auchterlonie
 */

import type { BaseModelStatus } from '@/api/api-types';
import type { AdapterStateTransitionEvent } from '@/api/streaming-types';

// ============================================================================
// Core Types
// ============================================================================

/**
 * Adapter lifecycle state
 * Matches backend lifecycle states from adapteros-lora-lifecycle
 */
export type AdapterLifecycleState = AdapterStateTransitionEvent['current_state'];

/**
 * Base model loading status
 * Matches useModelStatus states
 */
export type ModelStatusState = BaseModelStatus['status'];

// ============================================================================
// Error Types
// ============================================================================

/**
 * Error structure for chat loading failures
 * Includes retry information and user guidance
 */
export interface ChatLoadingError {
  /** Error message */
  message: string;

  /** Error code (e.g., 'ADAPTER_LOAD_FAILED', 'BASE_MODEL_NOT_READY') */
  code: string;

  /** Whether the operation can be retried */
  retryable: boolean;

  /** Number of retry attempts made so far */
  retryCount: number;

  /** Maximum retry attempts allowed */
  maxRetries: number;

  /** User-friendly suggestion for resolution */
  suggestion?: string;

  /** Original error details */
  details?: unknown;

  /** Timestamp when error occurred */
  timestamp: number;
}

// ============================================================================
// Adapter State Types
// ============================================================================

/**
 * Loading state for a single adapter
 * Tracks lifecycle, memory, and loading progress
 */
export interface AdapterLoadingItem {
  /** Unique adapter ID */
  adapterId: string;

  /** Display name */
  name: string;

  /** Current lifecycle state */
  state: AdapterLifecycleState;

  /** Loading operation in progress */
  isLoading: boolean;

  /** Load operation failed */
  hasError: boolean;

  /** Error message if load failed */
  errorMessage?: string;

  /** Memory usage in MB (if available) */
  memoryMb?: number;

  /** Timestamp of last state change */
  lastUpdated?: number;

  /** Whether this adapter is ready for inference */
  isReady: boolean;
}

// ============================================================================
// Hook Return Types
// ============================================================================

/**
 * Return type for useModelLoadingState hook
 *
 * Provides comprehensive state tracking for both base model and adapters
 * in chat sessions. Matches PRD specification for agent integration.
 *
 * @example
 * ```tsx
 * const {
 *   isReady,
 *   isLoading,
 *   progress,
 *   estimatedTimeRemaining,
 *   loadingAdapters,
 *   readyAdapters,
 *   failedAdapters,
 *   baseModel,
 *   refresh,
 * } = useModelLoadingState({ stackId: 'my-stack' });
 * ```
 */
export interface UseModelLoadingStateResult {
  // Primary properties
  /** Overall ready state - true when base model AND all adapters ready */
  isReady: boolean;

  /** Any loading operation in progress */
  isLoading: boolean;

  /** Array of adapters currently loading */
  loadingAdapters: AdapterLoadingItem[];

  /** Array of adapters that are ready (warm/hot/resident) */
  readyAdapters: AdapterLoadingItem[];

  /** Array of adapters that failed to load */
  failedAdapters: AdapterLoadingItem[];

  /** Overall loading progress (0-100) */
  progress: number;

  /** Estimated time remaining in seconds (null if not loading) */
  estimatedTimeRemaining: number | null;

  // Base model info (grouped)
  /** Base model state information */
  baseModel: {
    status: ModelStatusState;
    modelName: string | null;
    modelId: string | null;
    memoryUsageMb: number | null;
    errorMessage: string | null;
  };

  /** SSE connection active for real-time updates */
  sseConnected: boolean;

  /** Combined error from base model or adapters */
  error: ChatLoadingError | null;

  // Single refresh action (PRD spec)
  /** Refresh all state (base model and adapters) */
  refetch: () => Promise<void>;

  // Additional properties for backwards compatibility
  /** @deprecated Use isReady instead */
  overallReady: boolean;

  /** Base model is ready for inference */
  baseModelReady: boolean;

  /** All adapters are in ready states */
  allAdaptersReady: boolean;

  /** Map of adapter ID to loading state */
  adapterStates: Map<string, AdapterLoadingItem>;

  /** List of adapter IDs that are not ready */
  unreadyAdapters: string[];

  /** Number of adapters currently loading */
  loadingAdapterCount: number;

  /** Number of adapters with errors */
  errorAdapterCount: number;

  /** Base model loading status */
  baseModelStatus: ModelStatusState;

  /** Base model identifier */
  baseModelId: string | null;

  /** Base model display name */
  baseModelName: string | null;

  /** Base model memory usage in MB */
  baseModelMemoryMb: number | null;

  /** Base model error message */
  baseModelError: string | null;

  /** @deprecated Use sseConnected instead */
  isConnected: boolean;

  /** @deprecated Use refetch instead */
  refreshBaseModel: () => Promise<void>;

  /** @deprecated Use refetch instead */
  refreshAdapters: () => Promise<void>;

  /** @deprecated Use refetch instead */
  refreshAll: () => Promise<void>;
}

// ============================================================================
// Hook Configuration Options
// ============================================================================

/**
 * Configuration options for useModelLoadingState
 */
export interface UseModelLoadingStateOptions {
  /** Stack ID to monitor (required for adapter tracking) */
  stackId?: string;

  /** Tenant ID for base model (default: 'default') */
  tenantId?: string;

  /** Enable automatic state tracking (default: true) */
  enabled?: boolean;

  /** Polling interval for base model status in ms (default: 5000) */
  pollingInterval?: number;

  /** Callback when base model status changes */
  onBaseModelChange?: (status: ModelStatusState) => void;

  /** Callback when adapter state changes */
  onAdapterStateChange?: (adapterId: string, state: AdapterLifecycleState) => void;

  /** Callback when overall readiness changes */
  onReadinessChange?: (ready: boolean) => void;
}

// ============================================================================
// Utility Types
// ============================================================================

/**
 * Helper to check if an adapter is ready for inference
 */
export function isAdapterReady(state: AdapterLifecycleState): boolean {
  return state === 'warm' || state === 'hot' || state === 'resident';
}

/**
 * Helper to check if base model is ready
 */
export function isBaseModelReady(status: ModelStatusState): boolean {
  return status === 'ready';
}

/**
 * Helper to create a ChatLoadingError
 */
export function createChatLoadingError(
  message: string,
  code: string,
  options: {
    retryable?: boolean;
    retryCount?: number;
    maxRetries?: number;
    suggestion?: string;
    details?: unknown;
  } = {}
): ChatLoadingError {
  return {
    message,
    code,
    retryable: options.retryable ?? false,
    retryCount: options.retryCount ?? 0,
    maxRetries: options.maxRetries ?? 3,
    suggestion: options.suggestion,
    details: options.details,
    timestamp: Date.now(),
  };
}

/**
 * Helper to check if error is retryable
 */
export function isRetryableError(error: ChatLoadingError | null): boolean {
  if (!error) return false;
  return error.retryable && error.retryCount < error.maxRetries;
}

/**
 * Error codes for model loading failures
 */
export const ModelLoadingErrorCode = {
  BASE_MODEL_NOT_READY: 'BASE_MODEL_NOT_READY',
  BASE_MODEL_LOAD_FAILED: 'BASE_MODEL_LOAD_FAILED',
  ADAPTER_LOAD_FAILED: 'ADAPTER_LOAD_FAILED',
  ADAPTER_NOT_FOUND: 'ADAPTER_NOT_FOUND',
  STACK_NOT_FOUND: 'STACK_NOT_FOUND',
  MEMORY_INSUFFICIENT: 'MEMORY_INSUFFICIENT',
  TIMEOUT: 'TIMEOUT',
  NETWORK_ERROR: 'NETWORK_ERROR',
  UNKNOWN: 'UNKNOWN',
} as const;

export type ModelLoadingErrorCodeType = typeof ModelLoadingErrorCode[keyof typeof ModelLoadingErrorCode];
