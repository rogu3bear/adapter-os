import { BackendName, BackendStatus, CoremlPackageStatus } from '@/api/types';

/**
 * Backend option with availability and status information
 */
export interface BackendOption {
  name: BackendName;
  available: boolean;
  status?: BackendStatus['status'];
  mode?: BackendStatus['mode'];
  notes?: string[];
  hardwareHint?: string;
}

/**
 * Inference performance metrics
 */
export interface InferenceMetrics {
  latency: number;
  tokensPerSecond: number;
  totalTokens: number;
}

/**
 * CoreML export status with UI display properties
 */
export interface CoreMLStatusDisplay {
  label: string;
  variant: 'default' | 'secondary' | 'destructive' | 'outline';
}

/**
 * CoreML management state
 */
export interface CoreMLState {
  status: CoremlPackageStatus | null;
  isLoading: boolean;
  actionInProgress: 'export' | 'verify' | null;
}

/**
 * Backend selection result from fallback resolution
 */
export interface BackendSelectionResult {
  backend: BackendName;
  reason: string | null;
}

/**
 * Inference URL state parameters
 */
export interface InferenceUrlParams {
  modelId?: string;
  adapterId?: string;
  stackId?: string;
  backend?: string;
}

/**
 * Inference mode types
 */
export type InferenceMode = 'standard' | 'streaming' | 'batch';

/**
 * Playground view mode types
 */
export type PlaygroundMode = 'single' | 'comparison';
