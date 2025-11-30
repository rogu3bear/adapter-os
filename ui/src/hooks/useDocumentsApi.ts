/**
 * React Query hooks for Document API
 *
 * Provides hooks for CRUD operations on documents with cache invalidation.
 * Uses the createResourceHooks factory for standardized CRUD operations.
 */

import { useQuery, useMutation, useQueryClient, UseMutationOptions } from '@tanstack/react-query';
import { apiClient } from '../api/client';
import { createResourceHooks } from './factories/createApiHooks';
import type {
  Document,
  DocumentChunk,
} from '../api/document-types';

// Create standard CRUD hooks using the factory
const documentHooks = createResourceHooks<Document, Document, never, never>({
  resourceName: 'documents',
  api: {
    list: () => apiClient.listDocuments(),
    get: (id: string) => apiClient.getDocument(id),
    delete: (id: string) => apiClient.deleteDocument(id),
    // Note: upload and download are custom operations handled separately
  },
  staleTime: 30000,
});

// Export query keys for external use (extends factory keys with custom chunks key)
export const documentKeys = {
  ...documentHooks.keys,
  chunks: (id: string) => [...documentHooks.keys.detail(id), 'chunks'] as const,
};

/**
 * Hook for listing all documents
 */
export const useDocuments = documentHooks.useList;

/**
 * Hook for getting a single document
 */
export const useDocument = documentHooks.useDetail;

/**
 * Hook for deleting a document
 */
export const useDeleteDocument = documentHooks.useDelete;

/**
 * Hook for listing document chunks (custom operation)
 */
export function useDocumentChunks(documentId: string | undefined) {
  return useQuery({
    queryKey: documentKeys.chunks(documentId ?? ''),
    queryFn: () => apiClient.listDocumentChunks(documentId!),
    enabled: !!documentId,
    staleTime: 30000,
  });
}

/**
 * Hook for uploading a document (custom operation - uses File instead of standard create)
 */
export function useUploadDocument(
  options?: Omit<UseMutationOptions<Document, Error, { file: File; name?: string }, unknown>, 'mutationFn'>
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...restOptions } = options ?? {};

  return useMutation<Document, Error, { file: File; name?: string }>({
    mutationFn: ({ file, name }: { file: File; name?: string }) =>
      apiClient.uploadDocument(file, name),
    ...restOptions,
    onSuccess: async (data, variables, ...rest) => {
      queryClient.invalidateQueries({ queryKey: documentKeys.lists() });
      await onSuccess?.(data, variables, ...rest);
    },
  });
}

/**
 * Hook providing all document CRUD operations with cache invalidation
 *
 * @deprecated This combined hook is maintained for backwards compatibility.
 * Consider using individual hooks (useDocuments, useDocument, useUploadDocument, useDeleteDocument) instead.
 */
export function useDocumentsApi() {
  const queryClient = useQueryClient();
  const uploadMutation = useUploadDocument();
  const deleteMutation = useDeleteDocument();

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
