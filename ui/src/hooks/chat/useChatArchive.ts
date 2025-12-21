import { useQuery, useMutation, useQueryClient, UseQueryOptions, UseMutationOptions } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import type {
  ChatSessionWithStatus,
  ListArchivedQuery,
} from '@/api/chat-types';
import { toast } from 'sonner';
import { useTenant } from '@/providers/FeatureProviders';
import { withTenantKey } from '@/utils/tenant';

const createQueryKeys = (tenantId?: string | null) => ({
  archivedSessions: withTenantKey(['chat', 'sessions', 'archived'], tenantId),
  deletedSessions: withTenantKey(['chat', 'sessions', 'trash'], tenantId),
});

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
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useQuery({
    queryKey: [...QUERY_KEYS.archivedSessions, params],
    queryFn: (): Promise<ChatSessionWithStatus[]> => apiClient.listArchivedChatSessions(params?.limit),
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
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useQuery({
    queryKey: [...QUERY_KEYS.deletedSessions, params],
    queryFn: (): Promise<ChatSessionWithStatus[]> => apiClient.listDeletedChatSessions(params?.limit),
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
  const queryClient = useQueryClient();
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useMutation<void, Error, { sessionId: string; reason?: string }>({
    ...options,
    mutationFn: ({ sessionId, reason }) => apiClient.archiveChatSession(sessionId, reason),
    onSuccess: (data, variables, onMutateResult, mutationContext) => {
      // Invalidate archived and trash lists
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.archivedSessions });
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.deletedSessions });
      // Invalidate main sessions list with tenant scoping
      queryClient.invalidateQueries({ queryKey: withTenantKey(['chat-sessions'], selectedTenant) });
      toast.success('Session archived successfully');

      // Call user-provided onSuccess if exists
      options?.onSuccess?.(data, variables, onMutateResult, mutationContext);
    },
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
  const queryClient = useQueryClient();
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useMutation<void, Error, string>({
    ...options,
    mutationFn: (sessionId) => apiClient.restoreChatSession(sessionId),
    onSuccess: (data, variables, onMutateResult, mutationContext) => {
      // Invalidate archived and trash lists
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.archivedSessions });
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.deletedSessions });
      // Invalidate main sessions list with tenant scoping
      queryClient.invalidateQueries({ queryKey: withTenantKey(['chat-sessions'], selectedTenant) });
      toast.success('Session restored successfully');

      // Call user-provided onSuccess if exists
      options?.onSuccess?.(data, variables, onMutateResult, mutationContext);
    },
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
  const queryClient = useQueryClient();
  const { selectedTenant } = useTenant();
  const QUERY_KEYS = createQueryKeys(selectedTenant);

  return useMutation<void, Error, string>({
    ...options,
    mutationFn: (sessionId) => apiClient.hardDeleteChatSession(sessionId),
    onSuccess: (data, variables, onMutateResult, mutationContext) => {
      // Invalidate archived and trash lists
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.archivedSessions });
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.deletedSessions });
      // Invalidate main sessions list with tenant scoping
      queryClient.invalidateQueries({ queryKey: withTenantKey(['chat-sessions'], selectedTenant) });
      toast.success('Session permanently deleted');

      // Call user-provided onSuccess if exists
      options?.onSuccess?.(data, variables, onMutateResult, mutationContext);
    },
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
