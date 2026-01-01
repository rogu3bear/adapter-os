import { useQuery, useMutation, useQueryClient, UseQueryOptions, UseMutationOptions } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import type { ChatTag } from '@/api/services/chat';
import type {
  CreateTagRequest,
  UpdateTagRequest,
  AssignTagsRequest,
} from '@/api/chat-types';

const QUERY_KEYS = {
  chatTags: ['chat', 'tags'] as const,
  chatTag: (id: string) => ['chat', 'tags', id] as const,
  sessionTags: (sessionId: string) => ['chat', 'sessions', sessionId, 'tags'] as const,
};

// Chat Tags Hooks

/**
 * List all chat tags for the current tenant
 */
export function useChatTags(
  options?: Omit<UseQueryOptions<ChatTag[], Error>, 'queryKey' | 'queryFn'>
) {
  return useQuery({
    queryKey: QUERY_KEYS.chatTags,
    queryFn: (): Promise<ChatTag[]> => apiClient.listChatTags(),
    meta: {
      errorMessage: 'Failed to load chat tags',
    },
    ...options,
  });
}

/**
 * Create a new chat tag
 */
export function useCreateTag(
  options?: UseMutationOptions<ChatTag, Error, CreateTagRequest>
) {
  const queryClient = useQueryClient();
  return useMutation<ChatTag, Error, CreateTagRequest>({
    ...options,
    mutationFn: (request) => apiClient.createChatTag(request),
    onSuccess: (data, variables, context, mutation) => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.chatTags });
      options?.onSuccess?.(data, variables, context, mutation);
    },
  });
}

/**
 * Update an existing chat tag
 */
export function useUpdateTag(
  options?: UseMutationOptions<ChatTag, Error, { tagId: string; request: UpdateTagRequest }>
) {
  const queryClient = useQueryClient();
  return useMutation<ChatTag, Error, { tagId: string; request: UpdateTagRequest }>({
    ...options,
    mutationFn: ({ tagId, request }) => apiClient.updateChatTag(tagId, request),
    onSuccess: (data, variables, context, mutation) => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.chatTags });
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.chatTag(variables.tagId) });
      options?.onSuccess?.(data, variables, context, mutation);
    },
  });
}

/**
 * Delete a chat tag
 */
export function useDeleteTag(
  options?: UseMutationOptions<void, Error, string>
) {
  const queryClient = useQueryClient();
  return useMutation<void, Error, string>({
    ...options,
    mutationFn: (tagId) => apiClient.deleteChatTag(tagId),
    onSuccess: (data, variables, context, mutation) => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.chatTags });
      options?.onSuccess?.(data, variables, context, mutation);
    },
  });
}

/**
 * Get tags for a specific chat session
 */
export function useSessionTags(
  sessionId: string,
  options?: Omit<UseQueryOptions<ChatTag[], Error>, 'queryKey' | 'queryFn'>
) {
  return useQuery({
    queryKey: QUERY_KEYS.sessionTags(sessionId),
    queryFn: (): Promise<ChatTag[]> => apiClient.getSessionTags(sessionId),
    enabled: !!sessionId,
    meta: {
      errorMessage: 'Failed to load session tags',
    },
    ...options,
  });
}

/**
 * Assign tags to a chat session
 */
export function useAssignTagsToSession(
  options?: UseMutationOptions<ChatTag[], Error, { sessionId: string; tagIds: string[] }>
) {
  const queryClient = useQueryClient();
  return useMutation<ChatTag[], Error, { sessionId: string; tagIds: string[] }>({
    ...options,
    mutationFn: ({ sessionId, tagIds }) => apiClient.assignTagsToSession(sessionId, tagIds),
    onSuccess: (data, variables, context, mutation) => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.sessionTags(variables.sessionId) });
      options?.onSuccess?.(data, variables, context, mutation);
    },
  });
}

/**
 * Remove a tag from a chat session
 */
export function useRemoveTagFromSession(
  options?: UseMutationOptions<void, Error, { sessionId: string; tagId: string }>
) {
  const queryClient = useQueryClient();
  return useMutation<void, Error, { sessionId: string; tagId: string }>({
    ...options,
    mutationFn: ({ sessionId, tagId }) => apiClient.removeTagFromSession(sessionId, tagId),
    onSuccess: (data, variables, context, mutation) => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.sessionTags(variables.sessionId) });
      options?.onSuccess?.(data, variables, context, mutation);
    },
  });
}

// Export as namespace for cleaner usage
export const useChatTagsNamespace = {
  useChatTags,
  useCreateTag,
  useUpdateTag,
  useDeleteTag,
  useSessionTags,
  useAssignTagsToSession,
  useRemoveTagFromSession,
};
