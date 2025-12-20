/**
 * Adapter-specific hooks
 *
 * This module contains specialized hooks for working with adapters,
 * including export functionality, bulk actions, and other adapter-related operations.
 */

// Adapter operations and lifecycle
export { useAdapterOperations } from './useAdapterOperations';
export type {
  UseAdapterOperationsOptions,
  UseAdapterOperationsReturn,
} from './useAdapterOperations';

export { useAdapterActions } from './useAdapterActions';
export type {
  AdapterActionType,
  AdapterActionTarget,
  InlineStatus,
  UseAdapterActionsOptions,
} from './useAdapterActions';

export { useAdapterDetail } from './useAdapterDetail';
export type {
  UseAdapterDetailOptions,
  UseAdapterDetailReturn,
} from './useAdapterDetail';
export { adapterDetailKeys } from './useAdapterDetail';

// Adapter API hooks
export {
  useAdaptersApi,
  useAdapters,
  useAdapter,
  useCreateAdapter,
  useDeleteAdapter,
  useLoadAdapter,
  useUnloadAdapter,
  usePinAdapter,
  useEvictAdapter,
  usePromoteAdapter,
  useImportAdapter,
  usePromoteAdapterLifecycle,
  useDemoteAdapterLifecycle,
  invalidateAdapters,
  adapterKeys,
} from './useAdaptersApi';

// Adapter publishing
export {
  usePublishAdapter,
  useArchiveAdapter,
  useUnarchiveAdapter,
} from './useAdapterPublish';
export { adapterPublishKeys } from './useAdapterPublish';

// Adapter export functionality
export { useAdapterExport } from './useAdapterExport';
export type {
  UseAdapterExportOptions,
  UseAdapterExportReturn,
  ExportFormat,
  ExportScope,
} from './useAdapterExport';

// Bulk actions
export { useAdapterBulkActions } from './useAdapterBulkActions';
// Types are exported from @/types/hooks/adapters, not from the hook file
export type {
  UseAdapterBulkActionsOptions,
  UseAdapterBulkActionsReturn,
  BulkOperationProgress,
  BulkActionConfirmationState,
} from '@/types/hooks/adapters';

// Dialogs and UI state
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

// Adapter filters hook
export { useAdapterFilters } from './useAdapterFilters';

// Additional adapter query hooks (alternative to useAdaptersApi)
export {
  useAdapters as useAdaptersQuery,
  useAdapterDetail as useAdapterDetailQuery,
  useAdapterHealth,
  useLoadAdapter as useLoadAdapterMutation,
  useUnloadAdapter as useUnloadAdapterMutation,
  useDeleteAdapter as useDeleteAdapterMutation,
  usePinAdapter as usePinAdapterMutation,
  usePromoteAdapter as usePromoteAdapterMutation,
  useEvictAdapter as useEvictAdapterMutation,
  ADAPTER_QUERY_KEYS,
} from './useAdapters';
export type { AdapterFilters as AdapterQueryFilters, AdaptersData } from './useAdapters';

// Adapter stack validation
export { useStackValidation } from './useStackValidation';
export type { ValidationIssue, ValidationReport } from './useStackValidation';
