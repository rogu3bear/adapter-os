import { useRef } from 'react';
import { useQuery, useMutation, useQueryClient, UseQueryOptions, UseMutationOptions, type QueryClient } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import type {
  TrainingJob,
  TrainingTemplate,
  Dataset,
  ListDatasetsResponse,
  DatasetValidationResult,
  StartTrainingRequest,
  CreateDatasetRequest,
  DatasetResponse,
  DatasetVersionListResponse,
  ListTrainingJobsResponse,
  TrainingArtifactsResponse,
  CreateDatasetFromDocumentsResponse,
  ChatBootstrapResponse,
  CreateChatFromJobRequest,
  CreateChatFromJobResponse,
} from '@/api/training-types';
import { useTenant } from '@/providers/FeatureProviders';
import { withTenantKey } from '@/utils/tenant';

/**
 * Error thrown when workspace changes during a mutation operation.
 * This prevents cross-workspace data corruption.
 */
export class WorkspaceChangedError extends Error {
  constructor(originalTenant: string, currentTenant: string) {
    super(`Workspace changed during operation (${originalTenant} → ${currentTenant}). Please retry.`);
    this.name = 'WorkspaceChangedError';
  }
}

type TrainingMetrics = {
  step?: number;
  loss?: number;
  learning_rate?: number;
  epoch?: number;
  tokens_processed?: number;
  tokens_per_second?: number;
  time_elapsed?: number;
  eta_seconds?: number;
  progress_pct?: number;
  memory_usage?: number;
  gpu_utilization?: number;
  current_epoch?: number;
  total_epochs?: number;
  validation_loss?: number;
};

const createQueryKeys = (tenantId?: string | null) => ({
  trainingJobs: withTenantKey(['training', 'jobs'], tenantId),
  trainingJob: (id: string) => withTenantKey(['training', 'jobs', id], tenantId),
  jobLogs: (id: string) => withTenantKey(['training', 'jobs', id, 'logs'], tenantId),
  jobMetrics: (id: string) => withTenantKey(['training', 'jobs', id, 'metrics'], tenantId),
  jobArtifacts: (id: string) => withTenantKey(['training', 'jobs', id, 'artifacts'], tenantId),
  chatBootstrap: (jobId: string) => withTenantKey(['training', 'chat-bootstrap', jobId], tenantId),
  datasets: withTenantKey(['training', 'datasets'], tenantId),
  dataset: (id: string) => withTenantKey(['training', 'datasets', id], tenantId),
  datasetVersions: (id: string) => withTenantKey(['training', 'datasets', id, 'versions'], tenantId),
  templates: withTenantKey(['training', 'templates'], tenantId),
  template: (id: string) => withTenantKey(['training', 'templates', id], tenantId),
});

export async function invalidateTrainingCaches(queryClient: QueryClient, tenantId?: string | null) {
  const QUERY_KEYS = createQueryKeys(tenantId);
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: QUERY_KEYS.trainingJobs }),
    queryClient.invalidateQueries({ queryKey: QUERY_KEYS.datasets }),
  ]);
}

// Training Jobs Hooks

export function useTrainingJobs(
  params?: { dataset_id?: string; status?: string; adapter_name?: string; template_id?: string; page?: number; page_size?: number },
  options?: Omit<UseQueryOptions<ListTrainingJobsResponse, Error>, 'queryKey' | 'queryFn'>
) {
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useQuery<ListTrainingJobsResponse, Error>({
    queryKey: [...QUERY_KEYS.trainingJobs, params],
    queryFn: async () => {
      // API now returns ListTrainingJobsResponse directly
      return await apiClient.listTrainingJobs(params);
    },
    refetchInterval: 5000, // Poll every 5 seconds for active jobs
    ...options,
  });
}

export function useTrainingJob(
  jobId: string,
  options?: Omit<UseQueryOptions<TrainingJob, Error>, 'queryKey' | 'queryFn'>
) {
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useQuery<TrainingJob, Error>({
    queryKey: QUERY_KEYS.trainingJob(jobId),
    queryFn: () => apiClient.getTrainingJob(jobId),
    enabled: !!jobId,
    refetchInterval: (query) => {
      // Poll every 2-3 seconds while job is running or pending
      const job = query.state.data;
      if (job && (job.status === 'running' || job.status === 'pending')) {
        return 2500; // 2.5 seconds
      }
      return false; // Stop polling when completed/failed/cancelled
    },
    ...options,
  });
}

export function useStartTraining(
  options?: UseMutationOptions<TrainingJob, Error, StartTrainingRequest>
) {
  const tenantContext = useTenant();
  const tenantAtCreationRef = useRef(tenantContext.selectedTenant);

  return useMutation<TrainingJob, Error, StartTrainingRequest>({
    mutationFn: async (request) => {
      // Validate workspace hasn't changed
      const tenantAtCreation = tenantAtCreationRef.current;
      const currentTenant = tenantContext.selectedTenant;
      if (tenantAtCreation && currentTenant && tenantAtCreation !== currentTenant) {
        throw new WorkspaceChangedError(tenantAtCreation, currentTenant);
      }
      return apiClient.startTraining(request);
    },
    ...options,
  });
}

export function useCancelJob(
  options?: UseMutationOptions<void, Error, string>
) {
  const tenantContext = useTenant();
  const tenantAtCreationRef = useRef(tenantContext.selectedTenant);

  return useMutation<void, Error, string>({
    mutationFn: async (jobId) => {
      // Validate workspace hasn't changed
      const tenantAtCreation = tenantAtCreationRef.current;
      const currentTenant = tenantContext.selectedTenant;
      if (tenantAtCreation && currentTenant && tenantAtCreation !== currentTenant) {
        throw new WorkspaceChangedError(tenantAtCreation, currentTenant);
      }
      return apiClient.cancelTraining(jobId);
    },
    ...options,
  });
}

export function useJobLogs(
  jobId: string,
  options?: Omit<UseQueryOptions<string[], Error>, 'queryKey' | 'queryFn'>
) {
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useQuery<string[], Error>({
    queryKey: QUERY_KEYS.jobLogs(jobId),
    queryFn: () => apiClient.getTrainingLogs(jobId),
    enabled: !!jobId,
    refetchInterval: 2000, // Poll every 2 seconds for active jobs
    ...options,
  });
}

export function useJobMetrics(
  jobId: string,
  options?: Omit<UseQueryOptions<TrainingMetrics, Error>, 'queryKey' | 'queryFn'>
) {
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useQuery<TrainingMetrics, Error>({
    queryKey: QUERY_KEYS.jobMetrics(jobId),
    queryFn: () => apiClient.getTrainingMetrics(jobId),
    enabled: !!jobId,
    refetchInterval: 2000, // Poll every 2 seconds for active jobs
    ...options,
  });
}

export function useJobArtifacts(
  jobId: string,
  options?: Omit<UseQueryOptions<TrainingArtifactsResponse, Error>, 'queryKey' | 'queryFn'>
) {
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useQuery<TrainingArtifactsResponse, Error>({
    queryKey: QUERY_KEYS.jobArtifacts(jobId),
    queryFn: () => apiClient.getTrainingArtifacts(jobId),
    enabled: !!jobId,
    ...options,
  });
}

// Datasets Hooks

export function useDatasets(
  params?: { page?: number; page_size?: number },
  options?: Omit<UseQueryOptions<ListDatasetsResponse, Error>, 'queryKey' | 'queryFn'>
) {
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useQuery<ListDatasetsResponse, Error>({
    queryKey: [...QUERY_KEYS.datasets, params],
    queryFn: () => apiClient.listDatasets(params),
    ...options,
  });
}

export function useDataset(
  datasetId: string,
  options?: Omit<UseQueryOptions<Dataset, Error>, 'queryKey' | 'queryFn'>
) {
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useQuery<Dataset, Error>({
    queryKey: QUERY_KEYS.dataset(datasetId),
    queryFn: () => apiClient.getDataset(datasetId),
    enabled: !!datasetId,
    ...options,
  });
}

export function useDatasetVersions(
  datasetId: string,
  options?: Omit<UseQueryOptions<DatasetVersionListResponse, Error>, 'queryKey' | 'queryFn'>
) {
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useQuery<DatasetVersionListResponse, Error>({
    queryKey: QUERY_KEYS.datasetVersions(datasetId),
    queryFn: () => apiClient.listDatasetVersions(datasetId),
    enabled: !!datasetId,
    ...options,
  });
}

export function useCreateDataset(
  options?: UseMutationOptions<DatasetResponse, Error, CreateDatasetRequest>
) {
  const tenantContext = useTenant();
  const tenantAtCreationRef = useRef(tenantContext.selectedTenant);

  return useMutation<DatasetResponse, Error, CreateDatasetRequest>({
    mutationFn: async (request) => {
      // Validate workspace hasn't changed
      const tenantAtCreation = tenantAtCreationRef.current;
      const currentTenant = tenantContext.selectedTenant;
      if (tenantAtCreation && currentTenant && tenantAtCreation !== currentTenant) {
        throw new WorkspaceChangedError(tenantAtCreation, currentTenant);
      }
      return apiClient.createDataset(request);
    },
    ...options,
  });
}

export function useValidateDataset(
  options?: UseMutationOptions<DatasetValidationResult, Error, string>
) {
  const tenantContext = useTenant();
  const tenantAtCreationRef = useRef(tenantContext.selectedTenant);

  return useMutation<DatasetValidationResult, Error, string>({
    mutationFn: async (datasetId) => {
      // Validate workspace hasn't changed
      const tenantAtCreation = tenantAtCreationRef.current;
      const currentTenant = tenantContext.selectedTenant;
      if (tenantAtCreation && currentTenant && tenantAtCreation !== currentTenant) {
        throw new WorkspaceChangedError(tenantAtCreation, currentTenant);
      }
      return apiClient.validateDataset(datasetId);
    },
    ...options,
  });
}

export function useDeleteDataset(
  options?: UseMutationOptions<void, Error, string>
) {
  const tenantContext = useTenant();
  const tenantAtCreationRef = useRef(tenantContext.selectedTenant);

  return useMutation<void, Error, string>({
    mutationFn: async (datasetId) => {
      // Validate workspace hasn't changed
      const tenantAtCreation = tenantAtCreationRef.current;
      const currentTenant = tenantContext.selectedTenant;
      if (tenantAtCreation && currentTenant && tenantAtCreation !== currentTenant) {
        throw new WorkspaceChangedError(tenantAtCreation, currentTenant);
      }
      return apiClient.deleteDataset(datasetId);
    },
    ...options,
  });
}

/**
 * Create a training dataset from existing documents or a document collection.
 * Converts RAG documents into JSONL training format.
 * Automatically invalidates the datasets cache on success.
 */
export function useCreateDatasetFromDocuments(
  options?: UseMutationOptions<
    CreateDatasetFromDocumentsResponse,
    Error,
    { document_ids?: string[]; documentId?: string; collectionId?: string; name?: string; description?: string }
  >
) {
  const queryClient = useQueryClient();
  const tenantContext = useTenant();
  const tenantAtCreationRef = useRef(tenantContext.selectedTenant);
  const { onSuccess, ...restOptions } = options ?? {};

  return useMutation<
    CreateDatasetFromDocumentsResponse,
    Error,
    { document_ids?: string[]; documentId?: string; collectionId?: string; name?: string; description?: string }
  >({
    mutationFn: async (params) => {
      // Validate workspace hasn't changed
      const tenantAtCreation = tenantAtCreationRef.current;
      const currentTenant = tenantContext.selectedTenant;
      if (tenantAtCreation && currentTenant && tenantAtCreation !== currentTenant) {
        throw new WorkspaceChangedError(tenantAtCreation, currentTenant);
      }
      return apiClient.createDatasetFromDocuments(params);
    },
    ...restOptions,
    onSuccess: async (data, variables, context, mutation) => {
      await invalidateTrainingCaches(queryClient, tenantContext.selectedTenant);
      // Call user-provided onSuccess if any
      await onSuccess?.(data, variables, context, mutation);
    },
  });
}

// Templates Hooks

export function useTemplates(
  options?: Omit<UseQueryOptions<TrainingTemplate[], Error>, 'queryKey' | 'queryFn'>
) {
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useQuery<TrainingTemplate[], Error>({
    queryKey: QUERY_KEYS.templates,
    queryFn: () => apiClient.listTrainingTemplates(),
    ...options,
  });
}

export function useTemplate(
  templateId: string,
  options?: Omit<UseQueryOptions<TrainingTemplate, Error>, 'queryKey' | 'queryFn'>
) {
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useQuery<TrainingTemplate, Error>({
    queryKey: QUERY_KEYS.template(templateId),
    queryFn: () => apiClient.getTrainingTemplate(templateId),
    enabled: !!templateId,
    ...options,
  });
}

// Chat Bootstrap Hooks

/**
 * Hook to get chat bootstrap data for a training job
 * Returns the "recipe" for starting a chat from a completed training job
 *
 * @param jobId - Training job ID (hook is disabled if undefined)
 */
export function useChatBootstrap(
  jobId: string | undefined,
  options?: Omit<UseQueryOptions<ChatBootstrapResponse, Error>, 'queryKey' | 'queryFn'>
) {
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useQuery<ChatBootstrapResponse, Error>({
    queryKey: QUERY_KEYS.chatBootstrap(jobId!),
    queryFn: () => apiClient.getChatBootstrap(jobId!),
    enabled: !!jobId,
    ...options,
  });
}

/**
 * Mutation hook to create a chat session from a training job
 * Creates a chat session bound to the training job's stack in one call
 */
export function useCreateChatFromJob(
  options?: UseMutationOptions<CreateChatFromJobResponse, Error, CreateChatFromJobRequest>
) {
  const queryClient = useQueryClient();
  const tenantContext = useTenant();
  const tenantAtCreationRef = useRef(tenantContext.selectedTenant);
  const { onSuccess, ...restOptions } = options ?? {};

  return useMutation<CreateChatFromJobResponse, Error, CreateChatFromJobRequest>({
    mutationFn: async (request) => {
      // Validate workspace hasn't changed
      const tenantAtCreation = tenantAtCreationRef.current;
      const currentTenant = tenantContext.selectedTenant;
      if (tenantAtCreation && currentTenant && tenantAtCreation !== currentTenant) {
        throw new WorkspaceChangedError(tenantAtCreation, currentTenant);
      }
      return apiClient.createChatFromTrainingJob(request);
    },
    ...restOptions,
    onSuccess: async (data, variables, context, mutation) => {
      // Invalidate chat sessions list to show the new session
      await queryClient.invalidateQueries({ queryKey: withTenantKey(['chat', 'sessions'], tenantContext.selectedTenant) });
      // Call user-provided onSuccess if any
      await onSuccess?.(data, variables, context, mutation);
    },
  });
}

// Export as namespace for cleaner usage
export const useTraining = {
  useTrainingJobs,
  useTrainingJob,
  useStartTraining,
  useCancelJob,
  useJobLogs,
  useJobMetrics,
  useJobArtifacts,
  useDatasets,
  useDataset,
  useDatasetVersions,
  useCreateDataset,
  useValidateDataset,
  useDeleteDataset,
  useTemplates,
  useTemplate,
  useChatBootstrap,
  useCreateChatFromJob,
};
