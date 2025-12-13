/**
 * React Query hooks for Evidence API
 *
 * Provides hooks for CRUD operations on evidence entries with filtering.
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/api/client';
import { toast } from 'sonner';
import type {
  Evidence,
  CreateEvidenceRequest,
  ListEvidenceQuery,
} from '@/api/document-types';

// Query keys for cache management
export const evidenceKeys = {
  all: ['evidence'] as const,
  lists: () => [...evidenceKeys.all, 'list'] as const,
  list: (filter?: ListEvidenceQuery) => [...evidenceKeys.lists(), filter] as const,
  details: () => [...evidenceKeys.all, 'detail'] as const,
  detail: (id: string) => [...evidenceKeys.details(), id] as const,
  byDataset: (datasetId: string) => [...evidenceKeys.all, 'dataset', datasetId] as const,
  byAdapter: (adapterId: string) => [...evidenceKeys.all, 'adapter', adapterId] as const,
};

/**
 * Hook for listing evidence with optional filters
 */
export function useEvidence(filter?: ListEvidenceQuery, options?: { enabled?: boolean }) {
  return useQuery({
    queryKey: evidenceKeys.list(filter),
    queryFn: () => apiClient.listEvidence(filter),
    enabled: options?.enabled ?? true,
    meta: {
      errorMessage: 'Failed to load evidence entries',
    },
  });
}

/**
 * Hook for getting a single evidence entry
 */
export function useEvidenceEntry(evidenceId: string | undefined) {
  return useQuery({
    queryKey: evidenceKeys.detail(evidenceId ?? ''),
    queryFn: () => apiClient.getEvidence(evidenceId!),
    enabled: !!evidenceId,
    meta: {
      errorMessage: 'Failed to load evidence entry',
    },
  });
}

/**
 * Hook for getting evidence entries for a dataset
 */
export function useDatasetEvidence(datasetId: string | undefined) {
  return useQuery({
    queryKey: evidenceKeys.byDataset(datasetId ?? ''),
    queryFn: () => apiClient.getDatasetEvidence(datasetId!),
    enabled: !!datasetId,
    meta: {
      errorMessage: 'Failed to load dataset evidence',
    },
  });
}

/**
 * Hook for getting evidence entries for an adapter
 */
export function useAdapterEvidence(adapterId: string | undefined) {
  return useQuery({
    queryKey: evidenceKeys.byAdapter(adapterId ?? ''),
    queryFn: () => apiClient.getAdapterEvidence(adapterId!),
    enabled: !!adapterId,
    meta: {
      errorMessage: 'Failed to load adapter evidence',
    },
  });
}

/**
 * Hook providing all evidence CRUD operations with cache invalidation
 */
export function useEvidenceApi(filter?: ListEvidenceQuery, options?: { enabled?: boolean }) {
  const queryClient = useQueryClient();

  const createMutation = useMutation({
    mutationFn: (request: CreateEvidenceRequest) => apiClient.createEvidence(request),
    onSuccess: (newEvidence) => {
      queryClient.invalidateQueries({ queryKey: evidenceKeys.lists() });
      // Also invalidate dataset/adapter specific queries
      if (newEvidence.dataset_id) {
        queryClient.invalidateQueries({
          queryKey: evidenceKeys.byDataset(newEvidence.dataset_id),
        });
      }
      if (newEvidence.adapter_id) {
        queryClient.invalidateQueries({
          queryKey: evidenceKeys.byAdapter(newEvidence.adapter_id),
        });
      }
    },
  });

  type DeleteEvidenceParams = string | { evidenceId: string; datasetId?: string; adapterId?: string };

  const deleteMutation = useMutation({
    mutationFn: (params: DeleteEvidenceParams) => {
      const evidenceId = typeof params === 'string' ? params : params.evidenceId;
      return apiClient.deleteEvidence(evidenceId);
    },
    onSuccess: (_data, params) => {
      const evidenceId = typeof params === 'string' ? params : params.evidenceId;
      const datasetId = typeof params === 'string' ? undefined : params.datasetId;
      const adapterId = typeof params === 'string' ? undefined : params.adapterId;

      queryClient.invalidateQueries({ queryKey: evidenceKeys.lists() });
      queryClient.removeQueries({ queryKey: evidenceKeys.detail(evidenceId) });

      if (datasetId) {
        queryClient.invalidateQueries({ queryKey: evidenceKeys.byDataset(datasetId) });
      }
      if (adapterId) {
        queryClient.invalidateQueries({ queryKey: evidenceKeys.byAdapter(adapterId) });
      }
    },
    onError: (error) => {
      const message = error instanceof Error ? error.message : String(error);
      if (message.includes('404') || message.includes('Not Found')) {
        toast.error('Evidence deletion is not available on this backend (v0.9).');
        return;
      }
      toast.error('Failed to delete evidence');
    },
  });

  const downloadMutation = useMutation({
    mutationFn: (params: { evidenceId: string; filename?: string; triggerDownload?: boolean }) =>
      apiClient.downloadEvidence(params.evidenceId, {
        filename: params.filename,
        triggerDownload: params.triggerDownload,
      }),
    onError: (error: unknown) => {
      const message = error instanceof Error ? error.message : 'Failed to download evidence';
      const code = (error as any)?.code || (error as any)?.status;
      toast.error(code ? `Download failed (${code})` : 'Download failed', { description: message });
    },
  });

  return {
    // Queries
    evidence: useEvidence(filter, options),

    // Mutations
    createEvidence: createMutation.mutateAsync,
    isCreating: createMutation.isPending,
    createError: createMutation.error,

    deleteEvidence: deleteMutation.mutateAsync,
    isDeleting: deleteMutation.isPending,
    deleteError: deleteMutation.error,

    downloadEvidence: downloadMutation.mutateAsync,
    isDownloading: downloadMutation.isPending,
    downloadError: downloadMutation.error,

    // Cache invalidation
    invalidateEvidence: () =>
      queryClient.invalidateQueries({ queryKey: evidenceKeys.all }),
  };
}

export default useEvidenceApi;
