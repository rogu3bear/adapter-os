import { useQuery, useMutation, useQueryClient, UseQueryOptions, UseMutationOptions } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import type { ChatCategory } from '@/api/services/chat';
import type {
  CreateCategoryRequest,
  UpdateCategoryRequest,
} from '@/api/chat-types';
import { useTenant } from '@/providers/FeatureProviders';
import { withTenantKey } from '@/utils/tenant';

const createQueryKeys = (tenantId?: string | null) => ({
  categories: withTenantKey(['chat', 'categories'], tenantId),
  category: (id: string) => withTenantKey(['chat', 'categories', id], tenantId),
});

// Categories Hooks

/**
 * List all chat categories for the current tenant
 *
 * Returns categories in tree-sorted order by path (hierarchical)
 */
export function useChatCategories(
  options?: Omit<UseQueryOptions<ChatCategory[], Error>, 'queryKey' | 'queryFn'>
) {
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

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
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

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
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

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
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useMutation<void, Error, string>({
    mutationFn: (categoryId) => apiClient.deleteChatCategory(categoryId),
    onSuccess: (data, variables, context, mutation) => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.categories });
      queryClient.invalidateQueries({ queryKey: withTenantKey(['chat-sessions'], selectedTenant) });
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
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useMutation<void, Error, { sessionId: string; categoryId: string | null }>({
    mutationFn: ({ sessionId, categoryId }) => apiClient.setSessionCategory(sessionId, categoryId),
    onSuccess: (data, variables, context, mutation) => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.categories });
      queryClient.invalidateQueries({ queryKey: withTenantKey(['chat-sessions'], selectedTenant) });
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
