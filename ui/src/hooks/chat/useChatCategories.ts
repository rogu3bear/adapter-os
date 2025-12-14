import { useQuery, useMutation, useQueryClient, UseQueryOptions, UseMutationOptions } from '@tanstack/react-query';
import apiClient from '@/api/client';
import type {
  ChatCategory,
  CreateCategoryRequest,
  UpdateCategoryRequest,
} from '@/api/chat-types';

const QUERY_KEYS = {
  categories: ['chat', 'categories'] as const,
  category: (id: string) => ['chat', 'categories', id] as const,
};

// Categories Hooks

/**
 * List all chat categories for the current tenant
 *
 * Returns categories in tree-sorted order by path (hierarchical)
 */
export function useChatCategories(
  options?: Omit<UseQueryOptions<ChatCategory[], Error>, 'queryKey' | 'queryFn'>
) {
  return useQuery({
    queryKey: QUERY_KEYS.categories,
    queryFn: (): Promise<ChatCategory[]> => apiClient.listChatCategories(),
    meta: {
      errorMessage: 'Failed to load chat categories',
    },
    ...options,
  });
}

/**
 * Create a new chat category
 *
 * @param options - Mutation options
 * @returns Mutation hook for creating a category
 */
export function useCreateCategory(
  options?: UseMutationOptions<ChatCategory, Error, CreateCategoryRequest>
) {
  const queryClient = useQueryClient();

  return useMutation<ChatCategory, Error, CreateCategoryRequest>({
    mutationFn: (request) => apiClient.createChatCategory(request),
    onSuccess: (data, variables, context, mutation) => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.categories });
      options?.onSuccess?.(data, variables, context, mutation);
    },
    ...options,
  });
}

/**
 * Update an existing chat category
 *
 * @param options - Mutation options
 * @returns Mutation hook for updating a category
 */
export function useUpdateCategory(
  options?: UseMutationOptions<ChatCategory, Error, { categoryId: string; request: UpdateCategoryRequest }>
) {
  const queryClient = useQueryClient();

  return useMutation<ChatCategory, Error, { categoryId: string; request: UpdateCategoryRequest }>({
    mutationFn: ({ categoryId, request }) => apiClient.updateChatCategory(categoryId, request),
    onSuccess: (data, variables, context, mutation) => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.categories });
      options?.onSuccess?.(data, variables, context, mutation);
    },
    ...options,
  });
}

/**
 * Delete a chat category
 *
 * @param options - Mutation options
 * @returns Mutation hook for deleting a category
 */
export function useDeleteCategory(
  options?: UseMutationOptions<void, Error, string>
) {
  const queryClient = useQueryClient();

  return useMutation<void, Error, string>({
    mutationFn: (categoryId) => apiClient.deleteChatCategory(categoryId),
    onSuccess: (data, variables, context, mutation) => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.categories });
      queryClient.invalidateQueries({ queryKey: ['chat-sessions'] });
      options?.onSuccess?.(data, variables, context, mutation);
    },
    ...options,
  });
}

/**
 * Set a session's category
 *
 * @param options - Mutation options
 * @returns Mutation hook for setting session category
 */
export function useSetSessionCategory(
  options?: UseMutationOptions<void, Error, { sessionId: string; categoryId: string | null }>
) {
  const queryClient = useQueryClient();

  return useMutation<void, Error, { sessionId: string; categoryId: string | null }>({
    mutationFn: ({ sessionId, categoryId }) => apiClient.setSessionCategory(sessionId, categoryId),
    onSuccess: (data, variables, context, mutation) => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.categories });
      queryClient.invalidateQueries({ queryKey: ['chat-sessions'] });
      options?.onSuccess?.(data, variables, context, mutation);
    },
    ...options,
  });
}

// Export as namespace for cleaner usage
export const useChatCategory = {
  useChatCategories,
  useCreateCategory,
  useUpdateCategory,
  useDeleteCategory,
  useSetSessionCategory,
};
