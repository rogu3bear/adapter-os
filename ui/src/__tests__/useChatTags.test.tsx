import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import {
  useChatTags,
  useCreateTag,
  useUpdateTag,
  useDeleteTag,
  useSessionTags,
  useAssignTagsToSession,
  useRemoveTagFromSession,
} from '@/hooks/useChatTags';
import type { ChatTag, CreateTagRequest, UpdateTagRequest } from '@/api/chat-types';

// Mock API client
const mockListChatTags = vi.fn();
const mockCreateChatTag = vi.fn();
const mockUpdateChatTag = vi.fn();
const mockDeleteChatTag = vi.fn();
const mockGetSessionTags = vi.fn();
const mockAssignTagsToSession = vi.fn();
const mockRemoveTagFromSession = vi.fn();

vi.mock('@/api/client', () => ({
  default: {
    listChatTags: (...args: unknown[]) => mockListChatTags(...args),
    createChatTag: (...args: unknown[]) => mockCreateChatTag(...args),
    updateChatTag: (...args: unknown[]) => mockUpdateChatTag(...args),
    deleteChatTag: (...args: unknown[]) => mockDeleteChatTag(...args),
    getSessionTags: (...args: unknown[]) => mockGetSessionTags(...args),
    assignTagsToSession: (...args: unknown[]) => mockAssignTagsToSession(...args),
    removeTagFromSession: (...args: unknown[]) => mockRemoveTagFromSession(...args),
  },
}));

// Test data
const mockTags: ChatTag[] = [
  {
    id: 'tag-1',
    tenant_id: 'tenant-1',
    name: 'Important',
    color: '#FF0000',
    description: 'Important conversations',
    created_at: '2025-01-01T00:00:00Z',
    created_by: 'user-1',
  },
  {
    id: 'tag-2',
    tenant_id: 'tenant-1',
    name: 'Work',
    color: '#0000FF',
    description: 'Work related',
    created_at: '2025-01-02T00:00:00Z',
    created_by: 'user-1',
  },
  {
    id: 'tag-3',
    tenant_id: 'tenant-1',
    name: 'Personal',
    color: '#00FF00',
    description: undefined,
    created_at: '2025-01-03T00:00:00Z',
    created_by: undefined,
  },
];

const mockSessionTags: ChatTag[] = [mockTags[0], mockTags[1]];

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

describe('useChatTags - Query Hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('useChatTags', () => {
    it('returns all tags successfully', async () => {
      mockListChatTags.mockResolvedValue(mockTags);

      const { result } = renderHook(() => useChatTags(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(mockTags);
      expect(result.current.data).toHaveLength(3);
      expect(mockListChatTags).toHaveBeenCalledTimes(1);
    });

    it('handles empty tag list', async () => {
      mockListChatTags.mockResolvedValue([]);

      const { result } = renderHook(() => useChatTags(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual([]);
    });

    it('handles API error', async () => {
      const error = new Error('Failed to fetch tags');
      mockListChatTags.mockRejectedValue(error);

      const { result } = renderHook(() => useChatTags(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error).toEqual(error);
    });

    it('respects custom query options', async () => {
      mockListChatTags.mockResolvedValue(mockTags);

      const { result } = renderHook(() => useChatTags({ enabled: false }), {
        wrapper: createWrapper(),
      });

      // Should not fetch when disabled
      expect(result.current.isPending).toBe(true);
      expect(mockListChatTags).not.toHaveBeenCalled();
    });
  });

  describe('useSessionTags', () => {
    it('returns tags for a specific session', async () => {
      mockGetSessionTags.mockResolvedValue(mockSessionTags);

      const { result } = renderHook(() => useSessionTags('session-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(mockSessionTags);
      expect(result.current.data).toHaveLength(2);
      expect(mockGetSessionTags).toHaveBeenCalledWith('session-1');
      expect(mockGetSessionTags).toHaveBeenCalledTimes(1);
    });

    it('does not fetch when sessionId is empty', () => {
      const { result } = renderHook(() => useSessionTags(''), {
        wrapper: createWrapper(),
      });

      expect(result.current.isPending).toBe(true);
      expect(mockGetSessionTags).not.toHaveBeenCalled();
    });

    it('handles session with no tags', async () => {
      mockGetSessionTags.mockResolvedValue([]);

      const { result } = renderHook(() => useSessionTags('session-empty'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual([]);
    });

    it('handles API error', async () => {
      const error = new Error('Session not found');
      mockGetSessionTags.mockRejectedValue(error);

      const { result } = renderHook(() => useSessionTags('nonexistent'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error).toEqual(error);
    });

    it('respects custom query options', async () => {
      mockGetSessionTags.mockResolvedValue(mockSessionTags);

      const { result } = renderHook(
        () => useSessionTags('session-1', { enabled: false }),
        {
          wrapper: createWrapper(),
        }
      );

      // Should not fetch when disabled
      expect(result.current.isPending).toBe(true);
      expect(mockGetSessionTags).not.toHaveBeenCalled();
    });
  });
});

describe('useChatTags - Mutation Hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('useCreateTag', () => {
    it('creates a tag successfully', async () => {
      const newTag: ChatTag = {
        id: 'tag-new',
        tenant_id: 'tenant-1',
        name: 'New Tag',
        color: '#FFFF00',
        description: 'A new tag',
        created_at: '2025-01-04T00:00:00Z',
        created_by: 'user-1',
      };
      const request: CreateTagRequest = {
        name: 'New Tag',
        color: '#FFFF00',
        description: 'A new tag',
      };

      mockCreateChatTag.mockResolvedValue(newTag);

      const { result } = renderHook(() => useCreateTag(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync(request);

      expect(mockCreateChatTag).toHaveBeenCalledWith(request);
      expect(mockCreateChatTag).toHaveBeenCalledTimes(1);
    });

    it('invalidates chat tags query on success', async () => {
      const newTag: ChatTag = mockTags[0];
      mockCreateChatTag.mockResolvedValue(newTag);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useCreateTag(), { wrapper });

      await result.current.mutateAsync({ name: 'Test' });

      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: ['chat', 'tags'],
      });
    });

    it('calls onSuccess callback with correct parameters', async () => {
      const newTag: ChatTag = mockTags[0];
      const request: CreateTagRequest = { name: 'Test', color: '#FF0000' };
      mockCreateChatTag.mockResolvedValue(newTag);

      const onSuccessMock = vi.fn();

      const { result } = renderHook(
        () => useCreateTag({ onSuccess: onSuccessMock }),
        {
          wrapper: createWrapper(),
        }
      );

      await result.current.mutateAsync(request);

      await waitFor(() => {
        expect(onSuccessMock).toHaveBeenCalledTimes(1);
      });

      // TanStack Query v5 onSuccess receives 4 params: data, variables, context, mutation
      // Context is undefined when no onMutate is provided
      const [dataArg, variablesArg] = onSuccessMock.mock.calls[0];
      expect(dataArg).toEqual(newTag);
      expect(variablesArg).toEqual(request);
    });

    it('handles creation error', async () => {
      const error = new Error('Tag creation failed');
      mockCreateChatTag.mockRejectedValue(error);

      const { result } = renderHook(() => useCreateTag(), {
        wrapper: createWrapper(),
      });

      await expect(
        result.current.mutateAsync({ name: 'Test' })
      ).rejects.toThrow('Tag creation failed');

      await waitFor(() => {
        expect(result.current.error).toEqual(error);
      });
    });
  });

  describe('useUpdateTag', () => {
    it('updates a tag successfully', async () => {
      const updatedTag: ChatTag = {
        ...mockTags[0],
        name: 'Updated Important',
        color: '#FF00FF',
      };
      const request: UpdateTagRequest = {
        name: 'Updated Important',
        color: '#FF00FF',
      };

      mockUpdateChatTag.mockResolvedValue(updatedTag);

      const { result } = renderHook(() => useUpdateTag(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync({ tagId: 'tag-1', request });

      expect(mockUpdateChatTag).toHaveBeenCalledWith('tag-1', request);
      expect(mockUpdateChatTag).toHaveBeenCalledTimes(1);
    });

    it('invalidates chat tags and specific tag query on success', async () => {
      const updatedTag: ChatTag = mockTags[0];
      mockUpdateChatTag.mockResolvedValue(updatedTag);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useUpdateTag(), { wrapper });

      await result.current.mutateAsync({
        tagId: 'tag-1',
        request: { name: 'Updated' },
      });

      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: ['chat', 'tags'],
      });
      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: ['chat', 'tags', 'tag-1'],
      });
    });

    it('calls onSuccess callback with correct parameters', async () => {
      const updatedTag: ChatTag = mockTags[0];
      const variables = { tagId: 'tag-1', request: { name: 'Updated' } };
      mockUpdateChatTag.mockResolvedValue(updatedTag);

      const onSuccessMock = vi.fn();

      const { result } = renderHook(
        () => useUpdateTag({ onSuccess: onSuccessMock }),
        {
          wrapper: createWrapper(),
        }
      );

      await result.current.mutateAsync(variables);

      await waitFor(() => {
        expect(onSuccessMock).toHaveBeenCalledTimes(1);
      });

      // TanStack Query v5 onSuccess receives 4 params: data, variables, context, mutation
      // Context is undefined when no onMutate is provided
      const [dataArg, variablesArg] = onSuccessMock.mock.calls[0];
      expect(dataArg).toEqual(updatedTag);
      expect(variablesArg).toEqual(variables);
    });

    it('handles update error', async () => {
      const error = new Error('Tag update failed');
      mockUpdateChatTag.mockRejectedValue(error);

      const { result } = renderHook(() => useUpdateTag(), {
        wrapper: createWrapper(),
      });

      await expect(
        result.current.mutateAsync({ tagId: 'tag-1', request: { name: 'Test' } })
      ).rejects.toThrow('Tag update failed');

      await waitFor(() => {
        expect(result.current.error).toEqual(error);
      });
    });
  });

  describe('useDeleteTag', () => {
    it('deletes a tag successfully', async () => {
      mockDeleteChatTag.mockResolvedValue(undefined);

      const { result } = renderHook(() => useDeleteTag(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync('tag-1');

      expect(mockDeleteChatTag).toHaveBeenCalledWith('tag-1');
      expect(mockDeleteChatTag).toHaveBeenCalledTimes(1);
    });

    it('invalidates chat tags query on success', async () => {
      mockDeleteChatTag.mockResolvedValue(undefined);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useDeleteTag(), { wrapper });

      await result.current.mutateAsync('tag-1');

      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: ['chat', 'tags'],
      });
    });

    it('calls onSuccess callback with correct parameters', async () => {
      mockDeleteChatTag.mockResolvedValue(undefined);

      const onSuccessMock = vi.fn();

      const { result } = renderHook(
        () => useDeleteTag({ onSuccess: onSuccessMock }),
        {
          wrapper: createWrapper(),
        }
      );

      await result.current.mutateAsync('tag-1');

      await waitFor(() => {
        expect(onSuccessMock).toHaveBeenCalledTimes(1);
      });

      // TanStack Query v5 onSuccess receives 4 params: data, variables, context, mutation
      // Context is undefined when no onMutate is provided
      const [dataArg, variablesArg] = onSuccessMock.mock.calls[0];
      expect(dataArg).toBeUndefined();
      expect(variablesArg).toEqual('tag-1');
    });

    it('handles deletion error', async () => {
      const error = new Error('Tag deletion failed');
      mockDeleteChatTag.mockRejectedValue(error);

      const { result } = renderHook(() => useDeleteTag(), {
        wrapper: createWrapper(),
      });

      await expect(result.current.mutateAsync('tag-1')).rejects.toThrow(
        'Tag deletion failed'
      );

      await waitFor(() => {
        expect(result.current.error).toEqual(error);
      });
    });
  });

  describe('useAssignTagsToSession', () => {
    it('assigns tags to session successfully', async () => {
      mockAssignTagsToSession.mockResolvedValue(mockSessionTags);

      const { result } = renderHook(() => useAssignTagsToSession(), {
        wrapper: createWrapper(),
      });

      const variables = { sessionId: 'session-1', tagIds: ['tag-1', 'tag-2'] };
      await result.current.mutateAsync(variables);

      expect(mockAssignTagsToSession).toHaveBeenCalledWith('session-1', [
        'tag-1',
        'tag-2',
      ]);
      expect(mockAssignTagsToSession).toHaveBeenCalledTimes(1);
    });

    it('invalidates session tags query on success', async () => {
      mockAssignTagsToSession.mockResolvedValue(mockSessionTags);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useAssignTagsToSession(), { wrapper });

      await result.current.mutateAsync({
        sessionId: 'session-1',
        tagIds: ['tag-1'],
      });

      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: ['chat', 'sessions', 'session-1', 'tags'],
      });
    });

    it('calls onSuccess callback with correct parameters', async () => {
      const variables = { sessionId: 'session-1', tagIds: ['tag-1', 'tag-2'] };
      mockAssignTagsToSession.mockResolvedValue(mockSessionTags);

      const onSuccessMock = vi.fn();

      const { result } = renderHook(
        () => useAssignTagsToSession({ onSuccess: onSuccessMock }),
        {
          wrapper: createWrapper(),
        }
      );

      await result.current.mutateAsync(variables);

      await waitFor(() => {
        expect(onSuccessMock).toHaveBeenCalledTimes(1);
      });

      // TanStack Query v5 onSuccess receives 4 params: data, variables, context, mutation
      // Context is undefined when no onMutate is provided
      const [dataArg, variablesArg] = onSuccessMock.mock.calls[0];
      expect(dataArg).toEqual(mockSessionTags);
      expect(variablesArg).toEqual(variables);
    });

    it('handles assignment error', async () => {
      const error = new Error('Tag assignment failed');
      mockAssignTagsToSession.mockRejectedValue(error);

      const { result } = renderHook(() => useAssignTagsToSession(), {
        wrapper: createWrapper(),
      });

      await expect(
        result.current.mutateAsync({ sessionId: 'session-1', tagIds: ['tag-1'] })
      ).rejects.toThrow('Tag assignment failed');

      await waitFor(() => {
        expect(result.current.error).toEqual(error);
      });
    });

    it('handles empty tag list', async () => {
      mockAssignTagsToSession.mockResolvedValue([]);

      const { result } = renderHook(() => useAssignTagsToSession(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync({ sessionId: 'session-1', tagIds: [] });

      expect(mockAssignTagsToSession).toHaveBeenCalledWith('session-1', []);
    });
  });

  describe('useRemoveTagFromSession', () => {
    it('removes tag from session successfully', async () => {
      mockRemoveTagFromSession.mockResolvedValue(undefined);

      const { result } = renderHook(() => useRemoveTagFromSession(), {
        wrapper: createWrapper(),
      });

      const variables = { sessionId: 'session-1', tagId: 'tag-1' };
      await result.current.mutateAsync(variables);

      expect(mockRemoveTagFromSession).toHaveBeenCalledWith('session-1', 'tag-1');
      expect(mockRemoveTagFromSession).toHaveBeenCalledTimes(1);
    });

    it('invalidates session tags query on success', async () => {
      mockRemoveTagFromSession.mockResolvedValue(undefined);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useRemoveTagFromSession(), { wrapper });

      await result.current.mutateAsync({
        sessionId: 'session-1',
        tagId: 'tag-1',
      });

      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: ['chat', 'sessions', 'session-1', 'tags'],
      });
    });

    it('calls onSuccess callback with correct parameters', async () => {
      const variables = { sessionId: 'session-1', tagId: 'tag-1' };
      mockRemoveTagFromSession.mockResolvedValue(undefined);

      const onSuccessMock = vi.fn();

      const { result } = renderHook(
        () => useRemoveTagFromSession({ onSuccess: onSuccessMock }),
        {
          wrapper: createWrapper(),
        }
      );

      await result.current.mutateAsync(variables);

      await waitFor(() => {
        expect(onSuccessMock).toHaveBeenCalledTimes(1);
      });

      // TanStack Query v5 onSuccess receives 4 params: data, variables, context, mutation
      // Context is undefined when no onMutate is provided
      const [dataArg, variablesArg] = onSuccessMock.mock.calls[0];
      expect(dataArg).toBeUndefined();
      expect(variablesArg).toEqual(variables);
    });

    it('handles removal error', async () => {
      const error = new Error('Tag removal failed');
      mockRemoveTagFromSession.mockRejectedValue(error);

      const { result } = renderHook(() => useRemoveTagFromSession(), {
        wrapper: createWrapper(),
      });

      await expect(
        result.current.mutateAsync({ sessionId: 'session-1', tagId: 'tag-1' })
      ).rejects.toThrow('Tag removal failed');

      await waitFor(() => {
        expect(result.current.error).toEqual(error);
      });
    });
  });
});

describe('useChatTags - Edge Cases', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('handles tags with minimal data', async () => {
    const minimalTag: ChatTag = {
      id: 'tag-minimal',
      tenant_id: 'tenant-1',
      name: 'Minimal',
      created_at: '2025-01-01T00:00:00Z',
    };
    mockListChatTags.mockResolvedValue([minimalTag]);

    const { result } = renderHook(() => useChatTags(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(result.current.data?.[0]).toEqual(minimalTag);
    expect(result.current.data?.[0].color).toBeUndefined();
    expect(result.current.data?.[0].description).toBeUndefined();
    expect(result.current.data?.[0].created_by).toBeUndefined();
  });

  it('handles network timeout', async () => {
    const timeoutError = new Error('Network timeout');
    mockListChatTags.mockRejectedValue(timeoutError);

    const { result } = renderHook(() => useChatTags(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });

    expect(result.current.error).toEqual(timeoutError);
  });

  it('handles concurrent mutations', async () => {
    mockCreateChatTag.mockResolvedValue(mockTags[0]);

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );

    const { result } = renderHook(() => useCreateTag(), { wrapper });

    // Fire multiple mutations concurrently
    const promises = [
      result.current.mutateAsync({ name: 'Tag 1' }),
      result.current.mutateAsync({ name: 'Tag 2' }),
      result.current.mutateAsync({ name: 'Tag 3' }),
    ];

    await Promise.all(promises);

    expect(mockCreateChatTag).toHaveBeenCalledTimes(3);
  });

  it('handles partial update request', async () => {
    const partiallyUpdatedTag: ChatTag = {
      ...mockTags[0],
      description: 'Updated description only',
    };
    mockUpdateChatTag.mockResolvedValue(partiallyUpdatedTag);

    const { result } = renderHook(() => useUpdateTag(), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync({
      tagId: 'tag-1',
      request: { description: 'Updated description only' },
    });

    expect(mockUpdateChatTag).toHaveBeenCalledWith('tag-1', {
      description: 'Updated description only',
    });
  });
});
