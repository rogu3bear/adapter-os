/**
 * Adapter-specific hooks
 *
 * This module contains specialized hooks for working with adapters,
 * including export functionality, bulk actions, and other adapter-related operations.
 */

export { useAdapterExport } from './useAdapterExport';
export type {
  UseAdapterExportOptions,
  UseAdapterExportReturn,
  ExportFormat,
  ExportScope,
} from './useAdapterExport';

export { useAdapterBulkActions } from './useAdapterBulkActions';
export type {
  UseAdapterBulkActionsOptions,
  UseAdapterBulkActionsReturn,
  BulkOperationProgress,
  BulkActionConfirmationState,
} from './useAdapterBulkActions';

export { useAdapterDialogs } from './useAdapterDialogs';
export type {
  UseAdapterDialogsReturn,
  DialogType,
  DialogState,
  DialogDataTypes,
} from './useAdapterDialogs';

export { useAdapterFilterState } from './useAdapterFilterState';
export type {
  AdapterFilters,
  AdapterSortColumn,
  AdapterSortDirection,
  AdapterSortState,
  UseAdapterFilterStateReturn,
} from './useAdapterFilterState';
