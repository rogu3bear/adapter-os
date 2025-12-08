import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import {
  useChatCategories,
  useCreateCategory,
  useUpdateCategory,
  useDeleteCategory,
  useSetSessionCategory,
} from '@/hooks/useChatCategories';
import type {
  ChatCategory,
  CreateCategoryRequest,
  UpdateCategoryRequest,
} from '@/api/chat-types';

// Mock API client
const mockListChatCategories = vi.fn();
const mockCreateChatCategory = vi.fn();
const mockUpdateChatCategory = vi.fn();
const mockDeleteChatCategory = vi.fn();
const mockSetSessionCategory = vi.fn();

vi.mock('@/api/client', () => ({
  default: {
    listChatCategories: (...args: unknown[]) => mockListChatCategories(...args),
    createChatCategory: (...args: unknown[]) => mockCreateChatCategory(...args),
    updateChatCategory: (...args: unknown[]) => mockUpdateChatCategory(...args),
    deleteChatCategory: (...args: unknown[]) => mockDeleteChatCategory(...args),
    setSessionCategory: (...args: unknown[]) => mockSetSessionCategory(...args),
  },
}));

// Test data
const mockCategories: ChatCategory[] = [
  {
    id: 'cat-1',
    tenant_id: 'tenant-1',
    parent_id: undefined,
    name: 'Work',
    path: '/Work',
    depth: 0,
    sort_order: 0,
    icon: '💼',
    color: '#3B82F6',
    created_at: '2025-01-01T00:00:00Z',
  },
  {
    id: 'cat-2',
    tenant_id: 'tenant-1',
    parent_id: 'cat-1',
    name: 'Projects',
    path: '/Work/Projects',
    depth: 1,
    sort_order: 0,
    icon: '📂',
    color: '#10B981',
    created_at: '2025-01-02T00:00:00Z',
  },
  {
    id: 'cat-3',
    tenant_id: 'tenant-1',
    parent_id: undefined,
    name: 'Personal',
    path: '/Personal',
    depth: 0,
    sort_order: 1,
    icon: '🏠',
    color: undefined,
    created_at: '2025-01-03T00:00:00Z',
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

describe('useChatCategories - Query Hook', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('returns categories list successfully', async () => {
    mockListChatCategories.mockResolvedValue(mockCategories);

    const { result } = renderHook(() => useChatCategories(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(result.current.data).toEqual(mockCategories);
    expect(mockListChatCategories).toHaveBeenCalledTimes(1);
  });

  it('returns categories in hierarchical tree-sorted order', async () => {
    mockListChatCategories.mockResolvedValue(mockCategories);

    const { result } = renderHook(() => useChatCategories(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    // Verify hierarchical structure is preserved
    const data = result.current.data!;
    expect(data[0].depth).toBe(0);
    expect(data[1].depth).toBe(1);
    expect(data[1].parent_id).toBe(data[0].id);
    expect(data[2].depth).toBe(0);
  });

  it('handles empty categories list', async () => {
    mockListChatCategories.mockResolvedValue([]);

    const { result } = renderHook(() => useChatCategories(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(result.current.data).toEqual([]);
  });

  it('handles API error', async () => {
    const error = new Error('Failed to fetch categories');
    mockListChatCategories.mockRejectedValue(error);

    const { result } = renderHook(() => useChatCategories(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });

    expect(result.current.error).toEqual(error);
  });

  it('accepts custom query options', async () => {
    mockListChatCategories.mockResolvedValue(mockCategories);

    const { result } = renderHook(
      () =>
        useChatCategories({
          enabled: false,
        }),
      {
        wrapper: createWrapper(),
      }
    );

    // Should not fetch when disabled
    expect(result.current.isPending).toBe(true);
    expect(mockListChatCategories).not.toHaveBeenCalled();
  });
});

describe('useCreateCategory - Mutation Hook', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('creates category and calls API correctly', async () => {
    const newCategory: ChatCategory = {
      id: 'cat-4',
      tenant_id: 'tenant-1',
      parent_id: undefined,
      name: 'New Category',
      path: '/New Category',
      depth: 0,
      sort_order: 2,
      icon: '⭐',
      color: '#F59E0B',
      created_at: '2025-01-04T00:00:00Z',
    };
    mockCreateChatCategory.mockResolvedValue(newCategory);

    const { result } = renderHook(() => useCreateCategory(), {
      wrapper: createWrapper(),
    });

    const request: CreateCategoryRequest = {
      name: 'New Category',
      icon: '⭐',
      color: '#F59E0B',
    };

    await result.current.mutateAsync(request);

    expect(mockCreateChatCategory).toHaveBeenCalledWith(request);
    expect(mockCreateChatCategory).toHaveBeenCalledTimes(1);

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(result.current.data).toEqual(newCategory);
  });

  it('creates nested category with parent_id', async () => {
    const nestedCategory: ChatCategory = {
      id: 'cat-5',
      tenant_id: 'tenant-1',
      parent_id: 'cat-1',
      name: 'Meetings',
      path: '/Work/Meetings',
      depth: 1,
      sort_order: 1,
      icon: '📅',
      color: undefined,
      created_at: '2025-01-05T00:00:00Z',
    };
    mockCreateChatCategory.mockResolvedValue(nestedCategory);

    const { result } = renderHook(() => useCreateCategory(), {
      wrapper: createWrapper(),
    });

    const request: CreateCategoryRequest = {
      name: 'Meetings',
      parent_id: 'cat-1',
      icon: '📅',
    };

    await result.current.mutateAsync(request);

    expect(mockCreateChatCategory).toHaveBeenCalledWith(request);

    await waitFor(() => {
      expect(result.current.data?.parent_id).toBe('cat-1');
    });
  });

  it('invalidates categories cache on success', async () => {
    const newCategory: ChatCategory = mockCategories[0];
    mockCreateChatCategory.mockResolvedValue(newCategory);

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });

    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );

    // Pre-populate cache
    queryClient.setQueryData(['chat', 'categories'], mockCategories);

    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

    const { result } = renderHook(() => useCreateCategory(), { wrapper });

    await result.current.mutateAsync({ name: 'Test' });

    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['chat', 'categories'] });
  });

  it('calls onSuccess callback with correct TanStack v5 signature', async () => {
    const newCategory: ChatCategory = mockCategories[0];
    mockCreateChatCategory.mockResolvedValue(newCategory);

    const onSuccess = vi.fn();
    const request: CreateCategoryRequest = { name: 'Test' };

    const { result } = renderHook(() => useCreateCategory({ onSuccess }), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync(request);

    // Verify onSuccess receives 4 parameters: data, variables, context, mutation
    expect(onSuccess).toHaveBeenCalledTimes(1);
    expect(onSuccess.mock.calls[0]).toHaveLength(4);
    expect(onSuccess.mock.calls[0][0]).toEqual(newCategory); // data
    expect(onSuccess.mock.calls[0][1]).toEqual(request); // variables
    expect(onSuccess.mock.calls[0][2]).toBeUndefined(); // context
    expect(onSuccess.mock.calls[0][3]).toBeDefined(); // mutation object
  });

  it('handles API error', async () => {
    const error = new Error('Failed to create category');
    mockCreateChatCategory.mockRejectedValue(error);

    const { result } = renderHook(() => useCreateCategory(), {
      wrapper: createWrapper(),
    });

    await expect(result.current.mutateAsync({ name: 'Test' })).rejects.toThrow(
      'Failed to create category'
    );

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });

    expect(result.current.error).toEqual(error);
  });
});

describe('useUpdateCategory - Mutation Hook', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('updates category and calls API correctly', async () => {
    const updatedCategory: ChatCategory = {
      ...mockCategories[0],
      name: 'Updated Work',
      icon: '💻',
    };
    mockUpdateChatCategory.mockResolvedValue(updatedCategory);

    const { result } = renderHook(() => useUpdateCategory(), {
      wrapper: createWrapper(),
    });

    const updateRequest: UpdateCategoryRequest = {
      name: 'Updated Work',
      icon: '💻',
    };

    await result.current.mutateAsync({
      categoryId: 'cat-1',
      request: updateRequest,
    });

    expect(mockUpdateChatCategory).toHaveBeenCalledWith('cat-1', updateRequest);

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(result.current.data).toEqual(updatedCategory);
  });

  it('allows partial updates', async () => {
    const updatedCategory: ChatCategory = {
      ...mockCategories[0],
      color: '#FF0000',
    };
    mockUpdateChatCategory.mockResolvedValue(updatedCategory);

    const { result } = renderHook(() => useUpdateCategory(), {
      wrapper: createWrapper(),
    });

    // Only update color
    await result.current.mutateAsync({
      categoryId: 'cat-1',
      request: { color: '#FF0000' },
    });

    expect(mockUpdateChatCategory).toHaveBeenCalledWith('cat-1', { color: '#FF0000' });

    await waitFor(() => {
      expect(result.current.data?.color).toBe('#FF0000');
    });
  });

  it('invalidates categories cache on success', async () => {
    const updatedCategory: ChatCategory = mockCategories[0];
    mockUpdateChatCategory.mockResolvedValue(updatedCategory);

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });

    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );

    queryClient.setQueryData(['chat', 'categories'], mockCategories);

    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

    const { result } = renderHook(() => useUpdateCategory(), { wrapper });

    await result.current.mutateAsync({
      categoryId: 'cat-1',
      request: { name: 'Updated' },
    });

    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['chat', 'categories'] });
  });

  it('calls onSuccess callback with correct TanStack v5 signature', async () => {
    const updatedCategory: ChatCategory = mockCategories[0];
    mockUpdateChatCategory.mockResolvedValue(updatedCategory);

    const onSuccess = vi.fn();
    const variables = {
      categoryId: 'cat-1',
      request: { name: 'Updated' },
    };

    const { result } = renderHook(() => useUpdateCategory({ onSuccess }), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync(variables);

    // Verify onSuccess receives 4 parameters
    expect(onSuccess).toHaveBeenCalledTimes(1);
    expect(onSuccess.mock.calls[0]).toHaveLength(4);
    expect(onSuccess.mock.calls[0][0]).toEqual(updatedCategory); // data
    expect(onSuccess.mock.calls[0][1]).toEqual(variables); // variables
    expect(onSuccess.mock.calls[0][2]).toBeUndefined(); // context
    expect(onSuccess.mock.calls[0][3]).toBeDefined(); // mutation object
  });

  it('handles API error', async () => {
    const error = new Error('Category not found');
    mockUpdateChatCategory.mockRejectedValue(error);

    const { result } = renderHook(() => useUpdateCategory(), {
      wrapper: createWrapper(),
    });

    await expect(
      result.current.mutateAsync({
        categoryId: 'nonexistent',
        request: { name: 'Test' },
      })
    ).rejects.toThrow('Category not found');

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });

    expect(result.current.error).toEqual(error);
  });
});

describe('useDeleteCategory - Mutation Hook', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('deletes category and calls API correctly', async () => {
    mockDeleteChatCategory.mockResolvedValue(undefined);

    const { result } = renderHook(() => useDeleteCategory(), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync('cat-1');

    expect(mockDeleteChatCategory).toHaveBeenCalledWith('cat-1');
    expect(mockDeleteChatCategory).toHaveBeenCalledTimes(1);

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });
  });

  it('invalidates both categories AND chat-sessions cache on success', async () => {
    mockDeleteChatCategory.mockResolvedValue(undefined);

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });

    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );

    // Pre-populate both caches
    queryClient.setQueryData(['chat', 'categories'], mockCategories);
    queryClient.setQueryData(['chat-sessions'], [{ id: 'session-1', category_id: 'cat-1' }]);

    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

    const { result } = renderHook(() => useDeleteCategory(), { wrapper });

    await result.current.mutateAsync('cat-1');

    // Verify BOTH query keys are invalidated
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['chat', 'categories'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['chat-sessions'] });
    expect(invalidateSpy).toHaveBeenCalledTimes(2);
  });

  it('calls onSuccess callback with correct TanStack v5 signature', async () => {
    mockDeleteChatCategory.mockResolvedValue(undefined);

    const onSuccess = vi.fn();
    const categoryId = 'cat-1';

    const { result } = renderHook(() => useDeleteCategory({ onSuccess }), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync(categoryId);

    // Verify onSuccess receives 4 parameters
    expect(onSuccess).toHaveBeenCalledTimes(1);
    expect(onSuccess.mock.calls[0]).toHaveLength(4);
    expect(onSuccess.mock.calls[0][0]).toBeUndefined(); // data (void)
    expect(onSuccess.mock.calls[0][1]).toBe(categoryId); // variables
    expect(onSuccess.mock.calls[0][2]).toBeUndefined(); // context
    expect(onSuccess.mock.calls[0][3]).toBeDefined(); // mutation object
  });

  it('handles API error', async () => {
    const error = new Error('Failed to delete category');
    mockDeleteChatCategory.mockRejectedValue(error);

    const { result } = renderHook(() => useDeleteCategory(), {
      wrapper: createWrapper(),
    });

    await expect(result.current.mutateAsync('cat-1')).rejects.toThrow('Failed to delete category');

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });

    expect(result.current.error).toEqual(error);
  });

  it('handles category with children deletion', async () => {
    // Backend should handle cascade deletion or return error
    mockDeleteChatCategory.mockResolvedValue(undefined);

    const { result } = renderHook(() => useDeleteCategory(), {
      wrapper: createWrapper(),
    });

    // Delete parent category
    await result.current.mutateAsync('cat-1');

    expect(mockDeleteChatCategory).toHaveBeenCalledWith('cat-1');

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });
  });
});

describe('useSetSessionCategory - Mutation Hook', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('sets session category and calls API correctly', async () => {
    mockSetSessionCategory.mockResolvedValue(undefined);

    const { result } = renderHook(() => useSetSessionCategory(), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync({
      sessionId: 'session-1',
      categoryId: 'cat-1',
    });

    expect(mockSetSessionCategory).toHaveBeenCalledWith('session-1', 'cat-1');

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });
  });

  it('allows setting category to null (uncategorize)', async () => {
    mockSetSessionCategory.mockResolvedValue(undefined);

    const { result } = renderHook(() => useSetSessionCategory(), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync({
      sessionId: 'session-1',
      categoryId: null,
    });

    expect(mockSetSessionCategory).toHaveBeenCalledWith('session-1', null);

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });
  });

  it('invalidates both categories AND chat-sessions cache on success', async () => {
    mockSetSessionCategory.mockResolvedValue(undefined);

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });

    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );

    // Pre-populate both caches
    queryClient.setQueryData(['chat', 'categories'], mockCategories);
    queryClient.setQueryData(['chat-sessions'], [{ id: 'session-1' }]);

    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

    const { result } = renderHook(() => useSetSessionCategory(), { wrapper });

    await result.current.mutateAsync({
      sessionId: 'session-1',
      categoryId: 'cat-1',
    });

    // Verify BOTH query keys are invalidated
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['chat', 'categories'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['chat-sessions'] });
    expect(invalidateSpy).toHaveBeenCalledTimes(2);
  });

  it('calls onSuccess callback with correct TanStack v5 signature', async () => {
    mockSetSessionCategory.mockResolvedValue(undefined);

    const onSuccess = vi.fn();
    const variables = {
      sessionId: 'session-1',
      categoryId: 'cat-1',
    };

    const { result } = renderHook(() => useSetSessionCategory({ onSuccess }), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync(variables);

    // Verify onSuccess receives 4 parameters
    expect(onSuccess).toHaveBeenCalledTimes(1);
    expect(onSuccess.mock.calls[0]).toHaveLength(4);
    expect(onSuccess.mock.calls[0][0]).toBeUndefined(); // data (void)
    expect(onSuccess.mock.calls[0][1]).toEqual(variables); // variables
    expect(onSuccess.mock.calls[0][2]).toBeUndefined(); // context
    expect(onSuccess.mock.calls[0][3]).toBeDefined(); // mutation object
  });

  it('handles API error', async () => {
    const error = new Error('Session not found');
    mockSetSessionCategory.mockRejectedValue(error);

    const { result } = renderHook(() => useSetSessionCategory(), {
      wrapper: createWrapper(),
    });

    await expect(
      result.current.mutateAsync({
        sessionId: 'nonexistent',
        categoryId: 'cat-1',
      })
    ).rejects.toThrow('Session not found');

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });

    expect(result.current.error).toEqual(error);
  });

  it('handles invalid category error', async () => {
    const error = new Error('Category not found');
    mockSetSessionCategory.mockRejectedValue(error);

    const { result } = renderHook(() => useSetSessionCategory(), {
      wrapper: createWrapper(),
    });

    await expect(
      result.current.mutateAsync({
        sessionId: 'session-1',
        categoryId: 'nonexistent',
      })
    ).rejects.toThrow('Category not found');

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });
  });
});

describe('Error Handling - All Hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('all mutation hooks handle network errors', async () => {
    const networkError = new Error('Network request failed');

    mockCreateChatCategory.mockRejectedValue(networkError);
    mockUpdateChatCategory.mockRejectedValue(networkError);
    mockDeleteChatCategory.mockRejectedValue(networkError);
    mockSetSessionCategory.mockRejectedValue(networkError);

    const wrapper = createWrapper();

    // Test create
    const { result: createResult } = renderHook(() => useCreateCategory(), { wrapper });
    await expect(createResult.current.mutateAsync({ name: 'Test' })).rejects.toThrow(
      'Network request failed'
    );

    // Test update
    const { result: updateResult } = renderHook(() => useUpdateCategory(), { wrapper });
    await expect(
      updateResult.current.mutateAsync({ categoryId: 'cat-1', request: {} })
    ).rejects.toThrow('Network request failed');

    // Test delete
    const { result: deleteResult } = renderHook(() => useDeleteCategory(), { wrapper });
    await expect(deleteResult.current.mutateAsync('cat-1')).rejects.toThrow(
      'Network request failed'
    );

    // Test setSessionCategory
    const { result: setResult } = renderHook(() => useSetSessionCategory(), { wrapper });
    await expect(
      setResult.current.mutateAsync({ sessionId: 'session-1', categoryId: 'cat-1' })
    ).rejects.toThrow('Network request failed');
  });

  it('query hook handles 401 unauthorized', async () => {
    const authError = new Error('Unauthorized');
    mockListChatCategories.mockRejectedValue(authError);

    const { result } = renderHook(() => useChatCategories(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });

    expect(result.current.error?.message).toBe('Unauthorized');
  });

  it('query hook handles 403 forbidden', async () => {
    const forbiddenError = new Error('Forbidden');
    mockListChatCategories.mockRejectedValue(forbiddenError);

    const { result } = renderHook(() => useChatCategories(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });

    expect(result.current.error?.message).toBe('Forbidden');
  });
});
