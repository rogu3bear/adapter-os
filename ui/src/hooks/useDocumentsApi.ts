/**
 * React Query hooks for Document API
 *
 * Provides hooks for CRUD operations on documents with cache invalidation.
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '../api/client';
import type {
  Document,
  DocumentChunk,
} from '../api/document-types';

// Query keys for cache management
export const documentKeys = {
  all: ['documents'] as const,
  lists: () => [...documentKeys.all, 'list'] as const,
  list: () => [...documentKeys.lists()] as const,
  details: () => [...documentKeys.all, 'detail'] as const,
  detail: (id: string) => [...documentKeys.details(), id] as const,
  chunks: (id: string) => [...documentKeys.detail(id), 'chunks'] as const,
};

/**
 * Hook for listing all documents
 */
export function useDocuments() {
  return useQuery({
    queryKey: documentKeys.list(),
    queryFn: () => apiClient.listDocuments(),
  });
}

/**
 * Hook for getting a single document
 */
export function useDocument(documentId: string | undefined) {
  return useQuery({
    queryKey: documentKeys.detail(documentId ?? ''),
    queryFn: () => apiClient.getDocument(documentId!),
    enabled: !!documentId,
  });
}

/**
 * Hook for listing document chunks
 */
export function useDocumentChunks(documentId: string | undefined) {
  return useQuery({
    queryKey: documentKeys.chunks(documentId ?? ''),
    queryFn: () => apiClient.listDocumentChunks(documentId!),
    enabled: !!documentId,
  });
}

/**
 * Hook providing all document CRUD operations with cache invalidation
 */
export function useDocumentsApi() {
  const queryClient = useQueryClient();

  const uploadMutation = useMutation({
    mutationFn: ({ file, name }: { file: File; name?: string }) =>
      apiClient.uploadDocument(file, name),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: documentKeys.lists() });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (documentId: string) => apiClient.deleteDocument(documentId),
    onSuccess: (_data, documentId) => {
      queryClient.invalidateQueries({ queryKey: documentKeys.lists() });
      queryClient.removeQueries({ queryKey: documentKeys.detail(documentId) });
    },
  });

  const downloadDocument = async (documentId: string): Promise<Blob> => {
    return apiClient.downloadDocument(documentId);
  };

  return {
    // Queries
    documents: useDocuments(),

    // Mutations
    uploadDocument: uploadMutation.mutateAsync,
    isUploading: uploadMutation.isPending,
    uploadError: uploadMutation.error,

    deleteDocument: deleteMutation.mutateAsync,
    isDeleting: deleteMutation.isPending,
    deleteError: deleteMutation.error,

    // Direct methods
    downloadDocument,

    // Cache invalidation
    invalidateDocuments: () =>
      queryClient.invalidateQueries({ queryKey: documentKeys.all }),
  };
}

export default useDocumentsApi;
