/**
 * UI Hook Types
 *
 * Type definitions for UI-related hooks including pagination, selection,
 * filtering, sorting, bulk actions, and data loading.
 */

// ============================================================================
// usePagination Types
// ============================================================================

export interface PaginationOptions {
  /** Initial page number (default: 1) */
  initialPage?: number;
  /** Items per page (default: 10) */
  pageSize?: number;
  /** Total number of items */
  totalItems?: number;
}

export interface PaginationState {
  /** Current page number (1-indexed) */
  currentPage: number;
  /** Items per page */
  pageSize: number;
  /** Total number of pages */
  totalPages: number;
  /** Whether there is a previous page */
  hasPrevious: boolean;
  /** Whether there is a next page */
  hasNext: boolean;
  /** Zero-based offset for current page */
  offset: number;
}

export interface UsePaginationReturn extends PaginationState {
  /** Go to specific page */
  goToPage: (page: number) => void;
  /** Go to next page */
  nextPage: () => void;
  /** Go to previous page */
  previousPage: () => void;
  /** Go to first page */
  firstPage: () => void;
  /** Go to last page */
  lastPage: () => void;
  /** Update page size */
  setPageSize: (size: number) => void;
  /** Update total items */
  setTotalItems: (total: number) => void;
  /** Reset to initial state */
  reset: () => void;
}

// ============================================================================
// useSelection Types
// ============================================================================

export interface UseSelectionOptions<T, K extends string | number = string> {
  /** Items to select from */
  items?: T[];
  /** Key extractor function */
  getKey?: (item: T) => K;
  /** Initial selected keys */
  initialSelection?: K[];
  /** Selection mode */
  mode?: 'single' | 'multiple';
}

export interface UseSelectionReturn<T, K extends string | number = string> {
  /** Currently selected keys */
  selectedKeys: Set<K>;
  /** Selected items */
  selectedItems: T[];
  /** Whether a key is selected */
  isSelected: (key: K) => boolean;
  /** Select a key */
  select: (key: K) => void;
  /** Deselect a key */
  deselect: (key: K) => void;
  /** Toggle a key */
  toggle: (key: K) => void;
  /** Select all */
  selectAll: () => void;
  /** Clear selection */
  clearSelection: () => void;
  /** Select multiple keys */
  selectMultiple: (keys: K[]) => void;
  /** Number of selected items */
  selectionCount: number;
  /** Whether all items are selected */
  isAllSelected: boolean;
}

// ============================================================================
// useFilter Types
// ============================================================================

export interface UseFilterOptions<T, K extends string = string> {
  /** Items to filter */
  items: T[];
  /** Filter function */
  filterFn?: (item: T, filters: Record<K, unknown>) => boolean;
  /** Initial filters */
  initialFilters?: Record<K, unknown>;
}

export interface UseFilterReturn<T, K extends string = string> {
  /** Filtered items */
  filteredItems: T[];
  /** Current filters */
  filters: Record<K, unknown>;
  /** Set a filter */
  setFilter: (key: K, value: unknown) => void;
  /** Set multiple filters */
  setFilters: (filters: Record<K, unknown>) => void;
  /** Clear a filter */
  clearFilter: (key: K) => void;
  /** Clear all filters */
  clearAllFilters: () => void;
  /** Active filter count */
  activeFilterCount: number;
  /** Whether filters are active */
  hasActiveFilters: boolean;
}

// ============================================================================
// useSort Types
// ============================================================================

export interface UseSortOptions<T, K extends string = string> {
  /** Items to sort */
  items: T[];
  /** Initial sort key */
  initialSortKey?: K;
  /** Initial sort direction */
  initialDirection?: 'asc' | 'desc';
  /** Custom comparator */
  compareFn?: (a: T, b: T, key: K) => number;
}

export interface UseSortReturn<T, K extends string = string> {
  /** Sorted items */
  sortedItems: T[];
  /** Current sort key */
  sortKey: K | null;
  /** Current sort direction */
  sortDirection: 'asc' | 'desc';
  /** Sort by key */
  sortBy: (key: K) => void;
  /** Toggle sort direction for current key */
  toggleDirection: () => void;
  /** Clear sort */
  clearSort: () => void;
}

// ============================================================================
// useBulkActions Types
// ============================================================================

export type BulkActionStatus = 'idle' | 'pending' | 'executing' | 'completed' | 'failed' | 'cancelled';

export interface BulkActionError<K extends string | number = string> {
  /** ID of the item that failed */
  itemId: K;
  /** Error that occurred */
  error: Error;
  /** Index in the batch */
  index: number;
}

export interface BulkActionProgress {
  /** Total number of items to process */
  total: number;
  /** Number of completed items (success + failed) */
  completed: number;
  /** Number of successful operations */
  successful: number;
  /** Number of failed operations */
  failed: number;
  /** Current progress percentage (0-100) */
  percentage: number;
  /** ID of the item currently being processed */
  currentItemId: string | number | null;
}

export interface BulkActionResult<K extends string | number = string> {
  /** Items that were successfully processed */
  successfulIds: K[];
  /** Items that failed with their errors */
  failedItems: BulkActionError<K>[];
  /** Whether the operation was cancelled */
  wasCancelled: boolean;
  /** Total execution time in milliseconds */
  executionTimeMs: number;
}

export interface BulkActionOptions {
  /** Stop execution on first error */
  stopOnError?: boolean;
  /** Maximum concurrent operations (default: 1 for sequential) */
  concurrency?: number;
  /** Delay between operations in ms (default: 0) */
  delayBetweenOps?: number;
  /** Confirmation required before execution */
  confirmationRequired?: boolean;
  /** Custom confirmation message */
  confirmationMessage?: string;
  /** Operation name for logging */
  operationName?: string;
}

export interface UseBulkActionsOptions<K extends string | number = string> {
  /** Callback on successful completion */
  onSuccess?: (result: BulkActionResult<K>) => void;
  /** Callback on error (called for each error and on completion if any errors) */
  onError?: (errors: BulkActionError<K>[]) => void;
  /** Callback on progress update */
  onProgress?: (progress: BulkActionProgress) => void;
  /** Callback when execution starts */
  onStart?: (itemCount: number) => void;
  /** Callback on cancellation */
  onCancel?: () => void;
  /** Component name for logging */
  componentName?: string;
}

export interface UseBulkActionsReturn<K extends string | number = string> {
  /** Current status of bulk operation */
  status: BulkActionStatus;
  /** Current progress */
  progress: BulkActionProgress;
  /** Whether operation is currently executing */
  isExecuting: boolean;
  /** Most recent result */
  result: BulkActionResult<K> | null;
  /** Execute bulk operation */
  execute: <T = void>(
    itemIds: K[],
    operation: (id: K, index: number) => Promise<T>,
    options?: BulkActionOptions
  ) => Promise<BulkActionResult<K>>;
  /** Cancel ongoing operation */
  cancel: () => void;
  /** Reset state to idle */
  reset: () => void;
  /** Check if operation can be cancelled */
  canCancel: boolean;
  /** Errors from the most recent operation */
  errors: BulkActionError<K>[];
}

// ============================================================================
// useDataLoader Types
// ============================================================================

export interface UseDataLoaderOptions<T> {
  /** Function to load data */
  loadFn: () => Promise<T>;
  /** Enable/disable auto-load */
  enabled?: boolean;
  /** Auto-reload on mount */
  autoReload?: boolean;
  /** Callback on load success */
  onSuccess?: (data: T) => void;
  /** Callback on load error */
  onError?: (error: Error) => void;
}

export interface UseDataLoaderReturn<T> {
  /** Loaded data */
  data: T | null;
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
  /** Reload data */
  reload: () => Promise<void>;
  /** Reset state */
  reset: () => void;
}

// ============================================================================
// useConfirmation Types
// ============================================================================

export interface ConfirmationOptions {
  /** Confirmation title */
  title?: string;
  /** Confirmation message */
  message?: string;
  /** Confirm button text */
  confirmText?: string;
  /** Cancel button text */
  cancelText?: string;
  /** Confirm button variant */
  confirmVariant?: 'default' | 'destructive' | 'primary';
}

export interface UseConfirmationReturn {
  /** Whether confirmation dialog is open */
  isOpen: boolean;
  /** Current confirmation options */
  options: ConfirmationOptions | null;
  /** Request confirmation */
  confirm: (options: ConfirmationOptions) => Promise<boolean>;
  /** Close dialog and confirm */
  handleConfirm: () => void;
  /** Close dialog and cancel */
  handleCancel: () => void;
}

// ============================================================================
// useDebouncedValue Types
// ============================================================================

export interface UseDebouncedValueOptions {
  /** Delay in milliseconds */
  delay?: number;
  /** Leading edge (call immediately then debounce) */
  leading?: boolean;
}

export interface UseDebouncedValueReturn<T> {
  /** Debounced value */
  debouncedValue: T;
  /** Whether debounce is pending */
  isPending: boolean;
  /** Cancel pending debounce */
  cancel: () => void;
  /** Flush pending debounce immediately */
  flush: () => void;
}

// ============================================================================
// useInfiniteScroll Types
// ============================================================================

export type PageParam = number | string | null | undefined;

export interface UseInfiniteScrollOptions<TItem, TPageParam = PageParam> {
  /** Function to fetch page */
  fetchPage: (pageParam: TPageParam) => Promise<{ items: TItem[]; nextPage: TPageParam | null }>;
  /** Initial page param */
  initialPageParam?: TPageParam;
  /** Enable/disable */
  enabled?: boolean;
  /** Callback when page loads */
  onPageLoad?: (items: TItem[], pageParam: TPageParam) => void;
}

export interface UseInfiniteScrollReturn<TItem> {
  /** All items */
  items: TItem[];
  /** Loading state */
  isLoading: boolean;
  /** Loading more state */
  isLoadingMore: boolean;
  /** Error state */
  error: Error | null;
  /** Whether more items are available */
  hasMore: boolean;
  /** Load next page */
  loadMore: () => Promise<void>;
  /** Reset to first page */
  reset: () => void;
  /** Ref to attach to sentinel element */
  sentinelRef: (node: HTMLElement | null) => void;
}

// ============================================================================
// useProgressOperation Types
// ============================================================================

export interface UseProgressOperationReturn {
  /** Whether operation is in progress */
  isInProgress: boolean;
  /** Current progress (0-100) */
  progress: number;
  /** Status message */
  message: string;
  /** Start operation */
  start: (message?: string) => void;
  /** Update progress */
  updateProgress: (progress: number, message?: string) => void;
  /** Complete operation */
  complete: (message?: string) => void;
  /** Cancel operation */
  cancel: (message?: string) => void;
  /** Reset state */
  reset: () => void;
}

// ============================================================================
// useInformationDensity Types
// ============================================================================

export type InformationDensity = 'compact' | 'normal' | 'comfortable';

export interface UseInformationDensityReturn {
  /** Current density */
  density: InformationDensity;
  /** Set density */
  setDensity: (density: InformationDensity) => void;
  /** Toggle between densities */
  toggleDensity: () => void;
  /** CSS class for current density */
  densityClass: string;
}
