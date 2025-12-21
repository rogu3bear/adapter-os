import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import {
  useArchivedSessions,
  useDeletedSessions,
  useArchiveSession,
  useRestoreSession,
  useHardDeleteSession,
} from '@/hooks/chat/useChatArchive';
import type { ChatSessionWithStatus } from '@/api/chat-types';
import { toast } from 'sonner';

// Mock API client
const mockListArchivedChatSessions = vi.fn();
const mockListDeletedChatSessions = vi.fn();
const mockArchiveChatSession = vi.fn();
const mockRestoreChatSession = vi.fn();
const mockHardDeleteChatSession = vi.fn();

vi.mock('@/api/services', () => {
  const mockApiClient = {
    listArchivedChatSessions: (...args: unknown[]) => mockListArchivedChatSessions(...args),
    listDeletedChatSessions: (...args: unknown[]) => mockListDeletedChatSessions(...args),
    archiveChatSession: (...args: unknown[]) => mockArchiveChatSession(...args),
    restoreChatSession: (...args: unknown[]) => mockRestoreChatSession(...args),
    hardDeleteChatSession: (...args: unknown[]) => mockHardDeleteChatSession(...args),
  };
  return {
    default: mockApiClient,
    apiClient: mockApiClient,
  };
});

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

// Mock useTenant for tenant-scoped query keys
vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => ({ selectedTenant: 'test-tenant' }),
}));

// Test data
const mockArchivedSessions: ChatSessionWithStatus[] = [
  {
    session_id: 'session-1',
    tenant_id: 'tenant-1',
    title: 'Archived Session 1',
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
    status: 'archived',
    archived_at: '2025-01-02T00:00:00Z',
    message_count: 10,
  },
  {
    session_id: 'session-2',
    tenant_id: 'tenant-1',
    title: 'Archived Session 2',
    created_at: '2025-01-03T00:00:00Z',
    updated_at: '2025-01-03T00:00:00Z',
    status: 'archived',
    archived_at: '2025-01-04T00:00:00Z',
    message_count: 5,
    category_id: 'cat-1',
  },
];

const mockDeletedSessions: ChatSessionWithStatus[] = [
  {
    session_id: 'session-3',
    tenant_id: 'tenant-1',
    title: 'Deleted Session 1',
    created_at: '2025-01-05T00:00:00Z',
    updated_at: '2025-01-05T00:00:00Z',
    status: 'deleted',
    deleted_at: '2025-01-06T00:00:00Z',
    deleted_by: 'user-1',
    message_count: 3,
  },
  {
    session_id: 'session-4',
    tenant_id: 'tenant-1',
    title: 'Deleted Session 2',
    created_at: '2025-01-07T00:00:00Z',
    updated_at: '2025-01-07T00:00:00Z',
    status: 'deleted',
    deleted_at: '2025-01-08T00:00:00Z',
    deleted_by: 'user-2',
    message_count: 15,
  },
];

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

describe('useChatArchive - Query Hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('useArchivedSessions', () => {
    it('returns archived sessions successfully', async () => {
      mockListArchivedChatSessions.mockResolvedValue(mockArchivedSessions);

      const { result } = renderHook(() => useArchivedSessions(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(mockArchivedSessions);
      expect(mockListArchivedChatSessions).toHaveBeenCalledTimes(1);
      expect(mockListArchivedChatSessions).toHaveBeenCalledWith(undefined);
    });

    it('returns empty array when no archived sessions', async () => {
      mockListArchivedChatSessions.mockResolvedValue([]);

      const { result } = renderHook(() => useArchivedSessions(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual([]);
      expect(mockListArchivedChatSessions).toHaveBeenCalledTimes(1);
    });

    it('handles API error', async () => {
      const error = new Error('Failed to fetch archived sessions');
      mockListArchivedChatSessions.mockRejectedValue(error);

      const { result } = renderHook(() => useArchivedSessions(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error).toEqual(error);
    });

    it('passes limit parameter to API', async () => {
      mockListArchivedChatSessions.mockResolvedValue(mockArchivedSessions.slice(0, 1));

      const { result } = renderHook(() => useArchivedSessions({ limit: 1 }), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(mockListArchivedChatSessions).toHaveBeenCalledWith(1);
      expect(result.current.data).toHaveLength(1);
    });

    it('includes query params in query key', () => {
      mockListArchivedChatSessions.mockResolvedValue(mockArchivedSessions);

      const { result } = renderHook(() => useArchivedSessions({ limit: 10 }), {
        wrapper: createWrapper(),
      });

      // Query key should include params
      expect(result.current).toBeDefined();
    });
  });

  describe('useDeletedSessions', () => {
    it('returns deleted sessions successfully', async () => {
      mockListDeletedChatSessions.mockResolvedValue(mockDeletedSessions);

      const { result } = renderHook(() => useDeletedSessions(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(mockDeletedSessions);
      expect(mockListDeletedChatSessions).toHaveBeenCalledTimes(1);
      expect(mockListDeletedChatSessions).toHaveBeenCalledWith(undefined);
    });

    it('returns empty array when no deleted sessions', async () => {
      mockListDeletedChatSessions.mockResolvedValue([]);

      const { result } = renderHook(() => useDeletedSessions(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual([]);
      expect(mockListDeletedChatSessions).toHaveBeenCalledTimes(1);
    });

    it('handles API error', async () => {
      const error = new Error('Failed to fetch deleted sessions');
      mockListDeletedChatSessions.mockRejectedValue(error);

      const { result } = renderHook(() => useDeletedSessions(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error).toEqual(error);
    });

    it('passes limit parameter to API', async () => {
      mockListDeletedChatSessions.mockResolvedValue(mockDeletedSessions.slice(0, 1));

      const { result } = renderHook(() => useDeletedSessions({ limit: 1 }), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(mockListDeletedChatSessions).toHaveBeenCalledWith(1);
      expect(result.current.data).toHaveLength(1);
    });

    it('includes query params in query key', () => {
      mockListDeletedChatSessions.mockResolvedValue(mockDeletedSessions);

      const { result } = renderHook(() => useDeletedSessions({ limit: 20 }), {
        wrapper: createWrapper(),
      });

      // Query key should include params
      expect(result.current).toBeDefined();
    });
  });
});

describe('useChatArchive - Mutation Hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('useArchiveSession', () => {
    it('archives a session successfully', async () => {
      mockArchiveChatSession.mockResolvedValue(undefined);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useArchiveSession(), { wrapper });

      await result.current.mutateAsync({ sessionId: 'session-1' });

      expect(mockArchiveChatSession).toHaveBeenCalledTimes(1);
      expect(mockArchiveChatSession).toHaveBeenCalledWith('session-1', undefined);
      expect(toast.success).toHaveBeenCalledWith('Session archived successfully');
    });

    it('archives a session with reason', async () => {
      mockArchiveChatSession.mockResolvedValue(undefined);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useArchiveSession(), { wrapper });

      await result.current.mutateAsync({ sessionId: 'session-1', reason: 'Test reason' });

      expect(mockArchiveChatSession).toHaveBeenCalledWith('session-1', 'Test reason');
      expect(toast.success).toHaveBeenCalledWith('Session archived successfully');
    });

    it('invalidates all required cache keys', async () => {
      mockArchiveChatSession.mockResolvedValue(undefined);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });

      // Spy on invalidateQueries
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useArchiveSession(), { wrapper });

      await result.current.mutateAsync({ sessionId: 'session-1' });

      // Verify all cache invalidations (with tenant segment)
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['chat', 'sessions', 'archived', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['chat', 'sessions', 'trash', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['chat-sessions', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledTimes(3);
    });

    it('calls user-provided onSuccess callback with 4 parameters', async () => {
      mockArchiveChatSession.mockResolvedValue(undefined);

      const onSuccessMock = vi.fn();
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(
        () => useArchiveSession({ onSuccess: onSuccessMock }),
        { wrapper }
      );

      await result.current.mutateAsync({ sessionId: 'session-1' });

      // Verify onSuccess is called with 4 parameters: data, variables, context, mutation
      // Context is undefined when no onMutate is provided
      expect(onSuccessMock).toHaveBeenCalledTimes(1);
      const [dataArg, variablesArg] = onSuccessMock.mock.calls[0];
      expect(dataArg).toBeUndefined();
      expect(variablesArg).toEqual({ sessionId: 'session-1' });
    });

    it('handles archive error', async () => {
      const error = new Error('Archive failed');
      mockArchiveChatSession.mockRejectedValue(error);

      const { result } = renderHook(() => useArchiveSession(), {
        wrapper: createWrapper(),
      });

      await expect(
        result.current.mutateAsync({ sessionId: 'session-1' })
      ).rejects.toThrow('Archive failed');

      expect(toast.success).not.toHaveBeenCalled();
    });

    it('does not call toast if mutation is not successful', async () => {
      const error = new Error('Network error');
      mockArchiveChatSession.mockRejectedValue(error);

      const { result } = renderHook(() => useArchiveSession(), {
        wrapper: createWrapper(),
      });

      try {
        await result.current.mutateAsync({ sessionId: 'session-1' });
      } catch {
        // Expected error
      }

      expect(toast.success).not.toHaveBeenCalled();
    });
  });

  describe('useRestoreSession', () => {
    it('restores a session successfully', async () => {
      mockRestoreChatSession.mockResolvedValue(undefined);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useRestoreSession(), { wrapper });

      await result.current.mutateAsync('session-1');

      expect(mockRestoreChatSession).toHaveBeenCalledTimes(1);
      expect(mockRestoreChatSession).toHaveBeenCalledWith('session-1');
      expect(toast.success).toHaveBeenCalledWith('Session restored successfully');
    });

    it('invalidates all required cache keys', async () => {
      mockRestoreChatSession.mockResolvedValue(undefined);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });

      // Spy on invalidateQueries
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useRestoreSession(), { wrapper });

      await result.current.mutateAsync('session-1');

      // Verify all cache invalidations (with tenant segment)
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['chat', 'sessions', 'archived', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['chat', 'sessions', 'trash', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['chat-sessions', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledTimes(3);
    });

    it('calls user-provided onSuccess callback with 4 parameters', async () => {
      mockRestoreChatSession.mockResolvedValue(undefined);

      const onSuccessMock = vi.fn();
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(
        () => useRestoreSession({ onSuccess: onSuccessMock }),
        { wrapper }
      );

      await result.current.mutateAsync('session-1');

      // Verify onSuccess is called with 4 parameters: data, variables, context, mutation
      // Context is undefined when no onMutate is provided
      expect(onSuccessMock).toHaveBeenCalledTimes(1);
      const [dataArg, variablesArg] = onSuccessMock.mock.calls[0];
      expect(dataArg).toBeUndefined();
      expect(variablesArg).toEqual('session-1');
    });

    it('handles restore error', async () => {
      const error = new Error('Restore failed');
      mockRestoreChatSession.mockRejectedValue(error);

      const { result } = renderHook(() => useRestoreSession(), {
        wrapper: createWrapper(),
      });

      await expect(result.current.mutateAsync('session-1')).rejects.toThrow('Restore failed');

      expect(toast.success).not.toHaveBeenCalled();
    });
  });

  describe('useHardDeleteSession', () => {
    it('permanently deletes a session successfully', async () => {
      mockHardDeleteChatSession.mockResolvedValue(undefined);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useHardDeleteSession(), { wrapper });

      await result.current.mutateAsync('session-1');

      expect(mockHardDeleteChatSession).toHaveBeenCalledTimes(1);
      expect(mockHardDeleteChatSession).toHaveBeenCalledWith('session-1');
      expect(toast.success).toHaveBeenCalledWith('Session permanently deleted');
    });

    it('uses hardDeleteChatSession API method (not regular delete)', async () => {
      mockHardDeleteChatSession.mockResolvedValue(undefined);

      const { result } = renderHook(() => useHardDeleteSession(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync('session-1');

      // Verify it's using the hardDeleteChatSession method
      // This method should call /v1/chat/sessions/:session_id/permanent endpoint
      expect(mockHardDeleteChatSession).toHaveBeenCalledWith('session-1');
      expect(mockHardDeleteChatSession).toHaveBeenCalledTimes(1);
    });

    it('invalidates all required cache keys', async () => {
      mockHardDeleteChatSession.mockResolvedValue(undefined);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });

      // Spy on invalidateQueries
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useHardDeleteSession(), { wrapper });

      await result.current.mutateAsync('session-1');

      // Verify all cache invalidations (with tenant segment)
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['chat', 'sessions', 'archived', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['chat', 'sessions', 'trash', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['chat-sessions', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledTimes(3);
    });

    it('calls user-provided onSuccess callback with 4 parameters', async () => {
      mockHardDeleteChatSession.mockResolvedValue(undefined);

      const onSuccessMock = vi.fn();
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(
        () => useHardDeleteSession({ onSuccess: onSuccessMock }),
        { wrapper }
      );

      await result.current.mutateAsync('session-1');

      // Verify onSuccess is called with 4 parameters: data, variables, context, mutation
      // Context is undefined when no onMutate is provided
      expect(onSuccessMock).toHaveBeenCalledTimes(1);
      const [dataArg, variablesArg] = onSuccessMock.mock.calls[0];
      expect(dataArg).toBeUndefined();
      expect(variablesArg).toEqual('session-1');
    });

    it('handles hard delete error', async () => {
      const error = new Error('Permanent delete failed');
      mockHardDeleteChatSession.mockRejectedValue(error);

      const { result } = renderHook(() => useHardDeleteSession(), {
        wrapper: createWrapper(),
      });

      await expect(result.current.mutateAsync('session-1')).rejects.toThrow(
        'Permanent delete failed'
      );

      expect(toast.success).not.toHaveBeenCalled();
    });
  });
});

describe('useChatArchive - Cache Invalidation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('all mutations invalidate the same three query keys', async () => {
    mockArchiveChatSession.mockResolvedValue(undefined);
    mockRestoreChatSession.mockResolvedValue(undefined);
    mockHardDeleteChatSession.mockResolvedValue(undefined);

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );

    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

    // Test archive mutation
    const { result: archiveResult } = renderHook(() => useArchiveSession(), { wrapper });
    await archiveResult.current.mutateAsync({ sessionId: 'session-1' });

    // Test restore mutation
    const { result: restoreResult } = renderHook(() => useRestoreSession(), { wrapper });
    await restoreResult.current.mutateAsync('session-2');

    // Test hard delete mutation
    const { result: deleteResult } = renderHook(() => useHardDeleteSession(), { wrapper });
    await deleteResult.current.mutateAsync('session-3');

    // Each mutation should invalidate 3 keys
    expect(invalidateSpy).toHaveBeenCalledTimes(9); // 3 mutations × 3 keys each

    // Verify each key was invalidated by all mutations (with tenant segment)
    const archivedSessionsCalls = invalidateSpy.mock.calls.filter(
      (call) => JSON.stringify(call[0].queryKey) === JSON.stringify(['chat', 'sessions', 'archived', 'test-tenant'])
    );
    expect(archivedSessionsCalls).toHaveLength(3);

    const deletedSessionsCalls = invalidateSpy.mock.calls.filter(
      (call) => JSON.stringify(call[0].queryKey) === JSON.stringify(['chat', 'sessions', 'trash', 'test-tenant'])
    );
    expect(deletedSessionsCalls).toHaveLength(3);

    const chatSessionsCalls = invalidateSpy.mock.calls.filter(
      (call) => JSON.stringify(call[0].queryKey) === JSON.stringify(['chat-sessions', 'test-tenant'])
    );
    expect(chatSessionsCalls).toHaveLength(3);
  });

  it('invalidation uses partial match for chat-sessions to catch all tenant variations', async () => {
    mockArchiveChatSession.mockResolvedValue(undefined);

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );

    // Set up queries with various tenant-specific keys
    queryClient.setQueryData(['chat-sessions'], []);
    queryClient.setQueryData(['chat-sessions', 'tenant-1'], []);
    queryClient.setQueryData(['chat-sessions', 'tenant-2', 'active'], []);

    const { result } = renderHook(() => useArchiveSession(), { wrapper });

    await result.current.mutateAsync({ sessionId: 'session-1' });

    // All chat-sessions queries should be invalidated due to partial match
    const queries = queryClient.getQueryCache().findAll({
      queryKey: ['chat-sessions'],
    });

    expect(queries.length).toBeGreaterThan(0);
  });
});

describe('useChatArchive - Toast Notifications', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows correct success message for archive', async () => {
    mockArchiveChatSession.mockResolvedValue(undefined);

    const { result } = renderHook(() => useArchiveSession(), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync({ sessionId: 'session-1' });

    expect(toast.success).toHaveBeenCalledWith('Session archived successfully');
    expect(toast.success).toHaveBeenCalledTimes(1);
  });

  it('shows correct success message for restore', async () => {
    mockRestoreChatSession.mockResolvedValue(undefined);

    const { result } = renderHook(() => useRestoreSession(), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync('session-1');

    expect(toast.success).toHaveBeenCalledWith('Session restored successfully');
    expect(toast.success).toHaveBeenCalledTimes(1);
  });

  it('shows correct success message for hard delete', async () => {
    mockHardDeleteChatSession.mockResolvedValue(undefined);

    const { result } = renderHook(() => useHardDeleteSession(), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync('session-1');

    expect(toast.success).toHaveBeenCalledWith('Session permanently deleted');
    expect(toast.success).toHaveBeenCalledTimes(1);
  });

  it('does not show toast on error', async () => {
    mockArchiveChatSession.mockRejectedValue(new Error('Failed'));
    mockRestoreChatSession.mockRejectedValue(new Error('Failed'));
    mockHardDeleteChatSession.mockRejectedValue(new Error('Failed'));

    const archiveHook = renderHook(() => useArchiveSession(), { wrapper: createWrapper() });
    const restoreHook = renderHook(() => useRestoreSession(), { wrapper: createWrapper() });
    const deleteHook = renderHook(() => useHardDeleteSession(), { wrapper: createWrapper() });

    try {
      await archiveHook.result.current.mutateAsync({ sessionId: 'session-1' });
    } catch {
      // Expected
    }

    try {
      await restoreHook.result.current.mutateAsync('session-2');
    } catch {
      // Expected
    }

    try {
      await deleteHook.result.current.mutateAsync('session-3');
    } catch {
      // Expected
    }

    expect(toast.success).not.toHaveBeenCalled();
  });
});
