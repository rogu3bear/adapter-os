import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { useAdapterBulkActions } from '@/hooks/adapters/useAdapterBulkActions';
import type { Adapter } from '@/api/types';

// Mock the API client
const mockLoadAdapter = vi.fn();
const mockUnloadAdapter = vi.fn();
const mockDeleteAdapter = vi.fn();
const mockRegisterAdapter = vi.fn();

vi.mock('@/api/services', () => ({
  default: {
    loadAdapter: (...args: unknown[]) => mockLoadAdapter(...args),
    unloadAdapter: (...args: unknown[]) => mockUnloadAdapter(...args),
    deleteAdapter: (...args: unknown[]) => mockDeleteAdapter(...args),
    registerAdapter: (...args: unknown[]) => mockRegisterAdapter(...args),
  },
  apiClient: {
    loadAdapter: (...args: unknown[]) => mockLoadAdapter(...args),
    unloadAdapter: (...args: unknown[]) => mockUnloadAdapter(...args),
    deleteAdapter: (...args: unknown[]) => mockDeleteAdapter(...args),
    registerAdapter: (...args: unknown[]) => mockRegisterAdapter(...args),
  },
}));

// Mock toast
const mockToastSuccess = vi.fn();
const mockToastError = vi.fn();
const mockToastWarning = vi.fn();

vi.mock('sonner', () => ({
  toast: {
    success: (...args: unknown[]) => mockToastSuccess(...args),
    error: (...args: unknown[]) => mockToastError(...args),
    warning: (...args: unknown[]) => mockToastWarning(...args),
  },
}));

// Mock logger
const mockLoggerInfo = vi.fn();
const mockLoggerError = vi.fn();

vi.mock('@/utils/logger', () => ({
  logger: {
    info: (...args: unknown[]) => mockLoggerInfo(...args),
    error: (...args: unknown[]) => mockLoggerError(...args),
  },
  toError: (err: unknown) => (err instanceof Error ? err : new Error(String(err))),
}));

// Mock UndoRedoContext
const mockAddAction = vi.fn();

vi.mock('@/contexts/UndoRedoContext', () => ({
  useUndoRedoContext: () => ({
    addAction: mockAddAction,
    undo: vi.fn(),
    redo: vi.fn(),
    canUndo: false,
    canRedo: false,
    lastAction: null,
    clearHistory: vi.fn(),
    historyCount: 0,
  }),
}));

// Test data
const mockAdapters: Adapter[] = [
  {
    adapter_id: 'adapter-1',
    name: 'Test Adapter 1',
    hash_b3: 'hash1',
    rank: 16,
    tier: 'warm',
    category: 'code',
    framework: 'rust',
    scope: 'global',
    languages: ['rust'],
    active: false,
    pinned: false,
    created_at: '2025-01-01T00:00:00Z',
  },
  {
    adapter_id: 'adapter-2',
    name: 'Test Adapter 2',
    hash_b3: 'hash2',
    rank: 24,
    tier: 'persistent',
    category: 'framework',
    framework: 'typescript',
    scope: 'tenant',
    languages: ['typescript'],
    active: true,
    pinned: false,
    created_at: '2025-01-02T00:00:00Z',
  },
  {
    adapter_id: 'adapter-3',
    name: 'Test Adapter 3',
    hash_b3: 'hash3',
    rank: 8,
    tier: 'ephemeral',
    category: 'codebase',
    framework: 'python',
    scope: 'user',
    languages: ['python'],
    active: false,
    pinned: true,
    created_at: '2025-01-03T00:00:00Z',
  },
];

describe('useAdapterBulkActions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('initial state', () => {
    it('returns correct initial state', () => {
      const { result } = renderHook(() => useAdapterBulkActions());

      expect(result.current.selectedIds).toEqual(new Set());
      expect(result.current.isBulkOperationRunning).toBe(false);
      expect(result.current.bulkOperationProgress).toBeNull();
      expect(result.current.confirmationState).toBeNull();
    });

    it('returns all operation functions', () => {
      const { result } = renderHook(() => useAdapterBulkActions());

      expect(typeof result.current.selectAll).toBe('function');
      expect(typeof result.current.clearSelection).toBe('function');
      expect(typeof result.current.toggleSelection).toBe('function');
      expect(typeof result.current.bulkLoad).toBe('function');
      expect(typeof result.current.bulkUnload).toBe('function');
      expect(typeof result.current.bulkDelete).toBe('function');
      expect(typeof result.current.requestConfirmation).toBe('function');
      expect(typeof result.current.confirmAction).toBe('function');
      expect(typeof result.current.cancelConfirmation).toBe('function');
    });
  });

  describe('selection management', () => {
    it('selectAll adds all IDs to selection', () => {
      const { result } = renderHook(() => useAdapterBulkActions());

      act(() => {
        result.current.selectAll(['adapter-1', 'adapter-2', 'adapter-3']);
      });

      expect(result.current.selectedIds.size).toBe(3);
      expect(result.current.selectedIds.has('adapter-1')).toBe(true);
      expect(result.current.selectedIds.has('adapter-2')).toBe(true);
      expect(result.current.selectedIds.has('adapter-3')).toBe(true);
    });

    it('selectAll with empty array clears selection', () => {
      const { result } = renderHook(() => useAdapterBulkActions());

      act(() => {
        result.current.selectAll(['adapter-1', 'adapter-2']);
      });

      expect(result.current.selectedIds.size).toBe(2);

      act(() => {
        result.current.selectAll([]);
      });

      expect(result.current.selectedIds.size).toBe(0);
    });

    it('clearSelection removes all selections', () => {
      const { result } = renderHook(() => useAdapterBulkActions());

      act(() => {
        result.current.selectAll(['adapter-1', 'adapter-2']);
      });

      expect(result.current.selectedIds.size).toBe(2);

      act(() => {
        result.current.clearSelection();
      });

      expect(result.current.selectedIds.size).toBe(0);
    });

    it('toggleSelection adds ID if not present', () => {
      const { result } = renderHook(() => useAdapterBulkActions());

      act(() => {
        result.current.toggleSelection('adapter-1');
      });

      expect(result.current.selectedIds.has('adapter-1')).toBe(true);
      expect(result.current.selectedIds.size).toBe(1);
    });

    it('toggleSelection removes ID if present', () => {
      const { result } = renderHook(() => useAdapterBulkActions());

      act(() => {
        result.current.selectAll(['adapter-1', 'adapter-2']);
        result.current.toggleSelection('adapter-1');
      });

      expect(result.current.selectedIds.has('adapter-1')).toBe(false);
      expect(result.current.selectedIds.has('adapter-2')).toBe(true);
      expect(result.current.selectedIds.size).toBe(1);
    });

    it('toggleSelection multiple times toggles correctly', () => {
      const { result } = renderHook(() => useAdapterBulkActions());

      act(() => {
        result.current.toggleSelection('adapter-1');
      });
      expect(result.current.selectedIds.has('adapter-1')).toBe(true);

      act(() => {
        result.current.toggleSelection('adapter-1');
      });
      expect(result.current.selectedIds.has('adapter-1')).toBe(false);

      act(() => {
        result.current.toggleSelection('adapter-1');
      });
      expect(result.current.selectedIds.has('adapter-1')).toBe(true);
    });

    it('setSelectedIds updates selection directly', () => {
      const { result } = renderHook(() => useAdapterBulkActions());

      act(() => {
        result.current.setSelectedIds(new Set(['adapter-1', 'adapter-3']));
      });

      expect(result.current.selectedIds.size).toBe(2);
      expect(result.current.selectedIds.has('adapter-1')).toBe(true);
      expect(result.current.selectedIds.has('adapter-2')).toBe(false);
      expect(result.current.selectedIds.has('adapter-3')).toBe(true);
    });
  });

  describe('confirmation dialog state', () => {
    it('requestConfirmation opens dialog with correct state', () => {
      const { result } = renderHook(() => useAdapterBulkActions());

      act(() => {
        result.current.requestConfirmation('load', ['adapter-1', 'adapter-2']);
      });

      expect(result.current.confirmationState).toEqual({
        isOpen: true,
        action: 'load',
        ids: ['adapter-1', 'adapter-2'],
      });
    });

    it('cancelConfirmation closes dialog and clears state', () => {
      const { result } = renderHook(() => useAdapterBulkActions());

      act(() => {
        result.current.requestConfirmation('delete', ['adapter-1']);
        result.current.cancelConfirmation();
      });

      expect(result.current.confirmationState).toBeNull();
    });

    it('supports different action types in confirmation', () => {
      const { result } = renderHook(() => useAdapterBulkActions());

      act(() => {
        result.current.requestConfirmation('load', ['adapter-1']);
      });
      expect(result.current.confirmationState?.action).toBe('load');

      act(() => {
        result.current.requestConfirmation('unload', ['adapter-2']);
      });
      expect(result.current.confirmationState?.action).toBe('unload');

      act(() => {
        result.current.requestConfirmation('delete', ['adapter-3']);
      });
      expect(result.current.confirmationState?.action).toBe('delete');
    });
  });

  describe('bulkLoad', () => {
    it('requests confirmation before loading', async () => {
      const { result } = renderHook(() =>
        useAdapterBulkActions({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.bulkLoad(['adapter-1', 'adapter-2']);
      });

      expect(result.current.confirmationState).toEqual({
        isOpen: true,
        action: 'load',
        ids: ['adapter-1', 'adapter-2'],
      });
    });

    it('loads adapters after confirmation', async () => {
      mockLoadAdapter.mockResolvedValue({ success: true });
      const onSuccess = vi.fn();
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onSuccess,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkLoad(['adapter-1', 'adapter-2']);
        await result.current.confirmAction();
      });

      expect(mockLoadAdapter).toHaveBeenCalledTimes(2);
      expect(mockLoadAdapter).toHaveBeenCalledWith('adapter-1');
      expect(mockLoadAdapter).toHaveBeenCalledWith('adapter-2');
      expect(onSuccess).toHaveBeenCalledWith('load', 2);
      expect(onDataRefresh).toHaveBeenCalled();
    });

    it('tracks progress during bulk load', async () => {
      mockLoadAdapter.mockResolvedValue({ success: true });
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({ adapters: mockAdapters, onDataRefresh })
      );

      // First request confirmation
      await act(async () => {
        await result.current.bulkLoad(['adapter-1', 'adapter-2', 'adapter-3']);
      });

      // Then confirm and check the operation runs
      await act(async () => {
        await result.current.confirmAction();
      });

      // After completion, state should be reset
      expect(result.current.isBulkOperationRunning).toBe(false);
      expect(result.current.bulkOperationProgress).toBeNull();
      expect(mockLoadAdapter).toHaveBeenCalledTimes(3);
    });

    it('handles partial failures in bulk load', async () => {
      mockLoadAdapter
        .mockResolvedValueOnce({ success: true })
        .mockRejectedValueOnce(new Error('Load failed'))
        .mockResolvedValueOnce({ success: true });

      const onSuccess = vi.fn();
      const onError = vi.fn();
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onSuccess,
          onError,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkLoad(['adapter-1', 'adapter-2', 'adapter-3']);
        await result.current.confirmAction();
      });

      expect(mockLoadAdapter).toHaveBeenCalledTimes(3);
      expect(onSuccess).toHaveBeenCalledWith('load', 2);
      expect(onError).toHaveBeenCalled();
      expect(mockToastSuccess).toHaveBeenCalledWith('Successfully loaded 2 adapter(s)');
      expect(mockToastError).toHaveBeenCalledWith('Failed to load 1 adapter(s)');
    });

    it('keeps failed IDs selected after operation', async () => {
      mockLoadAdapter
        .mockResolvedValueOnce({ success: true })
        .mockRejectedValueOnce(new Error('Load failed'));

      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkLoad(['adapter-1', 'adapter-2']);
        await result.current.confirmAction();
      });

      expect(result.current.selectedIds.has('adapter-1')).toBe(false);
      expect(result.current.selectedIds.has('adapter-2')).toBe(true);
    });

    it('shows warning when no adapters selected', async () => {
      const { result } = renderHook(() => useAdapterBulkActions());

      await act(async () => {
        await result.current.bulkLoad([]);
        await result.current.confirmAction();
      });

      expect(mockToastWarning).toHaveBeenCalledWith('No adapters selected for load');
      expect(mockLoadAdapter).not.toHaveBeenCalled();
    });

    it('records undo action on successful load', async () => {
      mockLoadAdapter.mockResolvedValue({ success: true });
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkLoad(['adapter-1']);
        await result.current.confirmAction();
      });

      expect(mockAddAction).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'bulk_load_adapters',
          description: 'Load 1 adapter(s)',
          previousState: expect.any(Array),
          reverse: expect.any(Function),
        })
      );
    });
  });

  describe('bulkUnload', () => {
    it('requests confirmation before unloading', async () => {
      const { result } = renderHook(() =>
        useAdapterBulkActions({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.bulkUnload(['adapter-2']);
      });

      expect(result.current.confirmationState).toEqual({
        isOpen: true,
        action: 'unload',
        ids: ['adapter-2'],
      });
    });

    it('unloads adapters after confirmation', async () => {
      mockUnloadAdapter.mockResolvedValue({ success: true });
      const onSuccess = vi.fn();
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onSuccess,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkUnload(['adapter-2']);
        await result.current.confirmAction();
      });

      expect(mockUnloadAdapter).toHaveBeenCalledWith('adapter-2');
      expect(onSuccess).toHaveBeenCalledWith('unload', 1);
      expect(mockToastSuccess).toHaveBeenCalledWith('Successfully unloaded 1 adapter(s)');
    });

    it('handles errors during unload', async () => {
      mockUnloadAdapter.mockRejectedValue(new Error('Unload failed'));
      const onError = vi.fn();
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onError,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkUnload(['adapter-2']);
        await result.current.confirmAction();
      });

      expect(onError).toHaveBeenCalled();
      expect(mockLoggerError).toHaveBeenCalled();
    });

    it('records undo action on successful unload', async () => {
      mockUnloadAdapter.mockResolvedValue({ success: true });
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkUnload(['adapter-2']);
        await result.current.confirmAction();
      });

      expect(mockAddAction).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'bulk_unload_adapters',
          description: 'Unload 1 adapter(s)',
          reverse: expect.any(Function),
        })
      );
    });
  });

  describe('bulkDelete', () => {
    it('requests confirmation before deleting', async () => {
      const { result } = renderHook(() =>
        useAdapterBulkActions({ adapters: mockAdapters })
      );

      await act(async () => {
        await result.current.bulkDelete(['adapter-1', 'adapter-3']);
      });

      expect(result.current.confirmationState).toEqual({
        isOpen: true,
        action: 'delete',
        ids: ['adapter-1', 'adapter-3'],
      });
    });

    it('deletes adapters after confirmation', async () => {
      mockDeleteAdapter.mockResolvedValue({ success: true });
      const onSuccess = vi.fn();
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onSuccess,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkDelete(['adapter-1', 'adapter-3']);
        await result.current.confirmAction();
      });

      expect(mockDeleteAdapter).toHaveBeenCalledTimes(2);
      expect(mockDeleteAdapter).toHaveBeenCalledWith('adapter-1');
      expect(mockDeleteAdapter).toHaveBeenCalledWith('adapter-3');
      expect(onSuccess).toHaveBeenCalledWith('delete', 2);
      expect(mockToastSuccess).toHaveBeenCalledWith('Successfully deleted 2 adapter(s)');
    });

    it('handles partial deletion failures', async () => {
      mockDeleteAdapter
        .mockResolvedValueOnce({ success: true })
        .mockRejectedValueOnce(new Error('Delete failed'));

      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkDelete(['adapter-1', 'adapter-2']);
        await result.current.confirmAction();
      });

      expect(mockDeleteAdapter).toHaveBeenCalledTimes(2);
      expect(mockToastSuccess).toHaveBeenCalledWith('Successfully deleted 1 adapter(s)');
      expect(mockToastError).toHaveBeenCalledWith('Failed to delete 1 adapter(s)');
    });

    it('records undo action with adapter restoration data', async () => {
      mockDeleteAdapter.mockResolvedValue({ success: true });
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkDelete(['adapter-1']);
        await result.current.confirmAction();
      });

      expect(mockAddAction).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'bulk_delete_adapters',
          description: 'Delete 1 adapter(s)',
          previousState: expect.arrayContaining([
            expect.objectContaining({
              adapter_id: 'adapter-1',
              name: 'Test Adapter 1',
            }),
          ]),
          reverse: expect.any(Function),
        })
      );
    });

    it('clears confirmation state after deletion', async () => {
      mockDeleteAdapter.mockResolvedValue({ success: true });
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkDelete(['adapter-1']);
      });

      expect(result.current.confirmationState?.isOpen).toBe(true);

      await act(async () => {
        await result.current.confirmAction();
      });

      expect(result.current.confirmationState).toBeNull();
    });
  });

  describe('confirmAction', () => {
    it('does nothing if no pending action', async () => {
      const { result } = renderHook(() => useAdapterBulkActions());

      await act(async () => {
        await result.current.confirmAction();
      });

      expect(mockLoadAdapter).not.toHaveBeenCalled();
      expect(mockUnloadAdapter).not.toHaveBeenCalled();
      expect(mockDeleteAdapter).not.toHaveBeenCalled();
    });

    it('clears confirmation state after execution', async () => {
      mockLoadAdapter.mockResolvedValue({ success: true });
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkLoad(['adapter-1']);
        await result.current.confirmAction();
      });

      expect(result.current.confirmationState).toBeNull();
    });
  });

  describe('progress tracking', () => {
    it('updates progress during operation', async () => {
      let progressSnapshots: Array<{ current: number; total: number } | null> = [];

      mockLoadAdapter.mockImplementation(async () => {
        await new Promise((resolve) => setTimeout(resolve, 10));
        return { success: true };
      });

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onDataRefresh: vi.fn().mockResolvedValue(undefined),
        })
      );

      await act(async () => {
        await result.current.bulkLoad(['adapter-1', 'adapter-2', 'adapter-3']);
        const confirmPromise = result.current.confirmAction();

        // Capture progress snapshots
        progressSnapshots.push(result.current.bulkOperationProgress);

        await confirmPromise;
      });

      // Final state should have no progress
      expect(result.current.bulkOperationProgress).toBeNull();
      expect(result.current.isBulkOperationRunning).toBe(false);
    });

    it('sets total count correctly', async () => {
      mockLoadAdapter.mockResolvedValue({ success: true });

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onDataRefresh: vi.fn().mockResolvedValue(undefined),
        })
      );

      await act(async () => {
        await result.current.bulkLoad(['adapter-1', 'adapter-2', 'adapter-3']);
        const confirmPromise = result.current.confirmAction();
        await confirmPromise;
      });

      expect(mockLoadAdapter).toHaveBeenCalledTimes(3);
    });
  });

  describe('error handling', () => {
    it('logs errors with appropriate context', async () => {
      mockLoadAdapter.mockRejectedValue(new Error('Network error'));
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkLoad(['adapter-1']);
        await result.current.confirmAction();
      });

      expect(mockLoggerError).toHaveBeenCalledWith(
        'Bulk load: failed to load adapter',
        expect.objectContaining({
          component: 'useAdapterBulkActions',
          operation: 'bulkLoad',
          adapterId: 'adapter-1',
        }),
        expect.any(Error)
      );
    });

    it('resets operation state on error', async () => {
      mockDeleteAdapter.mockRejectedValue(new Error('Delete error'));
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkDelete(['adapter-1']);
        await result.current.confirmAction();
      });

      expect(result.current.isBulkOperationRunning).toBe(false);
      expect(result.current.bulkOperationProgress).toBeNull();
    });
  });

  describe('callback integration', () => {
    it('calls onDataRefresh after successful operation', async () => {
      mockLoadAdapter.mockResolvedValue({ success: true });
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkLoad(['adapter-1']);
        await result.current.confirmAction();
      });

      expect(onDataRefresh).toHaveBeenCalledTimes(1);
    });

    it('calls onSuccess with correct parameters', async () => {
      mockUnloadAdapter.mockResolvedValue({ success: true });
      const onSuccess = vi.fn();
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onSuccess,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkUnload(['adapter-1', 'adapter-2']);
        await result.current.confirmAction();
      });

      expect(onSuccess).toHaveBeenCalledWith('unload', 2);
    });

    it('calls onError with error details', async () => {
      const error = new Error('Operation failed');
      mockLoadAdapter.mockRejectedValue(error);
      const onError = vi.fn();
      const onDataRefresh = vi.fn().mockResolvedValue(undefined);

      const { result } = renderHook(() =>
        useAdapterBulkActions({
          adapters: mockAdapters,
          onError,
          onDataRefresh,
        })
      );

      await act(async () => {
        await result.current.bulkLoad(['adapter-1']);
        await result.current.confirmAction();
      });

      expect(onError).toHaveBeenCalledWith(
        expect.objectContaining({
          message: expect.stringContaining('Failed to load'),
        }),
        'load'
      );
    });
  });
});
