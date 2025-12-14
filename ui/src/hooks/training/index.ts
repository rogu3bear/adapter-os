/**
 * Training-related hooks
 *
 * This module exports all training-related hooks for managing training jobs,
 * datasets, templates, and notifications.
 */

// Core training hooks
export * from './useTraining';

// Training notifications
export {
  useTrainingNotifications,
  useAdapterCreationNotifications,
  useStackUpdateNotifications,
  globalNotifiedJobs,
  globalNotifiedAdapters,
} from './useTrainingNotifications';

// Batched training notifications
export { useBatchedTrainingNotifications } from './useBatchedTrainingNotifications';

// Behavior training
export * from './useBehaviorTraining';

// Training preflight checks
export { useTrainingPreflight } from './useTrainingPreflight';
export type { TrainingPreflightResult } from './useTrainingPreflight';

// Training data orchestration
export { useTrainingDataOrchestrator } from './useTrainingDataOrchestrator';
export type {
  TrainingDataSource,
  TrainingDataOrchestratorResult,
  TrainingDataOrchestratorState,
} from './useTrainingDataOrchestrator';
