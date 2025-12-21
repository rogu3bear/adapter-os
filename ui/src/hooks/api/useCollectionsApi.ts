/**
 * React Query hooks for Collection API
 * 【2025-11-25†prd-ux-01†collections_api_hook】
 * 【2025-11-29†migration/sql†migrated_to_factory】
 *
 * Provides hooks for CRUD operations on collections with cache invalidation.
 * Migrated to use createResourceHooks factory for standardized CRUD operations.
 */

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import type {
  Collection,
  CollectionDetail,
} from '@/api/document-types';
import { createResourceHooks } from '@/hooks/factories/createApiHooks';

// Create standard CRUD hooks using the factory
// Note: create returns Collection (not CollectionDetail), so we use 5th type param
const collectionHooks = createResourceHooks<
  Collection,
  CollectionDetail,
  { name: string; description?: string },
  { name?: string; description?: string },
  Collection // TCreateResult - API returns Collection, not CollectionDetail
>({
  resourceName: 'collections',
  api: {
    list: () => apiClient.listCollections(),
    get: (id: string) => apiClient.getCollection(id),
    create: ({ name, description }) => apiClient.createCollection(name, description),
    delete: (id: string) => apiClient.deleteCollection(id),
  },
  staleTime: 30000, // 30 seconds
  errorMessages: {
    list: 'Failed to load collections',
    detail: 'Failed to load collection',
  },
});

// Export query keys for external cache management
export const collectionKeys = collectionHooks.keys;

/**
 * Hook for listing all collections
 * @deprecated Use collectionHooks.useList() directly or destructure from useCollectionsApi()
 */
export const useCollections = collectionHooks.useList;

/**
 * Hook for getting a single collection with documents
 * @deprecated Use collectionHooks.useDetail(id) directly or destructure from useCollectionsApi()
 */
export const useCollection = collectionHooks.useDetail;

/**
 * Hook for adding a document to a collection
 * Custom operation not covered by standard CRUD
 */
export function useAddDocumentToCollection() {
  const queryClient = useQueryClient();

  return useMutation({
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
    meta: {
      errorMessage: 'Failed to add document to collection',
    },
  });
}

/**
 * Hook for removing a document from a collection
 * Custom operation not covered by standard CRUD
 */
export function useRemoveDocumentFromCollection() {
  const queryClient = useQueryClient();

  return useMutation({
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
    meta: {
      errorMessage: 'Failed to remove document from collection',
    },
  });
}

/**
 * Hook providing all collection CRUD operations with cache invalidation
 * Now uses factory-generated hooks with custom document operations
 */
export function useCollectionsApi() {
  const queryClient = useQueryClient();
  const createMutation = collectionHooks.useCreate();
  const deleteMutation = collectionHooks.useDelete();
  const addDocumentMutation = useAddDocumentToCollection();
  const removeDocumentMutation = useRemoveDocumentFromCollection();

  return {
    // Queries - using factory-generated hooks
    collections: collectionHooks.useList(),

    // Standard CRUD mutations
    createCollection: createMutation.mutateAsync,
    isCreating: createMutation.isPending,
    createError: createMutation.error,

    deleteCollection: deleteMutation.mutateAsync,
    isDeleting: deleteMutation.isPending,
    deleteError: deleteMutation.error,

    // Custom document management mutations
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
