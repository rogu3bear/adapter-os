import { useQuery, useMutation, UseQueryOptions, UseMutationOptions } from '@tanstack/react-query';
import apiClient from '@/api/client';
import type {
  ChatTag,
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
  return useQuery<ChatTag[], Error>({
    queryKey: QUERY_KEYS.chatTags,
    queryFn: () => apiClient.listChatTags(),
    ...options,
  });
}

/**
 * Create a new chat tag
 */
export function useCreateTag(
  options?: UseMutationOptions<ChatTag, Error, CreateTagRequest>
) {
  return useMutation<ChatTag, Error, CreateTagRequest>({
    mutationFn: (request) => apiClient.createChatTag(request),
    ...options,
  });
}

/**
 * Update an existing chat tag
 */
export function useUpdateTag(
  options?: UseMutationOptions<ChatTag, Error, { tagId: string; request: UpdateTagRequest }>
) {
  return useMutation<ChatTag, Error, { tagId: string; request: UpdateTagRequest }>({
    mutationFn: ({ tagId, request }) => apiClient.updateChatTag(tagId, request),
    ...options,
  });
}

/**
 * Delete a chat tag
 */
export function useDeleteTag(
  options?: UseMutationOptions<void, Error, string>
) {
  return useMutation<void, Error, string>({
    mutationFn: (tagId) => apiClient.deleteChatTag(tagId),
    ...options,
  });
}

/**
 * Get tags for a specific chat session
 */
export function useSessionTags(
  sessionId: string,
  options?: Omit<UseQueryOptions<ChatTag[], Error>, 'queryKey' | 'queryFn'>
) {
  return useQuery<ChatTag[], Error>({
    queryKey: QUERY_KEYS.sessionTags(sessionId),
    queryFn: () => apiClient.getSessionTags(sessionId),
    enabled: !!sessionId,
    ...options,
  });
}

/**
 * Assign tags to a chat session
 */
export function useAssignTagsToSession(
  options?: UseMutationOptions<void, Error, { sessionId: string; request: AssignTagsRequest }>
) {
  return useMutation<void, Error, { sessionId: string; request: AssignTagsRequest }>({
    mutationFn: ({ sessionId, request }) => apiClient.assignTagsToSession(sessionId, request),
    ...options,
  });
}

/**
 * Remove a tag from a chat session
 */
export function useRemoveTagFromSession(
  options?: UseMutationOptions<void, Error, { sessionId: string; tagId: string }>
) {
  return useMutation<void, Error, { sessionId: string; tagId: string }>({
    mutationFn: ({ sessionId, tagId }) => apiClient.removeTagFromSession(sessionId, tagId),
    ...options,
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
