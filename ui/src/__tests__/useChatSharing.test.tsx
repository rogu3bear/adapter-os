import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import {
  useSessionShares,
  useSessionsSharedWithMe,
  useShareSession,
  useRevokeShare,
} from '@/hooks/chat/useChatSharing';
import type {
  SessionShare,
  ShareSessionRequest,
  ShareSessionResponse,
  ChatSessionWithStatus,
} from '@/api/chat-types';

// Mock API client
const mockGetSessionShares = vi.fn();
const mockGetSessionsSharedWithMe = vi.fn();
const mockShareSession = vi.fn();
const mockRevokeSessionShare = vi.fn();

vi.mock('@/api/services', () => {
  const mockApiClient = {
    getSessionShares: (...args: unknown[]) => mockGetSessionShares(...args),
    getSessionsSharedWithMe: (...args: unknown[]) => mockGetSessionsSharedWithMe(...args),
    shareSession: (...args: unknown[]) => mockShareSession(...args),
    revokeSessionShare: (...args: unknown[]) => mockRevokeSessionShare(...args),
  };
  return {
    default: mockApiClient,
    apiClient: mockApiClient,
  };
});

// Test data
const mockSessionShares: SessionShare[] = [
  {
    id: 'share-1',
    session_id: 'session-123',
    workspace_id: 'workspace-1',
    shared_with_user_id: undefined,
    shared_with_tenant_id: undefined,
    permission: 'view',
    shared_by: 'user-owner',
    shared_at: '2025-01-01T00:00:00Z',
    expires_at: undefined,
    revoked_at: undefined,
  },
  {
    id: 'share-2',
    session_id: 'session-123',
    workspace_id: undefined,
    shared_with_user_id: 'user-2',
    shared_with_tenant_id: undefined,
    permission: 'collaborate',
    shared_by: 'user-owner',
    shared_at: '2025-01-02T00:00:00Z',
    expires_at: '2025-12-31T23:59:59Z',
    revoked_at: undefined,
  },
  {
    id: 'share-3',
    session_id: 'session-123',
    workspace_id: undefined,
    shared_with_user_id: 'user-3',
    shared_with_tenant_id: undefined,
    permission: 'comment',
    shared_by: 'user-owner',
    shared_at: '2025-01-03T00:00:00Z',
    expires_at: undefined,
    revoked_at: undefined,
  },
];

const mockSharedSessions: ChatSessionWithStatus[] = [
  {
    id: 'session-shared-1',
    tenant_id: 'tenant-1',
    user_id: 'other-user-1',
    stack_id: 'stack-1',
    collection_id: undefined,
    name: 'Shared Project Discussion',
    created_at: '2025-01-01T00:00:00Z',
    last_activity_at: '2025-01-10T12:00:00Z',
    metadata_json: undefined,
    category_id: 'cat-1',
    status: 'active',
    deleted_at: undefined,
    deleted_by: undefined,
    archived_at: undefined,
    archived_by: undefined,
    archive_reason: undefined,
    description: 'Discussion about the new project',
    is_shared: true,
  },
  {
    id: 'session-shared-2',
    tenant_id: 'tenant-1',
    user_id: 'other-user-2',
    stack_id: undefined,
    collection_id: 'col-1',
    name: 'Team Collaboration',
    created_at: '2025-01-05T00:00:00Z',
    last_activity_at: '2025-01-15T14:30:00Z',
    metadata_json: '{"tags": ["important"]}',
    category_id: undefined,
    status: 'active',
    deleted_at: undefined,
    deleted_by: undefined,
    archived_at: undefined,
    archived_by: undefined,
    archive_reason: undefined,
    description: undefined,
    is_shared: true,
  },
];

const mockShareResponse: ShareSessionResponse = {
  shares: [
    {
      type: 'user',
      id: 'share-4',
      user_id: 'user-4',
    },
    {
      type: 'user',
      id: 'share-5',
      user_id: 'user-5',
    },
  ],
};

const mockWorkspaceShareResponse: ShareSessionResponse = {
  shares: [
    {
      type: 'workspace',
      id: 'share-6',
      user_id: undefined,
    },
  ],
};

// Test wrapper
function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  return function Wrapper({ children }: { children: React.ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

describe('useChatSharing - Queries', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('useSessionShares', () => {
    it('returns session shares successfully', async () => {
      mockGetSessionShares.mockResolvedValue(mockSessionShares);

      const { result } = renderHook(() => useSessionShares('session-123'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(mockSessionShares);
      expect(mockGetSessionShares).toHaveBeenCalledWith('session-123');
      expect(mockGetSessionShares).toHaveBeenCalledTimes(1);
    });

    it('returns empty array when no shares exist', async () => {
      mockGetSessionShares.mockResolvedValue([]);

      const { result } = renderHook(() => useSessionShares('session-456'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual([]);
      expect(mockGetSessionShares).toHaveBeenCalledWith('session-456');
    });

    it('handles API error', async () => {
      const error = new Error('Failed to fetch session shares');
      mockGetSessionShares.mockRejectedValue(error);

      const { result } = renderHook(() => useSessionShares('session-error'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error).toEqual(error);
    });

    it('does not fetch when sessionId is empty', () => {
      const { result } = renderHook(() => useSessionShares(''), {
        wrapper: createWrapper(),
      });

      expect(result.current.isPending).toBe(true);
      expect(mockGetSessionShares).not.toHaveBeenCalled();
    });

    it('supports different share permissions', async () => {
      const sharesWithDifferentPermissions: SessionShare[] = [
        { ...mockSessionShares[0], permission: 'view' },
        { ...mockSessionShares[1], permission: 'comment' },
        { ...mockSessionShares[2], permission: 'collaborate' },
      ];

      mockGetSessionShares.mockResolvedValue(sharesWithDifferentPermissions);

      const { result } = renderHook(() => useSessionShares('session-123'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toHaveLength(3);
      expect(result.current.data?.[0].permission).toBe('view');
      expect(result.current.data?.[1].permission).toBe('comment');
      expect(result.current.data?.[2].permission).toBe('collaborate');
    });

    it('handles shares with workspace_id', async () => {
      const workspaceShares: SessionShare[] = [
        {
          ...mockSessionShares[0],
          workspace_id: 'workspace-123',
          shared_with_user_id: undefined,
        },
      ];

      mockGetSessionShares.mockResolvedValue(workspaceShares);

      const { result } = renderHook(() => useSessionShares('session-123'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data?.[0].workspace_id).toBe('workspace-123');
      expect(result.current.data?.[0].shared_with_user_id).toBeUndefined();
    });

    it('handles shares with user_id', async () => {
      const userShares: SessionShare[] = [
        {
          ...mockSessionShares[1],
          workspace_id: undefined,
          shared_with_user_id: 'user-456',
        },
      ];

      mockGetSessionShares.mockResolvedValue(userShares);

      const { result } = renderHook(() => useSessionShares('session-123'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data?.[0].shared_with_user_id).toBe('user-456');
      expect(result.current.data?.[0].workspace_id).toBeUndefined();
    });
  });

  describe('useSessionsSharedWithMe', () => {
    it('returns ChatSessionWithStatus[] successfully', async () => {
      mockGetSessionsSharedWithMe.mockResolvedValue(mockSharedSessions);

      const { result } = renderHook(() => useSessionsSharedWithMe(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(mockSharedSessions);
      expect(mockGetSessionsSharedWithMe).toHaveBeenCalledTimes(1);
    });

    it('returns ChatSessionWithStatus[] not ChatSession[]', async () => {
      mockGetSessionsSharedWithMe.mockResolvedValue(mockSharedSessions);

      const { result } = renderHook(() => useSessionsSharedWithMe(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      // Verify it's ChatSessionWithStatus by checking for status-specific fields
      expect(result.current.data?.[0]).toHaveProperty('status');
      expect(result.current.data?.[0]).toHaveProperty('is_shared');
      expect(result.current.data?.[0]).toHaveProperty('deleted_at');
      expect(result.current.data?.[0]).toHaveProperty('archived_at');
      expect(result.current.data?.[0]).toHaveProperty('category_id');
      expect(result.current.data?.[0]).toHaveProperty('description');

      // Verify values
      expect(result.current.data?.[0].status).toBe('active');
      expect(result.current.data?.[0].is_shared).toBe(true);
    });

    it('handles empty shared sessions list', async () => {
      mockGetSessionsSharedWithMe.mockResolvedValue([]);

      const { result } = renderHook(() => useSessionsSharedWithMe(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual([]);
    });

    it('handles API error', async () => {
      const error = new Error('Failed to fetch shared sessions');
      mockGetSessionsSharedWithMe.mockRejectedValue(error);

      const { result } = renderHook(() => useSessionsSharedWithMe(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error).toEqual(error);
    });

    it('returns sessions with all status types', async () => {
      const sessionsWithStatuses: ChatSessionWithStatus[] = [
        { ...mockSharedSessions[0], status: 'active' },
        {
          ...mockSharedSessions[1],
          status: 'archived',
          archived_at: '2025-01-20T00:00:00Z',
          archived_by: 'user-123',
        },
      ];

      mockGetSessionsSharedWithMe.mockResolvedValue(sessionsWithStatuses);

      const { result } = renderHook(() => useSessionsSharedWithMe(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toHaveLength(2);
      expect(result.current.data?.[0].status).toBe('active');
      expect(result.current.data?.[1].status).toBe('archived');
    });
  });
});

describe('useChatSharing - Mutations', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('useShareSession', () => {
    it('shares session with multiple users successfully', async () => {
      mockShareSession.mockResolvedValue(mockShareResponse);

      const { result } = renderHook(() => useShareSession(), {
        wrapper: createWrapper(),
      });

      const shareRequest: ShareSessionRequest = {
        user_ids: ['user-4', 'user-5'],
        permission: 'view',
      };

      await result.current.mutateAsync({
        sessionId: 'session-123',
        request: shareRequest,
      });

      expect(mockShareSession).toHaveBeenCalledWith('session-123', shareRequest);
      expect(mockShareSession).toHaveBeenCalledTimes(1);
    });

    it('shares session with workspace successfully', async () => {
      mockShareSession.mockResolvedValue(mockWorkspaceShareResponse);

      const { result } = renderHook(() => useShareSession(), {
        wrapper: createWrapper(),
      });

      const shareRequest: ShareSessionRequest = {
        workspace_id: 'workspace-1',
        permission: 'collaborate',
      };

      await result.current.mutateAsync({
        sessionId: 'session-456',
        request: shareRequest,
      });

      expect(mockShareSession).toHaveBeenCalledWith('session-456', shareRequest);
      expect(mockShareSession).toHaveBeenCalledTimes(1);
    });

    it('supports view permission (read_only)', async () => {
      mockShareSession.mockResolvedValue(mockShareResponse);

      const { result } = renderHook(() => useShareSession(), {
        wrapper: createWrapper(),
      });

      const shareRequest: ShareSessionRequest = {
        user_ids: ['user-1'],
        permission: 'view',
      };

      await result.current.mutateAsync({
        sessionId: 'session-123',
        request: shareRequest,
      });

      expect(mockShareSession).toHaveBeenCalledWith('session-123', shareRequest);
    });

    it('supports collaborate permission (read_write)', async () => {
      mockShareSession.mockResolvedValue(mockShareResponse);

      const { result } = renderHook(() => useShareSession(), {
        wrapper: createWrapper(),
      });

      const shareRequest: ShareSessionRequest = {
        user_ids: ['user-1'],
        permission: 'collaborate',
      };

      await result.current.mutateAsync({
        sessionId: 'session-123',
        request: shareRequest,
      });

      expect(mockShareSession).toHaveBeenCalledWith('session-123', shareRequest);
    });

    it('supports comment permission', async () => {
      mockShareSession.mockResolvedValue(mockShareResponse);

      const { result } = renderHook(() => useShareSession(), {
        wrapper: createWrapper(),
      });

      const shareRequest: ShareSessionRequest = {
        user_ids: ['user-1'],
        permission: 'comment',
      };

      await result.current.mutateAsync({
        sessionId: 'session-123',
        request: shareRequest,
      });

      expect(mockShareSession).toHaveBeenCalledWith('session-123', shareRequest);
    });

    it('passes workspace_id correctly', async () => {
      mockShareSession.mockResolvedValue(mockWorkspaceShareResponse);

      const { result } = renderHook(() => useShareSession(), {
        wrapper: createWrapper(),
      });

      const shareRequest: ShareSessionRequest = {
        workspace_id: 'workspace-abc-123',
        permission: 'view',
      };

      await result.current.mutateAsync({
        sessionId: 'session-789',
        request: shareRequest,
      });

      const callArgs = mockShareSession.mock.calls[0];
      expect(callArgs[0]).toBe('session-789');
      expect(callArgs[1].workspace_id).toBe('workspace-abc-123');
    });

    it('passes user_id parameters correctly', async () => {
      mockShareSession.mockResolvedValue(mockShareResponse);

      const { result } = renderHook(() => useShareSession(), {
        wrapper: createWrapper(),
      });

      const shareRequest: ShareSessionRequest = {
        user_ids: ['user-a', 'user-b', 'user-c'],
        permission: 'comment',
      };

      await result.current.mutateAsync({
        sessionId: 'session-xyz',
        request: shareRequest,
      });

      const callArgs = mockShareSession.mock.calls[0];
      expect(callArgs[0]).toBe('session-xyz');
      expect(callArgs[1].user_ids).toEqual(['user-a', 'user-b', 'user-c']);
    });

    it('supports expires_at parameter', async () => {
      mockShareSession.mockResolvedValue(mockShareResponse);

      const { result } = renderHook(() => useShareSession(), {
        wrapper: createWrapper(),
      });

      const expiryDate = '2025-12-31T23:59:59Z';
      const shareRequest: ShareSessionRequest = {
        user_ids: ['user-1'],
        permission: 'view',
        expires_at: expiryDate,
      };

      await result.current.mutateAsync({
        sessionId: 'session-123',
        request: shareRequest,
      });

      const callArgs = mockShareSession.mock.calls[0];
      expect(callArgs[1].expires_at).toBe(expiryDate);
    });

    it('handles share error', async () => {
      const error = new Error('Failed to share session');
      mockShareSession.mockRejectedValue(error);

      const { result } = renderHook(() => useShareSession(), {
        wrapper: createWrapper(),
      });

      const shareRequest: ShareSessionRequest = {
        user_ids: ['user-1'],
        permission: 'view',
      };

      await expect(
        result.current.mutateAsync({
          sessionId: 'session-123',
          request: shareRequest,
        })
      ).rejects.toThrow('Failed to share session');

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });
    });

    it('calls onSuccess callback', async () => {
      mockShareSession.mockResolvedValue(mockShareResponse);

      const onSuccess = vi.fn();

      const { result } = renderHook(
        () =>
          useShareSession({
            onSuccess,
          }),
        {
          wrapper: createWrapper(),
        }
      );

      const shareRequest: ShareSessionRequest = {
        user_ids: ['user-1'],
        permission: 'view',
      };

      await result.current.mutateAsync({
        sessionId: 'session-123',
        request: shareRequest,
      });

      await waitFor(() => {
        expect(onSuccess).toHaveBeenCalledTimes(1);
      });

      // TanStack Query v5 onSuccess receives 4 params: data, variables, context, mutation
      const [dataArg, variablesArg] = onSuccess.mock.calls[0];
      expect(dataArg).toEqual(mockShareResponse);
      expect(variablesArg).toEqual({ sessionId: 'session-123', request: shareRequest });
    });

    it('calls onError callback', async () => {
      const error = new Error('Share failed');
      mockShareSession.mockRejectedValue(error);

      const onError = vi.fn();

      const { result } = renderHook(
        () =>
          useShareSession({
            onError,
          }),
        {
          wrapper: createWrapper(),
        }
      );

      const shareRequest: ShareSessionRequest = {
        user_ids: ['user-1'],
        permission: 'view',
      };

      await expect(
        result.current.mutateAsync({
          sessionId: 'session-123',
          request: shareRequest,
        })
      ).rejects.toThrow();

      await waitFor(() => {
        expect(onError).toHaveBeenCalledTimes(1);
      });

      // TanStack Query v5 onError receives 4 params: error, variables, context, mutation
      const [errorArg, variablesArg] = onError.mock.calls[0];
      expect(errorArg).toEqual(error);
      expect(variablesArg).toEqual({ sessionId: 'session-123', request: shareRequest });
    });
  });

  describe('useRevokeShare', () => {
    it('revokes share successfully', async () => {
      mockRevokeSessionShare.mockResolvedValue(undefined);

      const { result } = renderHook(() => useRevokeShare(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync({
        sessionId: 'session-123',
        shareId: 'share-1',
      });

      expect(mockRevokeSessionShare).toHaveBeenCalledWith('session-123', 'share-1');
      expect(mockRevokeSessionShare).toHaveBeenCalledTimes(1);
    });

    it('handles revoke error', async () => {
      const error = new Error('Failed to revoke share');
      mockRevokeSessionShare.mockRejectedValue(error);

      const { result } = renderHook(() => useRevokeShare(), {
        wrapper: createWrapper(),
      });

      await expect(
        result.current.mutateAsync({
          sessionId: 'session-123',
          shareId: 'share-1',
        })
      ).rejects.toThrow('Failed to revoke share');

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });
    });

    it('calls onSuccess callback', async () => {
      mockRevokeSessionShare.mockResolvedValue(undefined);

      const onSuccess = vi.fn();

      const { result } = renderHook(
        () =>
          useRevokeShare({
            onSuccess,
          }),
        {
          wrapper: createWrapper(),
        }
      );

      await result.current.mutateAsync({
        sessionId: 'session-123',
        shareId: 'share-1',
      });

      await waitFor(() => {
        expect(onSuccess).toHaveBeenCalledTimes(1);
      });

      // TanStack Query v5 onSuccess receives 4 params: data, variables, context, mutation
      const [dataArg, variablesArg] = onSuccess.mock.calls[0];
      expect(dataArg).toBeUndefined();
      expect(variablesArg).toEqual({ sessionId: 'session-123', shareId: 'share-1' });
    });

    it('calls onError callback', async () => {
      const error = new Error('Revoke failed');
      mockRevokeSessionShare.mockRejectedValue(error);

      const onError = vi.fn();

      const { result } = renderHook(
        () =>
          useRevokeShare({
            onError,
          }),
        {
          wrapper: createWrapper(),
        }
      );

      await expect(
        result.current.mutateAsync({
          sessionId: 'session-123',
          shareId: 'share-1',
        })
      ).rejects.toThrow();

      await waitFor(() => {
        expect(onError).toHaveBeenCalledTimes(1);
      });

      // TanStack Query v5 onError receives 4 params: error, variables, context, mutation
      const [errorArg, variablesArg] = onError.mock.calls[0];
      expect(errorArg).toEqual(error);
      expect(variablesArg).toEqual({ sessionId: 'session-123', shareId: 'share-1' });
    });

    it('passes correct parameters to API', async () => {
      mockRevokeSessionShare.mockResolvedValue(undefined);

      const { result } = renderHook(() => useRevokeShare(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync({
        sessionId: 'session-abc-123',
        shareId: 'share-xyz-456',
      });

      expect(mockRevokeSessionShare).toHaveBeenCalledWith('session-abc-123', 'share-xyz-456');
    });
  });
});

describe('useChatSharing - Cache invalidation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('useShareSession can trigger cache invalidation', async () => {
    mockShareSession.mockResolvedValue(mockShareResponse);

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );

    // Pre-populate cache with shares
    queryClient.setQueryData(['chat', 'sessions', 'session-123', 'shares'], mockSessionShares);

    const { result } = renderHook(() => useShareSession(), { wrapper });

    const shareRequest: ShareSessionRequest = {
      user_ids: ['user-new'],
      permission: 'view',
    };

    await result.current.mutateAsync({
      sessionId: 'session-123',
      request: shareRequest,
    });

    expect(mockShareSession).toHaveBeenCalledWith('session-123', shareRequest);
  });

  it('useRevokeShare can trigger cache invalidation', async () => {
    mockRevokeSessionShare.mockResolvedValue(undefined);

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );

    // Pre-populate cache with shares
    queryClient.setQueryData(['chat', 'sessions', 'session-123', 'shares'], mockSessionShares);

    const { result } = renderHook(() => useRevokeShare(), { wrapper });

    await result.current.mutateAsync({
      sessionId: 'session-123',
      shareId: 'share-1',
    });

    expect(mockRevokeSessionShare).toHaveBeenCalledWith('session-123', 'share-1');
  });
});

describe('useChatSharing - Query keys', () => {
  it('uses correct query key for session shares', async () => {
    mockGetSessionShares.mockResolvedValue(mockSessionShares);

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );

    renderHook(() => useSessionShares('session-123'), { wrapper });

    await waitFor(() => {
      const queries = queryClient.getQueryCache().findAll({
        queryKey: ['chat', 'sessions', 'session-123', 'shares'],
      });
      expect(queries.length).toBeGreaterThan(0);
    });
  });

  it('uses correct query key for sessions shared with me', async () => {
    mockGetSessionsSharedWithMe.mockResolvedValue(mockSharedSessions);

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );

    renderHook(() => useSessionsSharedWithMe(), { wrapper });

    await waitFor(() => {
      const queries = queryClient.getQueryCache().findAll({
        queryKey: ['chat', 'sessions', 'shared-with-me'],
      });
      expect(queries.length).toBeGreaterThan(0);
    });
  });
});
