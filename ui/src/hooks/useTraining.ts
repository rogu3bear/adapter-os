import { useQuery, useMutation, UseQueryOptions, UseMutationOptions } from '@tanstack/react-query';
import apiClient from '@/api/client';
import type {
  TrainingJob,
  TrainingTemplate,
  Dataset,
  ListDatasetsResponse,
  DatasetValidationResult,
  StartTrainingRequest,
  CreateDatasetRequest,
  DatasetResponse,
  ListTrainingJobsResponse,
  TrainingArtifactsResponse,
} from '@/api/training-types';

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

const QUERY_KEYS = {
  trainingJobs: ['training', 'jobs'] as const,
  trainingJob: (id: string) => ['training', 'jobs', id] as const,
  jobLogs: (id: string) => ['training', 'jobs', id, 'logs'] as const,
  jobMetrics: (id: string) => ['training', 'jobs', id, 'metrics'] as const,
  jobArtifacts: (id: string) => ['training', 'jobs', id, 'artifacts'] as const,
  datasets: ['training', 'datasets'] as const,
  dataset: (id: string) => ['training', 'datasets', id] as const,
  templates: ['training', 'templates'] as const,
  template: (id: string) => ['training', 'templates', id] as const,
};

// Training Jobs Hooks

export function useTrainingJobs(
  params?: { dataset_id?: string; status?: string; adapter_name?: string; template_id?: string; page?: number; page_size?: number },
  options?: Omit<UseQueryOptions<ListTrainingJobsResponse, Error>, 'queryKey' | 'queryFn'>
) {
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
  return useMutation<TrainingJob, Error, StartTrainingRequest>({
    mutationFn: (request) => apiClient.startTraining(request),
    ...options,
  });
}

export function useCancelJob(
  options?: UseMutationOptions<void, Error, string>
) {
  return useMutation<void, Error, string>({
    mutationFn: (jobId) => apiClient.cancelTraining(jobId),
    ...options,
  });
}

export function useJobLogs(
  jobId: string,
  options?: Omit<UseQueryOptions<string[], Error>, 'queryKey' | 'queryFn'>
) {
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
  return useQuery<Dataset, Error>({
    queryKey: QUERY_KEYS.dataset(datasetId),
    queryFn: () => apiClient.getDataset(datasetId),
    enabled: !!datasetId,
    ...options,
  });
}

export function useCreateDataset(
  options?: UseMutationOptions<DatasetResponse, Error, CreateDatasetRequest>
) {
  return useMutation<DatasetResponse, Error, CreateDatasetRequest>({
    mutationFn: (request) => apiClient.createDataset(request),
    ...options,
  });
}

export function useValidateDataset(
  options?: UseMutationOptions<DatasetValidationResult, Error, string>
) {
  return useMutation<DatasetValidationResult, Error, string>({
    mutationFn: (datasetId) => apiClient.validateDataset(datasetId),
    ...options,
  });
}

export function useDeleteDataset(
  options?: UseMutationOptions<void, Error, string>
) {
  return useMutation<void, Error, string>({
    mutationFn: (datasetId) => apiClient.deleteDataset(datasetId),
    ...options,
  });
}

// Templates Hooks

export function useTemplates(
  options?: Omit<UseQueryOptions<TrainingTemplate[], Error>, 'queryKey' | 'queryFn'>
) {
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
  return useQuery<TrainingTemplate, Error>({
    queryKey: QUERY_KEYS.template(templateId),
    queryFn: () => apiClient.getTrainingTemplate(templateId),
    enabled: !!templateId,
    ...options,
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
  useCreateDataset,
  useValidateDataset,
  useDeleteDataset,
  useTemplates,
  useTemplate,
};
