/**
 * Model loading hooks - Unified model + adapter readiness tracking
 *
 * Primary exports for checking if chat inference is ready.
 */

// Re-export all types from types file
export * from './types';

// Main composition hook
export {
  useModelLoadingState,
  default as default,
} from './useModelLoadingState';

export {
  useAdapterStates,
  type AdapterStateInfo,
  type UseAdapterStatesOptions,
  type UseAdapterStatesResult,
} from './useAdapterStates';

// Model loader - coordinated base model + adapter loading
export {
  useModelLoader,
  type UseModelLoaderResult,
  type ModelLoaderError,
} from './useModelLoader';

// Loading coordinator for race prevention
export {
  loadingCoordinator,
  LoadingCoordinator,
  type LoadingState,
} from './internal/loadingCoordinator';

// Screen reader announcements for loading state
export {
  useLoadingAnnouncements,
  type LoadingPhase,
  type LoadingAnnouncementState,
  type UseLoadingAnnouncementsOptions,
  type UseLoadingAnnouncementsResult,
} from './useLoadingAnnouncements';

// SessionStorage persistence for loading state recovery
export {
  useChatLoadingPersistence,
  type ChatLoadingState,
  type UseChatLoadingPersistenceOptions,
  type UseChatLoadingPersistenceReturn,
} from './useChatLoadingPersistence';

// Model status monitoring
export {
  useModelStatus,
  MODEL_STATUS_EVENT,
  type ModelStatusState,
  type ModelStatusEventDetail,
  type UseModelStatusReturn,
} from './useModelStatus';

// Auto-load model for operator experience
export {
  useAutoLoadModel,
  type AutoLoadError,
  type UseAutoLoadModelReturn,
} from './useAutoLoadModel';
