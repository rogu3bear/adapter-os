// Core inference hooks
export { useInferenceConfig } from './useInferenceConfig';
export type {
  UseInferenceConfigOptions,
  UseInferenceConfigReturn,
} from './useInferenceConfig';

export { useBatchInference } from './useBatchInference';
export type {
  UseBatchInferenceOptions,
  UseBatchInferenceReturn,
  BatchInferenceResult,
  BatchMetrics,
} from './useBatchInference';

export { useInferenceSessions } from './useInferenceSessions';
export type {
  UseInferenceSessionsOptions,
  UseInferenceSessionsReturn,
} from './useInferenceSessions';

export { useStreamingInference } from './useStreamingInference';
export type {
  StreamingToken,
  StreamingState,
  UseStreamingInferenceOptions,
  UseStreamingInferenceReturn,
} from './useStreamingInference';

// Feature hooks
export { useBackendSelection } from './useBackendSelection';
export type {
  UseBackendSelectionOptions,
  UseBackendSelectionReturn,
} from './useBackendSelection';

export { useCoreMLManagement } from './useCoreMLManagement';
export type {
  UseCoreMLManagementOptions,
  UseCoreMLManagementReturn,
} from './useCoreMLManagement';

export { useAdapterSelection } from './useAdapterSelection';
export type {
  UseAdapterSelectionOptions,
  UseAdapterSelectionReturn,
} from './useAdapterSelection';

export { useInferenceUrlState } from './useInferenceUrlState';
export type { UseInferenceUrlStateReturn } from './useInferenceUrlState';
