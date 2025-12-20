/**
 * Adapter Hook Types
 *
 * Type definitions for adapter-related hooks including bulk actions,
 * operations, filters, exports, and detail views.
 */

import type { Adapter } from '@/api/types';

// ============================================================================
// useAdapterBulkActions Types
// ============================================================================

export interface UseAdapterBulkActionsOptions {
  /** Callback on successful operation completion */
  onSuccess?: (action: string, count: number) => void;
  /** Callback on operation error */
  onError?: (error: Error, action: string) => void;
  /** Query keys to invalidate on success (for react-query integration) */
  invalidateKeys?: string[][];
  /** Optional adapters list for snapshot/rollback */
  adapters?: Adapter[];
  /** Callback to refresh adapters list */
  onDataRefresh?: () => void | Promise<void>;
}

export interface BulkOperationProgress {
  /** Current item being processed */
  current: number;
  /** Total items to process */
  total: number;
}

export interface BulkActionConfirmationState {
  /** Whether confirmation dialog is open */
  isOpen: boolean;
  /** Action being confirmed (load, unload, delete) */
  action: string;
  /** Adapter IDs to operate on */
  ids: string[];
}

export interface UseAdapterBulkActionsReturn {
  // Selection state
  selectedIds: Set<string>;
  setSelectedIds: (ids: Set<string>) => void;
  selectAll: (ids: string[]) => void;
  clearSelection: () => void;
  toggleSelection: (id: string) => void;

  // Bulk operations
  bulkLoad: (ids: string[]) => Promise<void>;
  bulkUnload: (ids: string[]) => Promise<void>;
  bulkDelete: (ids: string[]) => Promise<void>;

  // State
  isBulkOperationRunning: boolean;
  bulkOperationProgress: BulkOperationProgress | null;

  // Confirmation
  confirmationState: BulkActionConfirmationState | null;
  requestConfirmation: (action: string, ids: string[]) => void;
  confirmAction: () => Promise<void>;
  cancelConfirmation: () => void;
}

// ============================================================================
// useAdapterExport Types
// ============================================================================

export interface UseAdapterExportOptions {
  /** Callback on successful export */
  onSuccess?: (adapterId: string, path: string) => void;
  /** Callback on export error */
  onError?: (error: Error, adapterId: string) => void;
  /** Show success toast automatically */
  showSuccessToast?: boolean;
  /** Show error toast automatically */
  showErrorToast?: boolean;
}

export interface UseAdapterExportReturn {
  /** Export a single adapter */
  exportAdapter: (adapterId: string, outputPath?: string) => Promise<void>;
  /** Export multiple adapters */
  exportAdapters: (adapterIds: string[], outputDir?: string) => Promise<void>;
  /** Whether export is in progress */
  isExporting: boolean;
  /** Current export progress */
  exportProgress: BulkOperationProgress | null;
  /** Last export error */
  error: Error | null;
  /** Reset export state */
  reset: () => void;
}

// ============================================================================
// useAdapterOperations Types
// ============================================================================

export interface UseAdapterOperationsOptions {
  /** Callback on successful operation */
  onSuccess?: (operation: string, adapterId: string) => void;
  /** Callback on operation error */
  onError?: (error: Error, operation: string, adapterId: string) => void;
  /** Query keys to invalidate on success */
  invalidateKeys?: string[][];
}

export interface UseAdapterOperationsReturn {
  /** Load an adapter */
  loadAdapter: (adapterId: string) => Promise<void>;
  /** Unload an adapter */
  unloadAdapter: (adapterId: string) => Promise<void>;
  /** Delete an adapter */
  deleteAdapter: (adapterId: string) => Promise<void>;
  /** Whether any operation is in progress */
  isOperating: boolean;
  /** Current operation */
  currentOperation: string | null;
  /** Current adapter being operated on */
  currentAdapterId: string | null;
  /** Last error */
  error: Error | null;
}

// ============================================================================
// useAdapterDetail Types
// ============================================================================

export interface UseAdapterDetailOptions {
  /** Adapter ID to fetch details for */
  adapterId: string;
  /** Enable/disable the query */
  enabled?: boolean;
}

export interface UseAdapterDetailReturn {
  /** Adapter details */
  adapter: Adapter | null;
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
  /** Refetch adapter details */
  refetch: () => Promise<void>;
}

// ============================================================================
// useAdapterFilterState Types
// ============================================================================

export interface UseAdapterFilterStateOptions {
  /** Initial filter values */
  initialFilters?: Record<string, unknown>;
}

export interface UseAdapterFilterStateReturn {
  /** Current filter values */
  filters: Record<string, unknown>;
  /** Update a single filter */
  setFilter: (key: string, value: unknown) => void;
  /** Update multiple filters */
  setFilters: (filters: Record<string, unknown>) => void;
  /** Clear all filters */
  clearFilters: () => void;
  /** Reset to initial filters */
  resetFilters: () => void;
  /** Active filter count */
  activeFilterCount: number;
}

// ============================================================================
// useAdapterActions Types
// ============================================================================

export interface UseAdapterActionsOptions {
  /** Callback on successful action */
  onSuccess?: (action: string, adapterId: string) => void;
  /** Callback on action error */
  onError?: (error: Error, action: string, adapterId: string) => void;
  /** Show toast notifications */
  showToasts?: boolean;
}

// ============================================================================
// useAdapterDialogs Types
// ============================================================================

export interface UseAdapterDialogsReturn {
  /** Whether create dialog is open */
  isCreateOpen: boolean;
  /** Open create dialog */
  openCreate: () => void;
  /** Close create dialog */
  closeCreate: () => void;
  /** Whether edit dialog is open */
  isEditOpen: boolean;
  /** Adapter being edited */
  editingAdapter: Adapter | null;
  /** Open edit dialog */
  openEdit: (adapter: Adapter) => void;
  /** Close edit dialog */
  closeEdit: () => void;
  /** Whether delete dialog is open */
  isDeleteOpen: boolean;
  /** Adapter being deleted */
  deletingAdapter: Adapter | null;
  /** Open delete dialog */
  openDelete: (adapter: Adapter) => void;
  /** Close delete dialog */
  closeDelete: () => void;
}
