/**
 * Inference Hook Types
 *
 * Type definitions for inference-related hooks including streaming,
 * batch inference, adapter selection, and configuration.
 */

import type { StreamingInferRequest } from '@/api/streaming-types';

// ============================================================================
// useStreamingInference Types
// ============================================================================

export interface UseStreamingInferenceOptions {
  /** Callback when token received */
  onToken?: (token: string, metadata?: unknown) => void;
  /** Callback when streaming completes */
  onComplete?: (text: string, metadata?: unknown) => void;
  /** Callback on error */
  onError?: (error: Error) => void;
  /** Auto-start streaming on mount */
  autoStart?: boolean;
  /** Default streaming request parameters */
  defaultRequest?: Partial<StreamingInferRequest>;
}

export interface UseStreamingInferenceReturn {
  /** Streamed text */
  text: string;
  /** Whether streaming is active */
  isStreaming: boolean;
  /** Error state */
  error: Error | null;
  /** Start streaming */
  startStreaming: (request: StreamingInferRequest) => Promise<void>;
  /** Cancel streaming */
  cancelStreaming: () => void;
  /** Reset state */
  reset: () => void;
  /** Tokens received */
  tokensReceived: number;
  /** Stream duration in ms */
  streamDuration: number | null;
}

// ============================================================================
// useBatchInference Types
// ============================================================================

export interface UseBatchInferenceOptions {
  /** Batch size */
  batchSize?: number;
  /** Concurrent requests */
  concurrency?: number;
  /** Callback on batch complete */
  onBatchComplete?: (results: unknown[]) => void;
  /** Callback on item complete */
  onItemComplete?: (result: unknown, index: number) => void;
  /** Callback on error */
  onError?: (error: Error, index: number) => void;
}

export interface UseBatchInferenceReturn {
  /** Execute batch inference */
  executeBatch: (prompts: string[]) => Promise<unknown[]>;
  /** Whether batch is running */
  isRunning: boolean;
  /** Progress (0-100) */
  progress: number;
  /** Results */
  results: unknown[];
  /** Errors */
  errors: Array<{ index: number; error: Error }>;
  /** Cancel batch */
  cancel: () => void;
  /** Reset state */
  reset: () => void;
}

// ============================================================================
// useAdapterSelection Types
// ============================================================================

export interface UseAdapterSelectionOptions {
  /** Initial adapter IDs */
  initialAdapters?: string[];
  /** Maximum adapters allowed */
  maxAdapters?: number;
  /** Callback when selection changes */
  onSelectionChange?: (adapterIds: string[]) => void;
}

export interface UseAdapterSelectionReturn {
  /** Selected adapter IDs */
  selectedAdapters: string[];
  /** Add adapter */
  addAdapter: (adapterId: string) => void;
  /** Remove adapter */
  removeAdapter: (adapterId: string) => void;
  /** Toggle adapter */
  toggleAdapter: (adapterId: string) => void;
  /** Clear all adapters */
  clearAdapters: () => void;
  /** Set adapters */
  setAdapters: (adapterIds: string[]) => void;
  /** Whether max adapters reached */
  isMaxReached: boolean;
}

// ============================================================================
// useBackendSelection Types
// ============================================================================

export interface UseBackendSelectionOptions {
  /** Initial backend */
  initialBackend?: string;
  /** Available backends */
  availableBackends?: string[];
  /** Callback when backend changes */
  onBackendChange?: (backend: string) => void;
}

export interface UseBackendSelectionReturn {
  /** Selected backend */
  selectedBackend: string;
  /** Set backend */
  setBackend: (backend: string) => void;
  /** Available backends */
  availableBackends: string[];
  /** Whether backend is available */
  isBackendAvailable: (backend: string) => boolean;
}

// ============================================================================
// useInferenceConfig Types
// ============================================================================

export interface UseInferenceConfigOptions {
  /** Initial configuration */
  initialConfig?: Record<string, unknown>;
  /** Save config on change */
  autoSave?: boolean;
}

export interface UseInferenceConfigReturn {
  /** Current configuration */
  config: Record<string, unknown>;
  /** Update config value */
  updateConfig: (key: string, value: unknown) => void;
  /** Set entire config */
  setConfig: (config: Record<string, unknown>) => void;
  /** Reset to defaults */
  resetConfig: () => void;
  /** Save config */
  saveConfig: () => Promise<void>;
}

// ============================================================================
// useInferenceSessions Types
// ============================================================================

export interface UseInferenceSessionsOptions {
  /** Enable/disable */
  enabled?: boolean;
  /** Polling interval in ms */
  pollingInterval?: number;
  /** Filter predicate */
  filter?: (session: unknown) => boolean;
}

export interface UseInferenceSessionsReturn {
  /** Inference sessions */
  sessions: unknown[];
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
  /** Refetch sessions */
  refetch: () => Promise<void>;
  /** Create session */
  createSession: (params: unknown) => Promise<unknown>;
  /** Delete session */
  deleteSession: (sessionId: string) => Promise<void>;
}

// ============================================================================
// useCoreMLManagement Types
// ============================================================================

export interface UseCoreMLManagementOptions {
  /** Enable/disable */
  enabled?: boolean;
  /** Polling interval for status */
  pollingInterval?: number;
  /** Callback on model loaded */
  onModelLoaded?: (modelId: string) => void;
  /** Callback on model unloaded */
  onModelUnloaded?: (modelId: string) => void;
}

export interface UseCoreMLManagementReturn {
  /** Loaded models */
  loadedModels: string[];
  /** Load model */
  loadModel: (modelId: string) => Promise<void>;
  /** Unload model */
  unloadModel: (modelId: string) => Promise<void>;
  /** Whether model is loaded */
  isModelLoaded: (modelId: string) => boolean;
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
}

// ============================================================================
// useInferenceUrlState Types
// ============================================================================

export interface UseInferenceUrlStateReturn {
  /** Prompt from URL */
  prompt: string | null;
  /** Adapter IDs from URL */
  adapterIds: string[];
  /** Model from URL */
  model: string | null;
  /** Update URL state */
  updateUrlState: (params: { prompt?: string; adapterIds?: string[]; model?: string }) => void;
  /** Clear URL state */
  clearUrlState: () => void;
}
