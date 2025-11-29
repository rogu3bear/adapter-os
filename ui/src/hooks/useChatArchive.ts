import { useQuery, useMutation, UseQueryOptions, UseMutationOptions } from '@tanstack/react-query';
import apiClient from '@/api/client';
import type {
  ChatSessionWithStatus,
  ArchiveSessionRequest,
  ListArchivedQuery,
} from '@/api/chat-types';

const QUERY_KEYS = {
  archivedSessions: ['chat', 'sessions', 'archived'] as const,
  deletedSessions: ['chat', 'sessions', 'trash'] as const,
};

// Archive Hooks

/**
 * List archived chat sessions
 *
 * GET /v1/chat/sessions/archived
 *
 * @param params - Optional query parameters (limit)
 * @param options - TanStack Query options
 * @returns Query result with archived sessions
 */
export function useArchivedSessions(
  params?: ListArchivedQuery,
  options?: Omit<UseQueryOptions<ChatSessionWithStatus[], Error>, 'queryKey' | 'queryFn'>
) {
  return useQuery<ChatSessionWithStatus[], Error>({
    queryKey: [...QUERY_KEYS.archivedSessions, params],
    queryFn: () => apiClient.listArchivedChatSessions(params?.limit),
    ...options,
  });
}

/**
 * List soft-deleted chat sessions (trash)
 *
 * GET /v1/chat/sessions/trash
 *
 * @param params - Optional query parameters (limit)
 * @param options - TanStack Query options
 * @returns Query result with deleted sessions
 */
export function useDeletedSessions(
  params?: ListArchivedQuery,
  options?: Omit<UseQueryOptions<ChatSessionWithStatus[], Error>, 'queryKey' | 'queryFn'>
) {
  return useQuery<ChatSessionWithStatus[], Error>({
    queryKey: [...QUERY_KEYS.deletedSessions, params],
    queryFn: () => apiClient.listDeletedChatSessions(params?.limit),
    ...options,
  });
}

/**
 * Archive a chat session
 *
 * POST /v1/chat/sessions/:session_id/archive
 *
 * @param options - TanStack Mutation options
 * @returns Mutation for archiving a session
 */
export function useArchiveSession(
  options?: UseMutationOptions<void, Error, { sessionId: string; reason?: string }>
) {
  return useMutation<void, Error, { sessionId: string; reason?: string }>({
    mutationFn: ({ sessionId, reason }) => apiClient.archiveChatSession(sessionId, reason),
    ...options,
  });
}

/**
 * Restore an archived or deleted session
 *
 * POST /v1/chat/sessions/:session_id/restore
 *
 * Requires WorkspaceManage permission (admin-only)
 *
 * @param options - TanStack Mutation options
 * @returns Mutation for restoring a session
 */
export function useRestoreSession(
  options?: UseMutationOptions<void, Error, string>
) {
  return useMutation<void, Error, string>({
    mutationFn: (sessionId) => apiClient.restoreChatSession(sessionId),
    ...options,
  });
}

/**
 * Permanently delete a session
 *
 * DELETE /v1/chat/sessions/:session_id/permanent
 *
 * Requires WorkspaceManage permission (admin-only)
 *
 * @param options - TanStack Mutation options
 * @returns Mutation for permanently deleting a session
 */
export function useHardDeleteSession(
  options?: UseMutationOptions<void, Error, string>
) {
  return useMutation<void, Error, string>({
    mutationFn: (sessionId) => apiClient.hardDeleteSession(sessionId),
    ...options,
  });
}

// Export as namespace for cleaner usage
export const useChatArchive = {
  useArchivedSessions,
  useDeletedSessions,
  useArchiveSession,
  useRestoreSession,
  useHardDeleteSession,
};
