/**
 * useTrainingPreflight - Hook for combined client + server preflight checks
 *
 * Performs dual validation:
 * - Client-side: Instant checks on dataset state
 * - Server-side: Authoritative checks via API
 */

import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';
import apiClient from '@/api/client';
import { useModelStatus } from '@/hooks/model-loading';
import { runClientPreflight, getPreflightSummary } from '@/utils/trainingPreflight';
import type { Dataset } from '@/api/training-types';
import type { PolicyCheck } from '@/components/PolicyPreflightDialog';

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

interface UseTrainingPreflightOptions {
  /** Whether to enable server-side checks */
  enabled?: boolean;
  /** Tenant ID for model status check */
  tenantId?: string;
}

/**
 * Hook for running training preflight checks
 *
 * @param dataset - Dataset to validate (required for client checks)
 * @param options - Configuration options
 * @returns Combined preflight results
 */
export function useTrainingPreflight(
  dataset: Dataset | null | undefined,
  options: UseTrainingPreflightOptions = {}
): TrainingPreflightResult {
  const { enabled = true, tenantId = 'default' } = options;

  // Client-side checks (instant)
  const clientResult = useMemo(() => {
    if (!dataset) {
      return {
        passed: false,
        clean: false,
        checks: [
          {
            policy_id: 'dataset_required',
            policy_name: 'Dataset Required',
            passed: false,
            severity: 'error' as const,
            message: 'No dataset selected',
            details: 'Select a dataset to continue.',
          },
        ],
      };
    }
    return runClientPreflight(dataset);
  }, [dataset]);

  // Server-side: Refetch dataset to confirm existence and current state
  const {
    data: serverDataset,
    isLoading: isDatasetLoading,
    error: datasetError,
    refetch: refetchDataset,
  } = useQuery({
    queryKey: ['training-preflight-dataset', dataset?.id],
    queryFn: () => apiClient.getDataset(dataset!.id),
    enabled: enabled && !!dataset?.id,
    staleTime: 10_000, // Cache for 10 seconds
    retry: 1,
  });

  // Server-side: Check model status
  const modelStatus = useModelStatus(tenantId);

  // Build server-side checks
  const serverChecks = useMemo((): PolicyCheck[] => {
    const checks: PolicyCheck[] = [];

    // Dataset existence check
    if (dataset?.id) {
      if (isDatasetLoading) {
        checks.push({
          policy_id: 'dataset_exists',
          policy_name: 'Dataset Verified',
          passed: true, // Assume pass while loading
          severity: 'info',
          message: 'Verifying dataset...',
        });
      } else if (datasetError) {
        checks.push({
          policy_id: 'dataset_exists',
          policy_name: 'Dataset Verified',
          passed: false,
          severity: 'error',
          message: 'Failed to verify dataset',
          details:
            datasetError instanceof Error
              ? datasetError.message
              : 'Could not reach server. Check your connection.',
        });
      } else if (serverDataset) {
        // Check if server state matches client state
        const statusMatch = serverDataset.validation_status === dataset.validation_status;
        checks.push({
          policy_id: 'dataset_exists',
          policy_name: 'Dataset Verified',
          passed: true,
          severity: statusMatch ? 'info' : 'warning',
          message: statusMatch
            ? 'Dataset verified on server'
            : `Status changed: ${serverDataset.validation_status}`,
          details: !statusMatch
            ? 'Dataset state may have changed. Refresh to see current state.'
            : undefined,
        });
      }
    }

    // Model/worker availability check
    if (modelStatus.status === 'checking') {
      checks.push({
        policy_id: 'worker_available',
        policy_name: 'Worker Available',
        passed: true, // Assume pass while checking
        severity: 'info',
        message: 'Checking model status...',
      });
    } else if (modelStatus.status === 'ready') {
      checks.push({
        policy_id: 'worker_available',
        policy_name: 'Worker Available',
        passed: true,
        severity: 'info',
        message: `Model ready: ${modelStatus.modelName || 'Unknown'}`,
      });
    } else if (modelStatus.status === 'loading') {
      checks.push({
        policy_id: 'worker_available',
        policy_name: 'Worker Available',
        passed: true, // Can still queue training
        severity: 'warning',
        message: 'Model is loading - training will queue',
        details: 'Training job will start once model loading completes.',
      });
    } else if (modelStatus.status === 'no-model') {
      checks.push({
        policy_id: 'worker_available',
        policy_name: 'Worker Available',
        passed: false,
        severity: 'error',
        message: 'No model loaded',
        details: 'Load a base model before starting training.',
      });
    } else if (modelStatus.status === 'error') {
      checks.push({
        policy_id: 'worker_available',
        policy_name: 'Worker Available',
        passed: false,
        severity: 'error',
        message: 'Model error',
        details: modelStatus.errorMessage || 'Model failed to load. Check worker status.',
      });
    }

    return checks;
  }, [dataset, serverDataset, isDatasetLoading, datasetError, modelStatus]);

  // Combine all checks
  const allChecks = useMemo(() => {
    return [...clientResult.checks, ...serverChecks];
  }, [clientResult.checks, serverChecks]);

  // Calculate overall result
  const hasClientError = clientResult.checks.some((c) => !c.passed && c.severity === 'error');
  const hasServerError = serverChecks.some((c) => !c.passed && c.severity === 'error');
  const hasWarning = allChecks.some((c) => c.severity === 'warning');

  const canProceed = !hasClientError && !hasServerError;
  const isClean = canProceed && !hasWarning;
  const isLoading = isDatasetLoading || modelStatus.status === 'checking';

  const summary = useMemo(() => {
    if (isLoading) {
      return 'Running preflight checks...';
    }
    if (!canProceed) {
      const errorCount = allChecks.filter((c) => !c.passed && c.severity === 'error').length;
      return `${errorCount} issue${errorCount !== 1 ? 's' : ''} must be resolved.`;
    }
    if (!isClean) {
      const warningCount = allChecks.filter((c) => c.severity === 'warning').length;
      return `Ready with ${warningCount} warning${warningCount !== 1 ? 's' : ''}.`;
    }
    return 'All checks passed. Ready to start training.';
  }, [isLoading, canProceed, isClean, allChecks]);

  return {
    canProceed,
    isClean,
    clientChecks: clientResult.checks,
    serverChecks,
    allChecks,
    summary,
    isLoading,
    error: datasetError instanceof Error ? datasetError : null,
    refetch: refetchDataset,
  };
}
