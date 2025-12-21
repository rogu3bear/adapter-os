"use client";

import type { ReactNode } from "react";

// ============================================================================
// Sort Types
// ============================================================================

/**
 * Sort direction for columns
 */
export type SortDirection = "asc" | "desc" | null;

/**
 * Sorting state for a single column
 */
export interface SortState {
  /** Column ID being sorted */
  columnId: string;
  /** Sort direction */
  direction: "asc" | "desc";
}

/**
 * Sorting state (nullable for no sort)
 */
export interface SortingState {
  /** Column ID being sorted */
  columnId: string | null;
  /** Sort direction */
  direction: SortDirection;
}

/**
 * Multi-column sort state for advanced sorting
 */
export type MultiSortState = SortState[];

// ============================================================================
// Filter Types
// ============================================================================

/**
 * Filter operator types for advanced filtering
 */
export type FilterOperator =
  | "equals"
  | "notEquals"
  | "contains"
  | "notContains"
  | "startsWith"
  | "endsWith"
  | "greaterThan"
  | "lessThan"
  | "greaterThanOrEqual"
  | "lessThanOrEqual"
  | "between"
  | "isEmpty"
  | "isNotEmpty";

/**
 * Column filter configuration
 */
export interface ColumnFilter<TData = unknown> {
  /** Unique filter identifier / column ID */
  id: string;
  /** Filter value */
  value: unknown;
  /** Filter operator (default: 'contains') */
  operator?: FilterOperator;
  /** Custom filter function */
  filterFn?: (row: TData, value: unknown) => boolean;
}

/**
 * Global filter state (searches across all filterable columns)
 */
export interface GlobalFilter {
  /** Global filter value */
  value: string;
  /** Column IDs to include in global filter (empty = all filterable) */
  columnIds?: string[];
}

/**
 * Combined filter state
 */
export interface FilterState<TData = unknown> {
  /** Per-column filters */
  columnFilters: ColumnFilter<TData>[];
  /** Global filter (optional) */
  globalFilter?: GlobalFilter;
}

// ============================================================================
// Pagination Types
// ============================================================================

/**
 * Pagination state for the DataTable
 */
export interface PaginationState {
  /** Current page index (0-based) */
  pageIndex: number;
  /** Number of rows per page */
  pageSize: number;
  /** Total number of rows (for server-side pagination) */
  totalRows?: number;
  /** Total number of pages (computed) */
  totalPages?: number;
  /** Available page size options */
  pageSizeOptions?: number[];
}

/**
 * Computed pagination metadata
 */
export interface PaginationMeta {
  /** Total number of pages */
  pageCount: number;
  /** Whether there is a previous page */
  canPreviousPage: boolean;
  /** Whether there is a next page */
  canNextPage: boolean;
  /** Index of first row on current page (1-based for display) */
  firstRowIndex: number;
  /** Index of last row on current page (1-based for display) */
  lastRowIndex: number;
}

// ============================================================================
// Selection Types
// ============================================================================

/**
 * Selection mode for DataTable
 */
export type SelectionMode = "none" | "single" | "multi";

/**
 * Row selection state
 */
export interface SelectionState<K = string> {
  /** Set of selected row IDs */
  selectedIds: Set<K>;
  /** Whether all rows are selected */
  isAllSelected: boolean;
  /** Whether some rows are selected (indeterminate) */
  isPartiallySelected: boolean;
}

/**
 * Legacy alias for SelectionState
 */
export type RowSelection = SelectionState<string>;

/**
 * Selection callbacks for row operations
 */
export interface SelectionCallbacks<TData, K = string> {
  /** Called when row selection changes */
  onSelectionChange?: (selection: SelectionState<K>, rows: TData[]) => void;
  /** Called when a single row is selected */
  onRowSelect?: (row: TData, id: K) => void;
  /** Called when a single row is deselected */
  onRowDeselect?: (row: TData, id: K) => void;
  /** Called when select all is toggled */
  onSelectAllChange?: (selected: boolean) => void;
}

// ============================================================================
// Column Types
// ============================================================================

/**
 * Cell context passed to cell renderers
 * Supports both TanStack Table v8 API (row.original) and direct property access
 */
export interface CellContext<TData, TValue = unknown> {
  /** Row data - includes both direct access and nested original for compatibility */
  row: TData & {
    /** Original row data (TanStack Table v8 API) */
    original: TData;
    /** Row index (TanStack Table v8 API) */
    index: number;
  };
  /** Row index (deprecated, use row.index) */
  rowIndex: number;
  /** Cell value from accessor */
  value: TValue;
  /** Column definition */
  column: Column<TData, TValue>;
  /** Get the current cell value */
  getValue: () => TValue;
}

/**
 * Column definition for DataTable
 * @template TData - The type of data in each row
 * @template TValue - The type of value returned by the accessor
 */
export interface Column<TData, TValue = unknown> {
  /** Unique column identifier */
  id: string;
  /** Column header - string or render function */
  header: string | (() => ReactNode);
  /** Accessor key for row data or accessor function */
  accessorKey?: keyof TData;
  /** Custom accessor function */
  accessorFn?: (row: TData, rowIndex: number) => TValue;
  /** Custom cell renderer */
  cell?: (info: CellContext<TData, TValue>) => ReactNode;
  /** Enable sorting for this column */
  sortable?: boolean;
  /** Enable filtering for this column */
  filterable?: boolean;
  /** Custom sort function */
  sortingFn?: (a: TData, b: TData, direction: SortDirection) => number;
  /** Custom filter matching function */
  filterMatcher?: (row: TData, filterValue: string, cellValue: TValue) => boolean;
  /** Column width (CSS value) */
  width?: string;
  /** Minimum column width (CSS value) */
  minWidth?: string;
  /** Maximum column width (CSS value) */
  maxWidth?: string;
  /** Column text alignment */
  align?: "left" | "center" | "right";
  /** Whether column is visible (default: true) */
  visible?: boolean;
  /** Whether the column can be hidden by user */
  hideable?: boolean;
  /** Whether the column can be resized */
  resizable?: boolean;
  /** Additional CSS classes for header */
  headerClassName?: string;
  /** Additional CSS classes for cells */
  cellClassName?: string;
  /** Sticky column position */
  sticky?: "left" | "right";
}

/**
 * Legacy alias for Column - deprecated, use Column instead
 * @deprecated Use Column<TData, TValue> instead
 */
export type ColumnDef<TData, TValue = unknown> = Column<TData, TValue> & {
  /** @deprecated Use sortable instead */
  enableSorting?: boolean;
  /** @deprecated Use filterable instead */
  enableFiltering?: boolean;
  /** @deprecated Use visible === false instead */
  hidden?: boolean;
};

// ============================================================================
// DataTable Props
// ============================================================================

/**
 * Server-side operations configuration
 */
export interface ServerSideConfig {
  /** Whether to use server-side sorting */
  sorting?: boolean;
  /** Whether to use server-side filtering */
  filtering?: boolean;
  /** Whether to use server-side pagination */
  pagination?: boolean;
}

/**
 * DataTable props
 * @template TData - The type of data in each row
 */
export interface DataTableProps<TData> {
  /** Array of row data */
  data: TData[];
  /** Column definitions */
  columns: Column<TData, unknown>[];
  /** Function to get unique row ID */
  getRowId: (row: TData) => string;

  // Selection
  /** Selection mode */
  selectionMode?: SelectionMode;
  /** Controlled selected row IDs */
  selectedIds?: Set<string>;
  /** Callback when selection changes */
  onSelectionChange?: (selectedIds: Set<string>) => void;
  /** Selection callbacks */
  selectionCallbacks?: SelectionCallbacks<TData, string>;

  // Sorting
  /** Enable sorting (default: true) */
  enableSorting?: boolean;
  /** Controlled sorting state */
  sorting?: SortingState;
  /** Callback when sorting changes */
  onSortingChange?: (sorting: SortingState) => void;
  /** Enable multi-column sorting */
  multiSort?: boolean;
  /** Multi-column sort state */
  multiSortState?: MultiSortState;
  /** Callback when multi-sort changes */
  onMultiSortChange?: (sorts: MultiSortState) => void;

  // Filtering
  /** Column filters */
  filters?: ColumnFilter<TData>[];
  /** Global filter value */
  globalFilter?: string;
  /** Callback when filters change */
  onFilterChange?: (filters: ColumnFilter<TData>[]) => void;
  /** Callback when global filter changes */
  onGlobalFilterChange?: (value: string) => void;
  /** Filter state (combined) */
  filterState?: FilterState<TData>;
  /** Callback when filter state changes */
  onFilterStateChange?: (state: FilterState<TData>) => void;
  /** Debounce delay for filter input (ms) */
  filterDebounce?: number;

  // Pagination
  /** Enable pagination (default: false - shows all rows) */
  enablePagination?: boolean;
  /** Pagination state */
  pagination?: PaginationState;
  /** Callback when pagination changes */
  onPaginationChange?: (pagination: PaginationState) => void;
  /** Available page sizes */
  pageSizes?: number[];

  // Server-side operations
  /** Configure which operations are server-side */
  serverSide?: ServerSideConfig;

  // States
  /** Loading state */
  isLoading?: boolean;
  /** Number of skeleton rows to show when loading */
  loadingRowCount?: number;
  /** Error state */
  error?: Error | string | null;
  /** Retry callback for errors */
  onRetry?: () => void;

  // Empty state
  /** Custom empty state content */
  emptyState?: ReactNode;
  /** Empty state title */
  emptyTitle?: string;
  /** Empty state description */
  emptyDescription?: string;
  /** Simple empty state message (alternative to emptyTitle/emptyDescription) */
  emptyStateMessage?: string;

  // Row interactions
  /** Callback when row is clicked */
  onRowClick?: (row: TData) => void;
  /** Callback when row is double-clicked */
  onRowDoubleClick?: (row: TData) => void;
  /** Custom row class name */
  rowClassName?: string | ((row: TData, index: number) => string);
  /** Whether rows are clickable (shows cursor) */
  clickableRows?: boolean;

  // Styling
  /** Additional class name for table container */
  className?: string;
  /** Dense mode (smaller padding) */
  dense?: boolean;
  /** Striped rows */
  striped?: boolean;
  /** Show borders */
  bordered?: boolean;
  /** Sticky header */
  stickyHeader?: boolean;
  /** Maximum height (enables vertical scroll) */
  maxHeight?: string;
  /** Whether to highlight rows on hover */
  hoverable?: boolean;

  // Column features
  /** Whether to show column visibility controls */
  columnVisibility?: boolean;
  /** Whether to enable column resizing */
  columnResizing?: boolean;
  /** Whether to enable column reordering */
  columnReordering?: boolean;

  // Export
  /** Whether to enable export functionality */
  exportEnabled?: boolean;
  /** Export formats available */
  exportFormats?: ("csv" | "json" | "xlsx")[];

  // Accessibility
  /** Table caption for screen readers */
  caption?: string;
  /** Aria label */
  ariaLabel?: string;
  /** Whether caption is visible */
  captionVisible?: boolean;
}

/**
 * DataTable ref interface
 */
export interface DataTableRef<TData> {
  /** Clear all selections */
  clearSelection: () => void;
  /** Select all rows */
  selectAll: () => void;
  /** Toggle row selection */
  toggleRow: (rowId: string) => void;
  /** Reset sorting */
  resetSorting: () => void;
  /** Reset filters */
  resetFilters: () => void;
  /** Reset pagination */
  resetPagination: () => void;
  /** Reset all state */
  reset: () => void;
  /** Get currently visible data */
  getVisibleData: () => TData[];
  /** Get selected rows */
  getSelectedRows: () => TData[];
  /** Scroll to row */
  scrollToRow: (rowId: string) => void;
}

// ============================================================================
// Hook Return Types
// ============================================================================

/**
 * Return type for useDataTable hook
 * @template TData - The type of data in each row
 */
export interface UseDataTableReturn<TData> {
  // Processed data
  /** Data after sorting, filtering, and pagination */
  processedData: TData[];
  /** Data after sorting and filtering (before pagination) */
  filteredData: TData[];
  /** Total count of filtered rows */
  totalFilteredRows: number;
  /** Total count of all rows */
  totalRows: number;

  // Sort state and actions
  sortState: SortingState;
  setSortState: (sort: SortingState) => void;
  toggleSort: (columnId: string) => void;
  clearSort: () => void;

  // Filter state and actions
  filterState: FilterState<TData>;
  setFilterState: (filter: FilterState<TData>) => void;
  setColumnFilter: (columnId: string, value: unknown, operator?: FilterOperator) => void;
  removeColumnFilter: (columnId: string) => void;
  setGlobalFilter: (value: string) => void;
  clearFilters: () => void;

  // Pagination state and actions
  paginationState: PaginationState;
  setPaginationState: (pagination: PaginationState) => void;
  setPageIndex: (index: number) => void;
  setPageSize: (size: number) => void;
  goToFirstPage: () => void;
  goToLastPage: () => void;
  goToNextPage: () => void;
  goToPreviousPage: () => void;
  paginationMeta: PaginationMeta;

  // Selection state and actions
  selectionState: SelectionState<string>;
  setSelectionState: (selection: SelectionState<string>) => void;
  selectRow: (id: string) => void;
  deselectRow: (id: string) => void;
  toggleRowSelection: (id: string) => void;
  selectAll: () => void;
  deselectAll: () => void;
  toggleSelectAll: () => void;
  isRowSelected: (id: string) => boolean;
  getSelectedRows: () => TData[];

  // Column visibility
  visibleColumns: Column<TData, unknown>[];
  hiddenColumnIds: Set<string>;
  toggleColumnVisibility: (columnId: string) => void;
  setColumnVisibility: (columnId: string, visible: boolean) => void;
  resetColumnVisibility: () => void;

  // Utilities
  reset: () => void;
  isLoading: boolean;
  isEmpty: boolean;
}

// ============================================================================
// Server-Side Integration Types
// ============================================================================

/**
 * Parameters for server-side data fetching
 */
export interface ServerSideParams<TData = unknown> {
  /** Current sort state */
  sort?: SortingState | null;
  /** Current filter state */
  filter?: FilterState<TData>;
  /** Current pagination state */
  pagination: PaginationState;
}

/**
 * Response shape for server-side data fetching
 */
export interface ServerSideResponse<TData> {
  /** Row data for current page */
  data: TData[];
  /** Total number of rows (for pagination) */
  total: number;
  /** Current page index */
  pageIndex: number;
  /** Page size used */
  pageSize: number;
}

/**
 * Query key factory for React Query integration
 */
export type QueryKeyFactory<TData = unknown> = (
  params: ServerSideParams<TData>
) => readonly unknown[];

/**
 * Query function type for React Query integration
 */
export type QueryFn<TData> = (
  params: ServerSideParams<TData>
) => Promise<ServerSideResponse<TData>>;
