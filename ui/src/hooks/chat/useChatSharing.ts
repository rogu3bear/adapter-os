import { useQuery, useMutation, UseQueryOptions, UseMutationOptions } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import type {
  SessionShare,
  ShareSessionRequest,
  ShareSessionResponse,
  ChatSession,
  ChatSessionWithStatus,
} from '@/api/chat-types';

const QUERY_KEYS = {
  sessionShares: (sessionId: string) => ['chat', 'sessions', sessionId, 'shares'] as const,
  sessionsSharedWithMe: ['chat', 'sessions', 'shared-with-me'] as const,
};

// Session Shares Hooks

/**
 * Get all shares for a specific chat session
 *
 * @param sessionId - The session ID to get shares for
 * @param options - Additional query options
 * @returns Query result with array of session shares
 */
export function useSessionShares(
  sessionId: string,
  options?: Omit<UseQueryOptions<SessionShare[], Error>, 'queryKey' | 'queryFn'>
) {
  return useQuery({
    queryKey: QUERY_KEYS.sessionShares(sessionId),
    queryFn: (): Promise<SessionShare[]> => apiClient.getSessionShares(sessionId),
    enabled: !!sessionId,
    ...options,
  });
}

/**
 * Get all sessions shared with the current user
 *
 * @param options - Additional query options
 * @returns Query result with array of shared sessions
 */
export function useSessionsSharedWithMe(
  options?: Omit<UseQueryOptions<ChatSessionWithStatus[], Error>, 'queryKey' | 'queryFn'>
) {
  return useQuery({
    queryKey: QUERY_KEYS.sessionsSharedWithMe,
    queryFn: (): Promise<ChatSessionWithStatus[]> => apiClient.getSessionsSharedWithMe(),
    ...options,
  });
}

/**
 * Share a chat session with users or workspace
 *
 * @param options - Mutation options including onSuccess, onError callbacks
 * @returns Mutation object with mutate function
 *
 * @example
 * const shareSession = useShareSession({
 *   onSuccess: (response) => {
 *     console.log('Session shared successfully', response);
 *   },
 *   onError: (error) => {
 *     console.error('Failed to share session', error);
 *   },
 * });
 *
 * shareSession.mutate({
 *   sessionId: 'session-123',
 *   request: {
 *     user_ids: ['user-1', 'user-2'],
 *     permission: 'view',
 *   },
 * });
 */
export function useShareSession(
  options?: UseMutationOptions<
    ShareSessionResponse,
    Error,
    { sessionId: string; request: ShareSessionRequest }
  >
) {
  return useMutation<
    ShareSessionResponse,
    Error,
    { sessionId: string; request: ShareSessionRequest }
  >({
    mutationFn: ({ sessionId, request }) => apiClient.shareSession(sessionId, request),
    ...options,
  });
}

/**
 * Revoke a session share
 *
 * @param options - Mutation options including onSuccess, onError callbacks
 * @returns Mutation object with mutate function
 *
 * @example
 * const revokeShare = useRevokeShare({
 *   onSuccess: () => {
 *     console.log('Share revoked successfully');
 *   },
 * });
 *
 * revokeShare.mutate({
 *   sessionId: 'session-123',
 *   shareId: 'share-456',
 * });
 */
export function useRevokeShare(
  options?: UseMutationOptions<void, Error, { sessionId: string; shareId: string }>
) {
  return useMutation<void, Error, { sessionId: string; shareId: string }>({
    mutationFn: ({ sessionId, shareId }) => apiClient.revokeSessionShare(sessionId, shareId),
    ...options,
  });
}

// Export as namespace for cleaner usage
export const useChatSharing = {
  useSessionShares,
  useSessionsSharedWithMe,
  useShareSession,
  useRevokeShare,
};
