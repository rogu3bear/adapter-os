/**
 * Selection State Types
 *
 * Generic selection state management for lists and tables.
 *
 * Citations:
 * - ui/src/hooks/adapters/useAdapterBulkActions.ts - Adapter selection
 * - ui/src/components/Adapters.tsx - Selection UI patterns
 */

/**
 * Generic selection state with typed items
 */
export interface SelectionState<T = unknown> {
  /** Set of selected item IDs */
  selectedIds: Set<string>;
  /** Full selected item objects */
  selectedItems: T[];
  /** Last selected ID (for shift-click range selection) */
  lastSelectedId: string | null;
  /** Total number of items available for selection */
  totalCount?: number;
}

/**
 * Selection actions
 */
export interface SelectionActions<T = unknown> {
  /** Toggle single item selection */
  toggle: (id: string, item?: T) => void;
  /** Select single item */
  select: (id: string, item?: T) => void;
  /** Deselect single item */
  deselect: (id: string) => void;
  /** Select all items */
  selectAll: (ids: string[], items?: T[]) => void;
  /** Clear all selections */
  clearSelection: () => void;
  /** Check if item is selected */
  isSelected: (id: string) => boolean;
  /** Get count of selected items */
  getSelectedCount: () => number;
  /** Check if all items are selected */
  isAllSelected: (totalCount: number) => boolean;
  /** Check if some (but not all) items are selected */
  isIndeterminate: (totalCount: number) => boolean;
}

/**
 * Complete selection state with actions
 */
export interface SelectionStateWithActions<T = unknown>
  extends SelectionState<T>,
    SelectionActions<T> {}

/**
 * Multi-select mode configuration
 */
export interface MultiSelectConfig {
  /** Allow shift-click range selection */
  enableRangeSelection?: boolean;
  /** Allow ctrl/cmd-click toggle */
  enableToggleSelection?: boolean;
  /** Maximum number of items that can be selected */
  maxSelection?: number;
  /** Whether to preserve selection on page change */
  preserveOnPageChange?: boolean;
}

/**
 * Selection mode
 */
export type SelectionMode = 'none' | 'single' | 'multiple';

/**
 * Selection state with mode
 */
export interface SelectionStateWithMode<T = unknown> extends SelectionState<T> {
  mode: SelectionMode;
}

/**
 * Bulk action confirmation state
 */
export interface BulkActionConfirmationState {
  /** Whether confirmation dialog is open */
  isOpen: boolean;
  /** Action being confirmed */
  action: string;
  /** Item IDs to operate on */
  ids: string[];
  /** Additional metadata for the action */
  metadata?: Record<string, unknown>;
}

/**
 * Bulk operation progress
 */
export interface BulkOperationProgress {
  /** Current item being processed */
  current: number;
  /** Total items to process */
  total: number;
  /** Percentage complete (0-100) */
  percentage?: number;
  /** Current operation status message */
  message?: string;
  /** IDs of successfully processed items */
  succeeded?: string[];
  /** IDs of failed items */
  failed?: string[];
}

/**
 * Bulk operation state
 */
export interface BulkOperationState {
  /** Whether operation is in progress */
  isRunning: boolean;
  /** Progress information */
  progress: BulkOperationProgress;
  /** Error if operation failed */
  error?: Error;
}
