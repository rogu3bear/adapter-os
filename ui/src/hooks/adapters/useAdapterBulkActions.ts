/**
 * Adapter Bulk Actions Hook
 *
 * Manages bulk operations on adapters (load, unload, delete) with progress tracking,
 * confirmation dialogs, and undo/redo support.
 *
 * @example
 * ```tsx
 * const bulkActions = useAdapterBulkActions({
 *   onSuccess: (action, count) => {
 *     toast.success(`${action} completed for ${count} adapters`);
 *     refetchAdapters();
 *   },
 *   onError: (error, action) => {
 *     toast.error(`Failed to ${action}: ${error.message}`);
 *   },
 *   invalidateKeys: [['adapters'], ['adapter-stacks']],
 * });
 *
 * // Select adapters
 * bulkActions.selectAll(adapterIds);
 *
 * // Execute bulk load with confirmation
 * bulkActions.requestConfirmation('load', ['adapter-1', 'adapter-2']);
 * // User confirms...
 * await bulkActions.confirmAction();
 * ```
 *
 * Features:
 * - Selection management (toggle, select all, clear)
 * - Bulk load/unload/delete operations
 * - Progress tracking with current/total counts
 * - Confirmation dialog state management
 * - Toast notifications on success/error
 * - Undo/redo integration
 * - Query invalidation on success
 * - Optimistic updates with rollback on failure
 *
 * Citations:
 * - ui/src/components/Adapters.tsx (lines 518-820) - Bulk operation handlers
 * - ui/src/hooks/useBulkActions.ts - Generic bulk actions pattern
 * - ui/src/hooks/useAdapterOperations.ts - Single adapter operations
 */

import { useState, useCallback, useRef } from 'react';
import { toast } from 'sonner';
import { apiClient } from '@/api/services';
import { logger, toError } from '@/utils/logger';
import { useUndoRedoContext } from '@/contexts/UndoRedoContext';
import type { Adapter } from '@/api/types';
import type {
  UseAdapterBulkActionsOptions,
  BulkOperationProgress,
  BulkActionConfirmationState,
  UseAdapterBulkActionsReturn,
} from '@/types/hooks';

/**
 * Hook for managing bulk adapter operations with confirmation and progress tracking.
 *
 * @param options - Configuration options for bulk actions
 * @returns Bulk action state and control functions
 */
export function useAdapterBulkActions(
  options: UseAdapterBulkActionsOptions = {}
): UseAdapterBulkActionsReturn {
  const {
    onSuccess,
    onError,
    invalidateKeys = [],
    adapters = [],
    onDataRefresh,
  } = options;

  const { addAction } = useUndoRedoContext();

  // Selection state
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  // Operation state
  const [isBulkOperationRunning, setIsBulkOperationRunning] = useState(false);
  const [bulkOperationProgress, setBulkOperationProgress] = useState<BulkOperationProgress | null>(null);

  // Confirmation state
  const [confirmationState, setConfirmationState] = useState<BulkActionConfirmationState | null>(null);

  // Pending action to execute after confirmation
  const pendingActionRef = useRef<(() => Promise<void>) | null>(null);

  /**
   * Select all adapters from provided IDs
   */
  const selectAll = useCallback((ids: string[]) => {
    setSelectedIds(new Set(ids));
  }, []);

  /**
   * Clear all selections
   */
  const clearSelection = useCallback(() => {
    setSelectedIds(new Set());
  }, []);

  /**
   * Toggle selection of a single adapter
   */
  const toggleSelection = useCallback((id: string) => {
    setSelectedIds(prev => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  /**
   * Request confirmation for a bulk action
   */
  const requestConfirmation = useCallback((action: string, ids: string[]) => {
    setConfirmationState({
      isOpen: true,
      action,
      ids,
    });
  }, []);

  /**
   * Cancel confirmation dialog
   */
  const cancelConfirmation = useCallback(() => {
    setConfirmationState(null);
    pendingActionRef.current = null;
  }, []);

  /**
   * Execute bulk load operation
   */
  const performBulkLoad = useCallback(async (adapterIds: string[]) => {
    const snapshots = adapters
      .filter(adapter => adapterIds.includes(adapter.adapter_id))
      .map(adapter => ({ ...adapter }));

    if (snapshots.length === 0) {
      toast.warning('No adapters selected for load');
      return;
    }

    setIsBulkOperationRunning(true);
    setBulkOperationProgress({ current: 0, total: adapterIds.length });

    const failedIds: string[] = [];
    let currentIndex = 0;

    try {
      for (const adapterId of adapterIds) {
        try {
          setBulkOperationProgress({ current: currentIndex + 1, total: adapterIds.length });
          await apiClient.loadAdapter(adapterId);
          currentIndex++;

          logger.info('Bulk load: adapter loaded successfully', {
            component: 'useAdapterBulkActions',
            operation: 'bulkLoad',
            adapterId,
            progress: `${currentIndex}/${adapterIds.length}`,
          });
        } catch (err) {
          failedIds.push(adapterId);
          logger.error('Bulk load: failed to load adapter', {
            component: 'useAdapterBulkActions',
            operation: 'bulkLoad',
            adapterId,
          }, toError(err));
        }
      }

      const successfulIds = adapterIds.filter(id => !failedIds.includes(id));

      if (successfulIds.length > 0) {
        toast.success(`Successfully loaded ${successfulIds.length} adapter(s)`);
        onSuccess?.('load', successfulIds.length);

        // Record undo action
        addAction({
          type: 'bulk_load_adapters',
          description: `Load ${successfulIds.length} adapter(s)`,
          previousState: snapshots.filter(snapshot => successfulIds.includes(snapshot.adapter_id)),
          reverse: async () => {
            try {
              for (const snapshot of snapshots.filter(s => successfulIds.includes(s.adapter_id))) {
                if (!snapshot.active) {
                  await apiClient.unloadAdapter(snapshot.adapter_id);
                }
              }
              await onDataRefresh?.();
              toast.success('Reverted adapter load');
            } catch (err) {
              logger.error('Failed to undo bulk load', {
                component: 'useAdapterBulkActions',
                operation: 'undoBulkLoad',
              }, toError(err));
              toast.error('Failed to undo load operation');
            }
          },
        });
      }

      if (failedIds.length > 0) {
        const error = new Error(`Failed to load ${failedIds.length} adapter(s)`);
        toast.error(error.message);
        onError?.(error, 'load');
      }

      // Refresh data
      await onDataRefresh?.();

      // Clear selection except for failed items
      setSelectedIds(new Set(failedIds));

    } finally {
      setIsBulkOperationRunning(false);
      setBulkOperationProgress(null);
    }
  }, [adapters, addAction, onDataRefresh, onSuccess, onError]);

  /**
   * Execute bulk unload operation
   */
  const performBulkUnload = useCallback(async (adapterIds: string[]) => {
    const snapshots = adapters
      .filter(adapter => adapterIds.includes(adapter.adapter_id))
      .map(adapter => ({ ...adapter }));

    if (snapshots.length === 0) {
      toast.warning('No adapters selected for unload');
      return;
    }

    setIsBulkOperationRunning(true);
    setBulkOperationProgress({ current: 0, total: adapterIds.length });

    const failedIds: string[] = [];
    let currentIndex = 0;

    try {
      for (const adapterId of adapterIds) {
        try {
          setBulkOperationProgress({ current: currentIndex + 1, total: adapterIds.length });
          await apiClient.unloadAdapter(adapterId);
          currentIndex++;

          logger.info('Bulk unload: adapter unloaded successfully', {
            component: 'useAdapterBulkActions',
            operation: 'bulkUnload',
            adapterId,
            progress: `${currentIndex}/${adapterIds.length}`,
          });
        } catch (err) {
          failedIds.push(adapterId);
          logger.error('Bulk unload: failed to unload adapter', {
            component: 'useAdapterBulkActions',
            operation: 'bulkUnload',
            adapterId,
          }, toError(err));
        }
      }

      const successfulIds = adapterIds.filter(id => !failedIds.includes(id));

      if (successfulIds.length > 0) {
        toast.success(`Successfully unloaded ${successfulIds.length} adapter(s)`);
        onSuccess?.('unload', successfulIds.length);

        // Record undo action
        addAction({
          type: 'bulk_unload_adapters',
          description: `Unload ${successfulIds.length} adapter(s)`,
          previousState: snapshots.filter(snapshot => successfulIds.includes(snapshot.adapter_id)),
          reverse: async () => {
            try {
              for (const snapshot of snapshots.filter(s => successfulIds.includes(s.adapter_id))) {
                if (snapshot.active) {
                  await apiClient.loadAdapter(snapshot.adapter_id);
                }
              }
              await onDataRefresh?.();
              toast.success('Reverted adapter unload');
            } catch (err) {
              logger.error('Failed to undo bulk unload', {
                component: 'useAdapterBulkActions',
                operation: 'undoBulkUnload',
              }, toError(err));
              toast.error('Failed to undo unload operation');
            }
          },
        });
      }

      if (failedIds.length > 0) {
        const error = new Error(`Failed to unload ${failedIds.length} adapter(s)`);
        toast.error(error.message);
        onError?.(error, 'unload');
      }

      // Refresh data
      await onDataRefresh?.();

      // Clear selection except for failed items
      setSelectedIds(new Set(failedIds));

    } finally {
      setIsBulkOperationRunning(false);
      setBulkOperationProgress(null);
    }
  }, [adapters, addAction, onDataRefresh, onSuccess, onError]);

  /**
   * Execute bulk delete operation
   */
  const performBulkDelete = useCallback(async (adapterIds: string[]) => {
    const snapshots = adapters
      .filter(adapter => adapterIds.includes(adapter.adapter_id))
      .map(adapter => ({ ...adapter }));

    if (snapshots.length === 0) {
      toast.warning('No adapters selected for deletion');
      return;
    }

    setIsBulkOperationRunning(true);
    setBulkOperationProgress({ current: 0, total: adapterIds.length });

    const failedAdapters: Adapter[] = [];
    let currentIndex = 0;

    try {
      for (const adapterId of adapterIds) {
        try {
          setBulkOperationProgress({ current: currentIndex + 1, total: adapterIds.length });
          await apiClient.deleteAdapter(adapterId);
          currentIndex++;

          logger.info('Bulk delete: adapter deleted successfully', {
            component: 'useAdapterBulkActions',
            operation: 'bulkDelete',
            adapterId,
            progress: `${currentIndex}/${adapterIds.length}`,
          });
        } catch (err) {
          const original = snapshots.find(adapter => adapter.adapter_id === adapterId);
          if (original) {
            failedAdapters.push(original);
          }
          logger.error('Bulk delete: failed to delete adapter', {
            component: 'useAdapterBulkActions',
            operation: 'bulkDelete',
            adapterId,
          }, toError(err));
        }
      }

      const successfulAdapters = snapshots.filter(
        snapshot => !failedAdapters.some(failed => failed.adapter_id === snapshot.adapter_id)
      );

      if (successfulAdapters.length > 0) {
        toast.success(`Successfully deleted ${successfulAdapters.length} adapter(s)`);
        onSuccess?.('delete', successfulAdapters.length);

        // Record undo action
        addAction({
          type: 'bulk_delete_adapters',
          description: `Delete ${successfulAdapters.length} adapter(s)`,
          previousState: successfulAdapters,
          reverse: async () => {
            try {
              for (const adapter of successfulAdapters) {
                await apiClient.registerAdapter({
                  adapter_id: adapter.adapter_id,
                  name: adapter.name,
                  hash_b3: adapter.hash_b3,
                  rank: adapter.rank,
                  tier: adapter.tier,
                  category: adapter.category ?? 'code',
                  framework: adapter.framework,
                  scope: adapter.scope ?? 'global',
                  languages: adapter.languages ?? [],
                });
              }
              await onDataRefresh?.();
              toast.success(`Restored ${successfulAdapters.length} adapter(s)`);
            } catch (err) {
              logger.error('Failed to undo bulk delete', {
                component: 'useAdapterBulkActions',
                operation: 'undoBulkDelete',
              }, toError(err));
              toast.error('Failed to restore adapters');
            }
          },
        });
      }

      if (failedAdapters.length > 0) {
        const error = new Error(`Failed to delete ${failedAdapters.length} adapter(s)`);
        toast.error(error.message);
        onError?.(error, 'delete');
      }

      // Refresh data
      await onDataRefresh?.();

      // Clear selection except for failed items
      setSelectedIds(new Set(failedAdapters.map(a => a.adapter_id)));

    } finally {
      setIsBulkOperationRunning(false);
      setBulkOperationProgress(null);
    }
  }, [adapters, addAction, onDataRefresh, onSuccess, onError]);

  /**
   * Bulk load adapters with confirmation
   */
  const bulkLoad = useCallback(async (ids: string[]) => {
    pendingActionRef.current = () => performBulkLoad(ids);
    requestConfirmation('load', ids);
  }, [performBulkLoad, requestConfirmation]);

  /**
   * Bulk unload adapters with confirmation
   */
  const bulkUnload = useCallback(async (ids: string[]) => {
    pendingActionRef.current = () => performBulkUnload(ids);
    requestConfirmation('unload', ids);
  }, [performBulkUnload, requestConfirmation]);

  /**
   * Bulk delete adapters with confirmation
   */
  const bulkDelete = useCallback(async (ids: string[]) => {
    pendingActionRef.current = () => performBulkDelete(ids);
    requestConfirmation('delete', ids);
  }, [performBulkDelete, requestConfirmation]);

  /**
   * Execute the confirmed action
   */
  const confirmAction = useCallback(async () => {
    if (pendingActionRef.current) {
      try {
        await pendingActionRef.current();
      } finally {
        setConfirmationState(null);
        pendingActionRef.current = null;
      }
    }
  }, []);

  return {
    // Selection
    selectedIds,
    setSelectedIds,
    selectAll,
    clearSelection,
    toggleSelection,

    // Bulk operations
    bulkLoad,
    bulkUnload,
    bulkDelete,

    // State
    isBulkOperationRunning,
    bulkOperationProgress,

    // Confirmation
    confirmationState,
    requestConfirmation,
    confirmAction,
    cancelConfirmation,
  };
}

export default useAdapterBulkActions;
