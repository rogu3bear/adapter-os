import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import {
  usePublishAdapter,
  useArchiveAdapter,
  useUnarchiveAdapter,
  adapterPublishKeys,
} from '@/hooks/adapters/useAdapterPublish';
import type {
  PublishAdapterRequest,
  PublishAdapterResponse,
  ArchiveAdapterResponse,
} from '@/api/adapter-types';

// Mock API client
const mockPublishAdapterVersion = vi.hoisted(() => vi.fn());
const mockArchiveAdapterVersion = vi.hoisted(() => vi.fn());
const mockUnarchiveAdapterVersion = vi.hoisted(() => vi.fn());

vi.mock('@/api/services', () => ({
  apiClient: {
    publishAdapterVersion: (...args: unknown[]) => mockPublishAdapterVersion(...args),
    archiveAdapterVersion: (...args: unknown[]) => mockArchiveAdapterVersion(...args),
    unarchiveAdapterVersion: (...args: unknown[]) => mockUnarchiveAdapterVersion(...args),
  },
}));

// Mock toast
const mockToastSuccess = vi.hoisted(() => vi.fn());
const mockToastError = vi.hoisted(() => vi.fn());

vi.mock('sonner', () => ({
  toast: {
    success: (...args: unknown[]) => mockToastSuccess(...args),
    error: (...args: unknown[]) => mockToastError(...args),
  },
}));

// Mock logger
const mockLoggerInfo = vi.hoisted(() => vi.fn());
const mockLoggerError = vi.hoisted(() => vi.fn());

vi.mock('@/utils/logger', () => ({
  logger: {
    info: (...args: unknown[]) => mockLoggerInfo(...args),
    error: (...args: unknown[]) => mockLoggerError(...args),
  },
}));

// Mock useTenant for tenant-scoped query keys
vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => ({ selectedTenant: 'test-tenant' }),
}));

// Test data
const mockPublishRequest: PublishAdapterRequest = {
  attach_mode: 'manual',
  notes: 'Initial publish',
};

const mockPublishResponse: PublishAdapterResponse = {
  version_id: 'v-123',
  repo_id: 'repo-1',
  attach_mode: 'manual',
  published_at: '2025-01-01T00:00:00Z',
};

const mockArchiveResponse: ArchiveAdapterResponse = {
  version_id: 'v-123',
  archived_at: '2025-01-01T00:00:00Z',
};

// Test wrapper
function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

describe('usePublishAdapter', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('mutation behavior', () => {
    it('publishes adapter successfully', async () => {
      mockPublishAdapterVersion.mockResolvedValue(mockPublishResponse);

      const { result } = renderHook(() => usePublishAdapter(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync({
        repoId: 'repo-1',
        versionId: 'v-123',
        data: mockPublishRequest,
      });

      expect(mockPublishAdapterVersion).toHaveBeenCalledWith('repo-1', 'v-123', mockPublishRequest);
    });

    it('calls API with correct parameters', async () => {
      mockPublishAdapterVersion.mockResolvedValue(mockPublishResponse);

      const { result } = renderHook(() => usePublishAdapter(), {
        wrapper: createWrapper(),
      });

      const request: PublishAdapterRequest = {
        attach_mode: 'auto',
        notes: 'Test publish',
      };

      await result.current.mutateAsync({
        repoId: 'repo-2',
        versionId: 'v-456',
        data: request,
      });

      expect(mockPublishAdapterVersion).toHaveBeenCalledWith('repo-2', 'v-456', request);
    });

    it('handles different attach modes', async () => {
      mockPublishAdapterVersion.mockResolvedValue(mockPublishResponse);

      const { result } = renderHook(() => usePublishAdapter(), {
        wrapper: createWrapper(),
      });

      // Manual mode
      await result.current.mutateAsync({
        repoId: 'repo-1',
        versionId: 'v-1',
        data: { attach_mode: 'manual' },
      });

      expect(mockPublishAdapterVersion).toHaveBeenCalledWith('repo-1', 'v-1', {
        attach_mode: 'manual',
      });

      // Auto mode
      await result.current.mutateAsync({
        repoId: 'repo-1',
        versionId: 'v-2',
        data: { attach_mode: 'auto' },
      });

      expect(mockPublishAdapterVersion).toHaveBeenCalledWith('repo-1', 'v-2', {
        attach_mode: 'auto',
      });
    });
  });

  describe('success handling', () => {
    it('shows success toast on publish', async () => {
      mockPublishAdapterVersion.mockResolvedValue(mockPublishResponse);

      const { result } = renderHook(() => usePublishAdapter(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync({
        repoId: 'repo-1',
        versionId: 'v-123',
        data: mockPublishRequest,
      });

      expect(mockToastSuccess).toHaveBeenCalledWith('Adapter published successfully');
    });

    it('logs success with details', async () => {
      mockPublishAdapterVersion.mockResolvedValue(mockPublishResponse);

      const { result } = renderHook(() => usePublishAdapter(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync({
        repoId: 'repo-1',
        versionId: 'v-123',
        data: mockPublishRequest,
      });

      expect(mockLoggerInfo).toHaveBeenCalledWith(
        'Adapter published',
        expect.objectContaining({
          component: 'useAdapterPublish',
          operation: 'publishAdapter',
          versionId: 'v-123',
          repoId: 'repo-1',
          attachMode: 'manual',
        })
      );
    });

    it('invalidates related queries on success', async () => {
      mockPublishAdapterVersion.mockResolvedValue(mockPublishResponse);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      // Spy on invalidateQueries
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

      const { result } = renderHook(() => usePublishAdapter(), { wrapper });

      await result.current.mutateAsync({
        repoId: 'repo-1',
        versionId: 'v-123',
        data: mockPublishRequest,
      });

      // Query keys now include tenant segment via withTenantKey
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['adapters', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['adapter-versions', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['training-jobs', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['repos', 'test-tenant'] });
    });
  });

  describe('error handling', () => {
    it('shows error toast on failure', async () => {
      const error = new Error('Publish failed');
      mockPublishAdapterVersion.mockRejectedValue(error);

      const { result } = renderHook(() => usePublishAdapter(), {
        wrapper: createWrapper(),
      });

      await expect(
        result.current.mutateAsync({
          repoId: 'repo-1',
          versionId: 'v-123',
          data: mockPublishRequest,
        })
      ).rejects.toThrow('Publish failed');

      await waitFor(() => {
        expect(mockToastError).toHaveBeenCalledWith('Failed to publish adapter: Publish failed');
      });
    });

    it('logs error with context', async () => {
      const error = new Error('Network error');
      mockPublishAdapterVersion.mockRejectedValue(error);

      const { result } = renderHook(() => usePublishAdapter(), {
        wrapper: createWrapper(),
      });

      try {
        await result.current.mutateAsync({
          repoId: 'repo-1',
          versionId: 'v-123',
          data: mockPublishRequest,
        });
      } catch (e) {
        // Expected to throw
      }

      await waitFor(() => {
        expect(mockLoggerError).toHaveBeenCalledWith(
          'Failed to publish adapter',
          expect.objectContaining({
            component: 'useAdapterPublish',
            operation: 'publishAdapter',
          }),
          error
        );
      });
    });

    it('handles validation errors', async () => {
      const validationError = new Error('Invalid attach mode');
      mockPublishAdapterVersion.mockRejectedValue(validationError);

      const { result } = renderHook(() => usePublishAdapter(), {
        wrapper: createWrapper(),
      });

      await expect(
        result.current.mutateAsync({
          repoId: 'repo-1',
          versionId: 'v-123',
          data: mockPublishRequest,
        })
      ).rejects.toThrow('Invalid attach mode');
    });
  });

  describe('loading state', () => {
    it('sets isPending during mutation', async () => {
      mockPublishAdapterVersion.mockImplementation(
        () => new Promise((resolve) => setTimeout(() => resolve(mockPublishResponse), 100))
      );

      const { result } = renderHook(() => usePublishAdapter(), {
        wrapper: createWrapper(),
      });

      const mutatePromise = result.current.mutateAsync({
        repoId: 'repo-1',
        versionId: 'v-123',
        data: mockPublishRequest,
      });

      await waitFor(() => {
        expect(result.current.isPending).toBe(true);
      });

      await mutatePromise;

      await waitFor(() => {
        expect(result.current.isPending).toBe(false);
      });
    });
  });
});

describe('useArchiveAdapter', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('mutation behavior', () => {
    it('archives adapter successfully', async () => {
      mockArchiveAdapterVersion.mockResolvedValue(mockArchiveResponse);

      const { result } = renderHook(() => useArchiveAdapter(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync({ versionId: 'v-123' });

      expect(mockArchiveAdapterVersion).toHaveBeenCalledWith('v-123', undefined);
    });

    it('archives adapter with reason', async () => {
      mockArchiveAdapterVersion.mockResolvedValue(mockArchiveResponse);

      const { result } = renderHook(() => useArchiveAdapter(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync({
        versionId: 'v-123',
        reason: 'Deprecated due to security issue',
      });

      expect(mockArchiveAdapterVersion).toHaveBeenCalledWith(
        'v-123',
        'Deprecated due to security issue'
      );
    });
  });

  describe('success handling', () => {
    it('shows success toast on archive', async () => {
      mockArchiveAdapterVersion.mockResolvedValue(mockArchiveResponse);

      const { result } = renderHook(() => useArchiveAdapter(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync({ versionId: 'v-123' });

      expect(mockToastSuccess).toHaveBeenCalledWith('Adapter archived');
    });

    it('logs archive with details', async () => {
      mockArchiveAdapterVersion.mockResolvedValue(mockArchiveResponse);

      const { result } = renderHook(() => useArchiveAdapter(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync({ versionId: 'v-123' });

      expect(mockLoggerInfo).toHaveBeenCalledWith(
        'Adapter archived',
        expect.objectContaining({
          component: 'useAdapterPublish',
          operation: 'archiveAdapter',
          versionId: 'v-123',
        })
      );
    });

    it('invalidates related queries on success', async () => {
      mockArchiveAdapterVersion.mockResolvedValue(mockArchiveResponse);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

      const { result } = renderHook(() => useArchiveAdapter(), { wrapper });

      await result.current.mutateAsync({ versionId: 'v-123' });

      // Query keys now include tenant segment via withTenantKey
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['adapters', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['adapter-versions', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['adapter-stacks', 'test-tenant'] });
    });
  });

  describe('error handling', () => {
    it('shows error toast on failure', async () => {
      const error = new Error('Archive failed');
      mockArchiveAdapterVersion.mockRejectedValue(error);

      const { result } = renderHook(() => useArchiveAdapter(), {
        wrapper: createWrapper(),
      });

      await expect(result.current.mutateAsync({ versionId: 'v-123' })).rejects.toThrow(
        'Archive failed'
      );

      await waitFor(() => {
        expect(mockToastError).toHaveBeenCalledWith('Failed to archive adapter: Archive failed');
      });
    });

    it('logs error with context', async () => {
      const error = new Error('Database error');
      mockArchiveAdapterVersion.mockRejectedValue(error);

      const { result } = renderHook(() => useArchiveAdapter(), {
        wrapper: createWrapper(),
      });

      try {
        await result.current.mutateAsync({ versionId: 'v-123' });
      } catch (e) {
        // Expected
      }

      await waitFor(() => {
        expect(mockLoggerError).toHaveBeenCalledWith(
          'Failed to archive adapter',
          expect.objectContaining({
            component: 'useAdapterPublish',
            operation: 'archiveAdapter',
          }),
          error
        );
      });
    });
  });
});

describe('useUnarchiveAdapter', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('mutation behavior', () => {
    it('unarchives adapter successfully', async () => {
      mockUnarchiveAdapterVersion.mockResolvedValue(mockArchiveResponse);

      const { result } = renderHook(() => useUnarchiveAdapter(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync('v-123');

      expect(mockUnarchiveAdapterVersion).toHaveBeenCalledWith('v-123');
    });

    it('handles multiple unarchive operations', async () => {
      mockUnarchiveAdapterVersion.mockResolvedValue(mockArchiveResponse);

      const { result } = renderHook(() => useUnarchiveAdapter(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync('v-1');
      await result.current.mutateAsync('v-2');

      expect(mockUnarchiveAdapterVersion).toHaveBeenCalledTimes(2);
      expect(mockUnarchiveAdapterVersion).toHaveBeenCalledWith('v-1');
      expect(mockUnarchiveAdapterVersion).toHaveBeenCalledWith('v-2');
    });
  });

  describe('success handling', () => {
    it('shows success toast on unarchive', async () => {
      mockUnarchiveAdapterVersion.mockResolvedValue(mockArchiveResponse);

      const { result } = renderHook(() => useUnarchiveAdapter(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync('v-123');

      expect(mockToastSuccess).toHaveBeenCalledWith('Adapter restored');
    });

    it('logs unarchive with details', async () => {
      mockUnarchiveAdapterVersion.mockResolvedValue(mockArchiveResponse);

      const { result } = renderHook(() => useUnarchiveAdapter(), {
        wrapper: createWrapper(),
      });

      await result.current.mutateAsync('v-123');

      expect(mockLoggerInfo).toHaveBeenCalledWith(
        'Adapter unarchived',
        expect.objectContaining({
          component: 'useAdapterPublish',
          operation: 'unarchiveAdapter',
          versionId: 'v-123',
        })
      );
    });

    it('invalidates related queries on success', async () => {
      mockUnarchiveAdapterVersion.mockResolvedValue(mockArchiveResponse);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');

      const { result } = renderHook(() => useUnarchiveAdapter(), { wrapper });

      await result.current.mutateAsync('v-123');

      // Query keys now include tenant segment via withTenantKey
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['adapters', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['adapter-versions', 'test-tenant'] });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['adapter-stacks', 'test-tenant'] });
    });
  });

  describe('error handling', () => {
    it('shows error toast on failure', async () => {
      const error = new Error('Unarchive failed');
      mockUnarchiveAdapterVersion.mockRejectedValue(error);

      const { result } = renderHook(() => useUnarchiveAdapter(), {
        wrapper: createWrapper(),
      });

      await expect(result.current.mutateAsync('v-123')).rejects.toThrow('Unarchive failed');

      await waitFor(() => {
        expect(mockToastError).toHaveBeenCalledWith('Failed to restore adapter: Unarchive failed');
      });
    });

    it('logs error with context', async () => {
      const error = new Error('Permission denied');
      mockUnarchiveAdapterVersion.mockRejectedValue(error);

      const { result } = renderHook(() => useUnarchiveAdapter(), {
        wrapper: createWrapper(),
      });

      try {
        await result.current.mutateAsync('v-123');
      } catch (e) {
        // Expected
      }

      await waitFor(() => {
        expect(mockLoggerError).toHaveBeenCalledWith(
          'Failed to unarchive adapter',
          expect.objectContaining({
            component: 'useAdapterPublish',
            operation: 'unarchiveAdapter',
          }),
          error
        );
      });
    });
  });
});

describe('adapterPublishKeys', () => {
  it('generates correct query keys', () => {
    expect(adapterPublishKeys.all).toEqual(['adapter-publish']);
    expect(adapterPublishKeys.detail('v-123')).toEqual(['adapter-publish', 'v-123']);
  });
});
