/**
 * Bulk Action State Types
 * Types for managing bulk operations on multiple items
 */

export interface BulkOperationProgress {
  current: number;
  total: number;
  successCount: number;
  failureCount: number;
}

export interface BulkOperationResult<T = any> {
  success: boolean;
  item: T;
  error?: string;
}

export interface BulkOperationState<T = any> {
  selectedItems: T[];
  isProcessing: boolean;
  progress: BulkOperationProgress;
  results: BulkOperationResult<T>[];
  error?: string;
}

export type BulkOperationType =
  | 'delete'
  | 'archive'
  | 'export'
  | 'update'
  | 'move'
  | 'copy'
  | 'tag'
  | 'publish'
  | 'unpublish';

export interface BulkActionConfig<T = any> {
  type: BulkOperationType;
  label: string;
  icon?: React.ReactNode;
  confirmMessage?: string;
  warningMessage?: string;
  handler: (items: T[]) => Promise<BulkOperationResult<T>[]>;
  validateSelection?: (items: T[]) => boolean | string;
  maxItems?: number;
  requireConfirmation?: boolean;
}

export interface BulkSelectionState<T = any> {
  selectedIds: Set<string>;
  items: Map<string, T>;
  selectAll: boolean;
  excludedIds?: Set<string>;
}

export interface BulkActionHookResult<T = any> {
  state: BulkOperationState<T>;
  selectedCount: number;
  isSelected: (item: T) => boolean;
  toggleSelection: (item: T) => void;
  toggleAll: (items: T[]) => void;
  clearSelection: () => void;
  executeBulkAction: (config: BulkActionConfig<T>) => Promise<void>;
  reset: () => void;
}
