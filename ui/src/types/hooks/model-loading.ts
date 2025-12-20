/**
 * Model Loading Hook Types
 *
 * Type definitions for model loading hooks including status tracking,
 * adapter states, auto-loading, and persistence.
 */

// ============================================================================
// useModelStatus Types
// ============================================================================

export type ModelLoadingStatus = 'checking' | 'loading' | 'ready' | 'no-model' | 'error';

export interface UseModelStatusReturn {
  /** Current model status */
  status: ModelLoadingStatus;
  /** Model name if loaded */
  modelName: string | null;
  /** Error message if status is error */
  errorMessage: string | null;
  /** Loading progress (0-100) */
  progress: number;
  /** Refresh status */
  refresh: () => Promise<void>;
  /** Whether status is being checked */
  isChecking: boolean;
}

// ============================================================================
// useModelLoader Types
// ============================================================================

export interface UseModelLoaderResult {
  /** Load a model */
  loadModel: (modelId: string, tenantId?: string) => Promise<void>;
  /** Unload current model */
  unloadModel: () => Promise<void>;
  /** Current loading status */
  status: ModelLoadingStatus;
  /** Current model ID */
  currentModelId: string | null;
  /** Loading progress */
  progress: number;
  /** Error if any */
  error: Error | null;
  /** Whether a model is loaded */
  isLoaded: boolean;
  /** Whether loading is in progress */
  isLoading: boolean;
}

// ============================================================================
// useAdapterStates Types
// ============================================================================

export interface UseAdapterStatesOptions {
  /** Tenant ID */
  tenantId?: string;
  /** Enable/disable polling */
  enabled?: boolean;
  /** Polling interval in ms */
  pollingInterval?: number;
  /** Callback when states change */
  onStatesChange?: (states: Map<string, unknown>) => void;
}

export interface UseAdapterStatesResult {
  /** Adapter states map (adapterId -> state) */
  adapterStates: Map<string, unknown>;
  /** Get state for specific adapter */
  getAdapterState: (adapterId: string) => unknown | null;
  /** Whether adapter is loaded */
  isAdapterLoaded: (adapterId: string) => boolean;
  /** Refresh states */
  refresh: () => Promise<void>;
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
}

// ============================================================================
// useAutoLoadModel Types
// ============================================================================

export interface UseAutoLoadModelReturn {
  /** Whether auto-load is enabled */
  isEnabled: boolean;
  /** Enable auto-load */
  enable: () => void;
  /** Disable auto-load */
  disable: () => void;
  /** Toggle auto-load */
  toggle: () => void;
  /** Last auto-loaded model */
  lastLoadedModel: string | null;
}

// ============================================================================
// useChatLoadingPersistence Types
// ============================================================================

export interface UseChatLoadingPersistenceOptions {
  /** Chat session ID */
  sessionId: string;
  /** Enable/disable persistence */
  enabled?: boolean;
  /** Auto-restore on mount */
  autoRestore?: boolean;
}

export interface UseChatLoadingPersistenceReturn {
  /** Save loading state */
  saveLoadingState: (state: unknown) => void;
  /** Restore loading state */
  restoreLoadingState: () => unknown | null;
  /** Clear saved state */
  clearLoadingState: () => void;
  /** Whether state exists */
  hasPersistedState: boolean;
}

// ============================================================================
// useLoadingAnnouncements Types
// ============================================================================

export interface UseLoadingAnnouncementsOptions {
  /** Enable/disable announcements */
  enabled?: boolean;
  /** Announcement duration in ms */
  duration?: number;
  /** Callback when announcement shown */
  onAnnouncement?: (message: string) => void;
}

export interface UseLoadingAnnouncementsResult {
  /** Current announcement */
  currentAnnouncement: string | null;
  /** Show announcement */
  announce: (message: string) => void;
  /** Clear announcement */
  clearAnnouncement: () => void;
  /** Whether announcement is visible */
  isVisible: boolean;
}

// ============================================================================
// useModelLoadingState Types (from types.ts)
// ============================================================================

export interface UseModelLoadingStateOptions {
  /** Tenant ID */
  tenantId?: string;
  /** Enable polling */
  enablePolling?: boolean;
  /** Polling interval in ms */
  pollingInterval?: number;
  /** SSE endpoint */
  sseEndpoint?: string;
  /** Callback on state change */
  onStateChange?: (state: unknown) => void;
}

export interface UseModelLoadingStateResult {
  /** Current loading state */
  loadingState: unknown | null;
  /** Whether model is loading */
  isLoading: boolean;
  /** Loading progress */
  progress: number;
  /** Current phase */
  currentPhase: string | null;
  /** Estimated time remaining in ms */
  estimatedTimeRemaining: number | null;
  /** Error if any */
  error: Error | null;
  /** Refresh state */
  refresh: () => Promise<void>;
}
