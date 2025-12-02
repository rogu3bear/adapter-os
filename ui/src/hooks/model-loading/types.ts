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

// ============================================================================
// Core Types
// ============================================================================

/**
 * Adapter lifecycle state
 * Matches backend lifecycle states from adapteros-lora-lifecycle
 */
export type AdapterLifecycleState = 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';

/**
 * Base model loading status
 * Matches useModelStatus states
 */
export type ModelStatusState =
  | 'no-model'      // No model configured/imported
  | 'loading'       // Model is loading into memory
  | 'loaded'        // Model is loaded and ready
  | 'unloading'     // Model is being unloaded
  | 'error'         // Model failed to load
  | 'checking';     // Initial status check in progress

/**
 * Loading operation state
 */
export type LoadingState = 'idle' | 'loading' | 'loaded' | 'error';

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

/**
 * Full adapter state information
 * Extends AdapterLoadingItem with additional metadata
 */
export interface AdapterStateInfo extends AdapterLoadingItem {
  /** Tenant ID owning this adapter */
  tenantId?: string;

  /** Tier classification */
  tier?: 'ephemeral' | 'warm' | 'persistent';

  /** LoRA rank parameter */
  rank?: number;

  /** LoRA alpha parameter */
  alpha?: number;

  /** Adapter version */
  version?: string;

  /** Tags for categorization */
  tags?: string[];

  /** Whether adapter is pinned in memory */
  isPinned?: boolean;

  /** Activation percentage in recent inferences */
  activationPercentage?: number;

  /** Base model ID this adapter is built for */
  baseModelId?: string;
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
  // PRD-specified primary properties
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
  refresh: () => Promise<void>;

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

  /** @deprecated Use refresh instead */
  refreshBaseModel: () => Promise<void>;

  /** @deprecated Use refresh instead */
  refreshAdapters: () => Promise<void>;

  /** @deprecated Use refresh instead */
  refreshAll: () => Promise<void>;
}

/**
 * Return type for useModelLoader hook
 *
 * Provides imperative loading controls with progress tracking
 * and error recovery.
 *
 * @example
 * ```tsx
 * const {
 *   loadAll,
 *   loadAdapters,
 *   loadBaseModel,
 *   isLoading,
 *   progress,
 *   error,
 *   reset,
 * } = useModelLoader({ stackId: 'my-stack' });
 *
 * // Load everything before chat
 * await loadAll();
 * ```
 */
export interface UseModelLoaderResult {
  // Loading Actions
  /** Load both base model and all adapters */
  loadAll: () => Promise<void>;

  /** Load only adapters (assumes base model ready) */
  loadAdapters: () => Promise<void>;

  /** Load only base model */
  loadBaseModel: () => Promise<void>;

  /** Retry failed loads */
  retry: () => Promise<void>;

  /** Reset error state */
  reset: () => void;

  // State
  /** Any loading operation in progress */
  isLoading: boolean;

  /** Loading operation succeeded */
  isSuccess: boolean;

  /** Loading operation failed */
  isError: boolean;

  /** Loading progress (0-100) */
  progress: number;

  /** Current operation status message */
  statusMessage: string | null;

  /** Error details if load failed */
  error: ChatLoadingError | null;

  /** Items currently being loaded */
  loadingItems: {
    baseModel: boolean;
    adapters: string[];
  };

  /** Items that failed to load */
  failedItems: {
    baseModel: boolean;
    adapters: string[];
  };

  /** Items successfully loaded */
  successItems: {
    baseModel: boolean;
    adapters: string[];
  };
}

/**
 * Return type for useAdapterStates hook
 *
 * Lightweight hook for monitoring adapter states via SSE.
 * Focuses on real-time state tracking without loading controls.
 *
 * @example
 * ```tsx
 * const {
 *   adapterStates,
 *   getAdapterState,
 *   isConnected,
 *   lastUpdate,
 * } = useAdapterStates({ stackId: 'my-stack' });
 *
 * const state = getAdapterState('adapter-123');
 * ```
 */
export interface UseAdapterStatesResult {
  /** Map of adapter ID to current state */
  adapterStates: Map<string, AdapterStateInfo>;

  /** Get state for a specific adapter */
  getAdapterState: (adapterId: string) => AdapterStateInfo | undefined;

  /** Filter adapters by lifecycle state */
  filterByState: (state: AdapterLifecycleState) => AdapterStateInfo[];

  /** Filter adapters by readiness */
  filterByReadiness: (ready: boolean) => AdapterStateInfo[];

  /** SSE connection active */
  isConnected: boolean;

  /** Timestamp of last state update */
  lastUpdate: number | null;

  /** Total number of adapters being tracked */
  totalAdapters: number;

  /** Number of ready adapters */
  readyAdapters: number;

  /** Number of loading adapters */
  loadingAdapters: number;

  /** Number of errored adapters */
  erroredAdapters: number;
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

/**
 * Configuration options for useModelLoader
 */
export interface UseModelLoaderOptions {
  /** Stack ID for adapter loading (required) */
  stackId?: string;

  /** Tenant ID for base model (default: 'default') */
  tenantId?: string;

  /** Auto-load on mount (default: false) */
  autoLoad?: boolean;

  /** Maximum retry attempts for failed loads (default: 3) */
  maxRetries?: number;

  /** Retry delay in ms (default: 2000) */
  retryDelay?: number;

  /** Parallel adapter loading (default: false, loads sequentially) */
  parallelLoading?: boolean;

  /** Callback when load completes */
  onLoadComplete?: () => void;

  /** Callback when load fails */
  onLoadError?: (error: ChatLoadingError) => void;

  /** Callback on progress update */
  onProgress?: (progress: number, statusMessage: string) => void;
}

/**
 * Configuration options for useAdapterStates
 */
export interface UseAdapterStatesOptions {
  /** Stack ID to monitor (required) */
  stackId?: string;

  /** Enable SSE subscription (default: true) */
  enabled?: boolean;

  /** Include detailed metadata in state (default: false) */
  includeMetadata?: boolean;

  /** Callback when any adapter state changes */
  onStateChange?: (adapterId: string, state: AdapterStateInfo) => void;

  /** Filter adapters by lifecycle state */
  filterStates?: AdapterLifecycleState[];

  /** Auto-refresh stale states older than N ms (default: disabled) */
  refreshStaleAfterMs?: number;
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
  return status === 'loaded';
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
