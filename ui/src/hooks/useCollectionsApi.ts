/**
 * React Query hooks for Collection API
 * 【2025-11-25†prd-ux-01†collections_api_hook】
 *
 * Provides hooks for CRUD operations on collections with cache invalidation.
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '../api/client';
import type {
  Collection,
  CollectionDetail,
} from '../api/document-types';

// Query keys for cache management
export const collectionKeys = {
  all: ['collections'] as const,
  lists: () => [...collectionKeys.all, 'list'] as const,
  list: () => [...collectionKeys.lists()] as const,
  details: () => [...collectionKeys.all, 'detail'] as const,
  detail: (id: string) => [...collectionKeys.details(), id] as const,
};

/**
 * Hook for listing all collections
 */
export function useCollections() {
  return useQuery({
    queryKey: collectionKeys.list(),
    queryFn: () => apiClient.listCollections(),
    staleTime: 30000, // 30 seconds
    refetchOnWindowFocus: true,
  });
}

/**
 * Hook for getting a single collection with documents
 */
export function useCollection(collectionId: string | undefined) {
  return useQuery({
    queryKey: collectionKeys.detail(collectionId ?? ''),
    queryFn: () => apiClient.getCollection(collectionId!),
    enabled: !!collectionId,
    staleTime: 30000, // 30 seconds
  });
}

/**
 * Hook providing all collection CRUD operations with cache invalidation
 */
export function useCollectionsApi() {
  const queryClient = useQueryClient();

  const createMutation = useMutation({
    mutationFn: ({ name, description }: { name: string; description?: string }) =>
      apiClient.createCollection(name, description),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: collectionKeys.lists() });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (collectionId: string) => apiClient.deleteCollection(collectionId),
    onSuccess: (_data, collectionId) => {
      queryClient.invalidateQueries({ queryKey: collectionKeys.lists() });
      queryClient.removeQueries({ queryKey: collectionKeys.detail(collectionId) });
    },
  });

  const addDocumentMutation = useMutation({
    mutationFn: ({
      collectionId,
      documentId,
    }: {
      collectionId: string;
      documentId: string;
    }) => apiClient.addDocumentToCollection(collectionId, documentId),
    onSuccess: (_data, { collectionId }) => {
      queryClient.invalidateQueries({ queryKey: collectionKeys.detail(collectionId) });
      queryClient.invalidateQueries({ queryKey: collectionKeys.lists() });
    },
  });

  const removeDocumentMutation = useMutation({
    mutationFn: ({
      collectionId,
      documentId,
    }: {
      collectionId: string;
      documentId: string;
    }) => apiClient.removeDocumentFromCollection(collectionId, documentId),
    onSuccess: (_data, { collectionId }) => {
      queryClient.invalidateQueries({ queryKey: collectionKeys.detail(collectionId) });
      queryClient.invalidateQueries({ queryKey: collectionKeys.lists() });
    },
  });

  return {
    // Queries
    collections: useCollections(),

    // Mutations
    createCollection: createMutation.mutateAsync,
    isCreating: createMutation.isPending,
    createError: createMutation.error,

    deleteCollection: deleteMutation.mutateAsync,
    isDeleting: deleteMutation.isPending,
    deleteError: deleteMutation.error,

    addDocumentToCollection: addDocumentMutation.mutateAsync,
    isAddingDocument: addDocumentMutation.isPending,
    addDocumentError: addDocumentMutation.error,

    removeDocumentFromCollection: removeDocumentMutation.mutateAsync,
    isRemovingDocument: removeDocumentMutation.isPending,
    removeDocumentError: removeDocumentMutation.error,

    // Cache invalidation
    invalidateCollections: () =>
      queryClient.invalidateQueries({ queryKey: collectionKeys.all }),
  };
}

export default useCollectionsApi;
