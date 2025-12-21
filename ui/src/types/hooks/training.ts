/**
 * Training Hook Types
 *
 * Type definitions for training-related hooks including preflight checks,
 * notifications, and job monitoring.
 */

import type { Dataset } from '@/api/training-types';
import type { PolicyCheck } from '@/components/PolicyPreflightDialog';

// ============================================================================
// useTrainingPreflight Types
// ============================================================================

export interface TrainingPreflightResult {
  /** Whether all critical checks passed (training can proceed) */
  canProceed: boolean;
  /** Whether all checks passed cleanly (no warnings) */
  isClean: boolean;
  /** Client-side check results (instant) */
  clientChecks: PolicyCheck[];
  /** Server-side check results (async) */
  serverChecks: PolicyCheck[];
  /** All checks combined */
  allChecks: PolicyCheck[];
  /** Summary message */
  summary: string;
  /** Whether server checks are still loading */
  isLoading: boolean;
  /** Error from server checks */
  error: Error | null;
  /** Refetch server checks */
  refetch: () => void;
}

export interface UseTrainingPreflightOptions {
  /** Whether to enable server-side checks */
  enabled?: boolean;
  /** Tenant ID for model status check */
  tenantId?: string;
}

// ============================================================================
// useTrainingNotifications Types
// ============================================================================

export interface UseTrainingNotificationsOptions {
  /** Training job ID to monitor */
  jobId: string;
  /** Enable/disable notifications */
  enabled?: boolean;
  /** Callback when job starts */
  onJobStart?: (jobId: string) => void;
  /** Callback on progress update */
  onProgress?: (progress: number) => void;
  /** Callback when job completes */
  onJobComplete?: (jobId: string, success: boolean) => void;
  /** Callback on error */
  onError?: (error: Error) => void;
}

// ============================================================================
// useBatchedTrainingNotifications Types
// ============================================================================

export interface UseBatchedTrainingNotificationsOptions {
  /** Array of job IDs to monitor */
  jobIds: string[];
  /** Enable/disable notifications */
  enabled?: boolean;
  /** Batch size for notifications */
  batchSize?: number;
  /** Callback when all jobs complete */
  onAllComplete?: (results: Record<string, boolean>) => void;
}

// ============================================================================
// useTrainingJob Types (common pattern)
// ============================================================================

export interface UseTrainingJobOptions {
  /** Training job ID */
  jobId: string;
  /** Enable/disable the query */
  enabled?: boolean;
  /** Polling interval in ms */
  pollingInterval?: number;
}

export interface UseTrainingJobReturn {
  /** Job details */
  job: unknown | null;
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
  /** Refetch job details */
  refetch: () => Promise<void>;
}

// ============================================================================
// useTrainingMonitor Types
// ============================================================================

export interface UseTrainingMonitorReturn {
  /** Active training jobs */
  activeJobs: unknown[];
  /** Completed jobs count */
  completedCount: number;
  /** Failed jobs count */
  failedCount: number;
  /** Overall progress percentage */
  overallProgress: number;
  /** Whether any jobs are running */
  hasActiveJobs: boolean;
}
