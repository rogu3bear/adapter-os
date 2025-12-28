/**
 * Dashboard Stats Hook
 *
 * Provides computed statistics for dashboard display including
 * dataset stats, training job metrics, and adapter/stack information.
 */

import { useMemo, useCallback } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTraining } from '@/hooks/training';
import { useAdapterStacks, useGetDefaultStack } from '@/hooks/admin/useAdmin';
import { apiClient } from '@/api/services';
import { useTenant } from '@/providers/FeatureProviders';
import { withTenantKey } from '@/utils/tenant';
import type { TrainingJob, DatasetValidationStatus, Adapter, AdapterStack } from '@/api/types';

/**
 * Options for configuring the dashboard stats hook
 */
export interface UseDashboardStatsOptions {
  /** Currently selected tenant/workspace */
  selectedTenant?: string;
}

/**
 * Dataset validation statistics
 */
export interface DatasetStats {
  pending: number;
  validating: number;
  valid: number;
  invalid: number;
  skipped: number;
  draft: number;
  total: number;
}

/**
 * Return type for the dashboard stats hook
 */
export interface UseDashboardStatsReturn {
  // Dataset stats
  datasets: Array<{ validation_status: DatasetValidationStatus }>;
  datasetStats: DatasetStats;
  datasetsLoading: boolean;
  datasetsError: Error | null;
  refetchDatasets: () => void;

  // Training job stats
  trainingJobs: TrainingJob[];
  recentTrainingJob: TrainingJob | null;
  recentCompletedJobWithStack: TrainingJob | null;
  runningJobs: number;
  completedLast7Days: number;
  trainingJobsLoading: boolean;
  trainingJobsError: Error | null;
  refetchTrainingJobs: () => void;

  // Adapter/Stack stats
  adapters: Adapter[];
  adapterTotal: number;
  stacks: AdapterStack[];
  stackTotal: number;
  stackNameLookup: Map<string, string>;
  defaultStack: AdapterStack | null;
  defaultStackLabel: string;
  adaptersLoading: boolean;
  stacksLoading: boolean;
  defaultStackLoading: boolean;
  adapterStackError: Error | null;
  refetchAdapters: () => void;
  refetchStacks: () => void;
  refetchDefaultStack: () => void;
}

/**
 * Hook for computing and managing dashboard statistics.
 *
 * Aggregates data from multiple sources (datasets, training jobs, adapters, stacks)
 * and provides computed metrics for dashboard display.
 *
 * @example
 * ```tsx
 * const {
 *   datasetStats,
 *   runningJobs,
 *   adapterTotal,
 *   defaultStack,
 * } = useDashboardStats({ selectedTenant: 'my-workspace' });
 * ```
 */
export function useDashboardStats(
  options: UseDashboardStatsOptions = {}
): UseDashboardStatsReturn {
  const { selectedTenant: optionsTenant } = options;
  const { selectedTenant: contextTenant } = useTenant();
  // Prefer options tenant if provided, otherwise use context
  const selectedTenant = optionsTenant ?? contextTenant;

  // Datasets query
  const {
    data: datasetsData,
    isLoading: datasetsLoading,
    error: datasetsError,
    refetch: refetchDatasets,
  } = useTraining.useDatasets(undefined, { staleTime: 30000 });

  // Training jobs query
  const {
    data: trainingJobsData,
    isLoading: trainingJobsLoading,
    error: trainingJobsError,
    refetch: refetchTrainingJobs,
  } = useTraining.useTrainingJobs(undefined, {
    refetchInterval: 10000,
    staleTime: 5000,
  });

  // Adapters query - keyed by tenant to prevent cross-workspace data leakage
  const {
    data: adapterList,
    isLoading: adaptersLoading,
    error: adaptersError,
    refetch: refetchAdapters,
  } = useQuery({
    queryKey: withTenantKey(['adapters', 'dashboard'], selectedTenant),
    queryFn: (): Promise<Adapter[]> => apiClient.listAdapters(),
    staleTime: 15_000,
    refetchInterval: 30_000,
    refetchOnWindowFocus: true,
    retry: 1,
  });

  // Stacks query
  const {
    data: stacks = [],
    isLoading: stacksLoading,
    error: stacksError,
    refetch: refetchStacks,
  } = useAdapterStacks();

  // Default stack query
  const {
    data: defaultStack,
    isLoading: defaultStackLoading,
    error: defaultStackError,
    refetch: refetchDefaultStack,
  } = useGetDefaultStack(selectedTenant);

  // Parse timestamp helper
  const parseTimestamp = useCallback((value?: string | null) => {
    if (!value) return 0;
    const time = Date.parse(value);
    return Number.isNaN(time) ? 0 : time;
  }, []);

  // Get timestamp for training job sorting
  const trainingJobTimestamp = useCallback(
    (job: TrainingJob) =>
      parseTimestamp(job.updated_at) ||
      parseTimestamp(job.completed_at) ||
      parseTimestamp(job.created_at) ||
      parseTimestamp(job.started_at),
    [parseTimestamp]
  );

  // Computed dataset values
  const datasets = useMemo(() => datasetsData?.datasets ?? [], [datasetsData]);

  const datasetStats = useMemo((): DatasetStats => {
    const counts: DatasetStats = {
      pending: 0,
      validating: 0,
      valid: 0,
      invalid: 0,
      skipped: 0,
      draft: 0,
      total: datasets.length,
    };

    datasets.forEach((dataset) => {
      const status = dataset.validation_status as keyof Omit<DatasetStats, 'total'>;
      if (status in counts) {
        counts[status] = (counts[status] || 0) + 1;
      }
    });

    return counts;
  }, [datasets]);

  // Computed training job values
  const trainingJobs = useMemo(() => trainingJobsData?.jobs ?? [], [trainingJobsData]);

  const recentTrainingJob = useMemo<TrainingJob | null>(() => {
    if (trainingJobs.length === 0) return null;
    return [...trainingJobs].sort(
      (a, b) => trainingJobTimestamp(b) - trainingJobTimestamp(a)
    )[0];
  }, [trainingJobs, trainingJobTimestamp]);

  const recentCompletedJobWithStack = useMemo<TrainingJob | null>(() => {
    const completed = trainingJobs.filter(
      (job) => job.status === 'completed' && job.stack_id
    );
    if (completed.length === 0) return null;
    return [...completed].sort(
      (a, b) => trainingJobTimestamp(b) - trainingJobTimestamp(a)
    )[0];
  }, [trainingJobs, trainingJobTimestamp]);

  const runningJobs = useMemo(
    () =>
      trainingJobs.filter(
        (job) => job.status === 'running' || job.status === 'pending'
      ).length,
    [trainingJobs]
  );

  const completedLast7Days = useMemo(() => {
    const now = Date.now();
    const windowMs = 7 * 24 * 60 * 60 * 1000;
    return trainingJobs.filter((job) => {
      if (job.status !== 'completed') return false;
      const completedAt = parseTimestamp(
        job.completed_at || job.updated_at || job.created_at
      );
      return completedAt > 0 && now - completedAt <= windowMs;
    }).length;
  }, [trainingJobs, parseTimestamp]);

  // Computed adapter/stack values
  const adapters = useMemo(
    () => (Array.isArray(adapterList) ? adapterList : []),
    [adapterList]
  );
  const adapterTotal = adapters.length;
  const stackTotal = stacks?.length ?? 0;

  const stackNameLookup = useMemo(
    () => new Map(stacks.map((stack) => [stack.id, stack.name])),
    [stacks]
  );

  const defaultStackLabel = defaultStackLoading
    ? 'Stack: loading'
    : defaultStackError
      ? 'Stack: unavailable'
      : defaultStack
        ? `Stack: ${defaultStack.name}`
        : 'Stack: not set';

  const adapterStackError =
    (adaptersError as Error | undefined) ||
    (stacksError as Error | undefined) ||
    (defaultStackError as Error | undefined);

  return {
    // Dataset stats
    datasets,
    datasetStats,
    datasetsLoading,
    datasetsError: datasetsError as Error | null,
    refetchDatasets,

    // Training job stats
    trainingJobs,
    recentTrainingJob,
    recentCompletedJobWithStack,
    runningJobs,
    completedLast7Days,
    trainingJobsLoading,
    trainingJobsError: trainingJobsError as Error | null,
    refetchTrainingJobs,

    // Adapter/Stack stats
    adapters,
    adapterTotal,
    stacks,
    stackTotal,
    stackNameLookup,
    defaultStack: defaultStack ?? null,
    defaultStackLabel,
    adaptersLoading,
    stacksLoading,
    defaultStackLoading,
    adapterStackError: adapterStackError ?? null,
    refetchAdapters,
    refetchStacks,
    refetchDefaultStack,
  };
}
