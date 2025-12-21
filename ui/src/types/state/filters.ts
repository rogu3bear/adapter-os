/**
 * Filter and Sort State Types
 *
 * Generic filter, sort, and pagination state management.
 *
 * Citations:
 * - ui/src/hooks/adapters/useAdapterFilterState.ts - Adapter filtering
 * - ui/src/hooks/ui/usePagination.ts - Pagination state
 */

/**
 * Generic sort state
 */
export interface SortState<T extends string = string> {
  /** Column/field to sort by */
  column: T;
  /** Sort direction */
  direction: 'asc' | 'desc';
}

/**
 * Sort actions
 */
export interface SortActions<T extends string = string> {
  /** Set sort column and direction */
  setSort: (column: T, direction?: 'asc' | 'desc') => void;
  /** Toggle sort direction for current column */
  toggleDirection: () => void;
  /** Change sort column (resets to ascending) */
  changeColumn: (column: T) => void;
  /** Reset to default sort */
  resetSort: () => void;
}

/**
 * Generic filter state
 */
export interface FilterState<T = Record<string, unknown>> {
  /** Active filters */
  filters: T;
  /** Whether any filters are active */
  hasActiveFilters: boolean;
}

/**
 * Filter actions
 */
export interface FilterActions<T = Record<string, unknown>> {
  /** Update specific filter fields */
  updateFilters: (updates: Partial<T>) => void;
  /** Clear specific filter field */
  clearFilter: (key: keyof T) => void;
  /** Reset all filters to defaults */
  resetFilters: () => void;
  /** Apply filters to data */
  applyFilters: <D>(data: D[]) => D[];
}

/**
 * Search state
 */
export interface SearchState {
  /** Search query string */
  query: string;
  /** Whether search is active */
  isSearching: boolean;
  /** Debounced query value */
  debouncedQuery?: string;
}

/**
 * Search actions
 */
export interface SearchActions {
  /** Set search query */
  setQuery: (query: string) => void;
  /** Clear search */
  clearSearch: () => void;
}

/**
 * Pagination state
 */
export interface PaginationState {
  /** Current page (1-indexed) */
  currentPage: number;
  /** Items per page */
  pageSize: number;
  /** Total number of items */
  totalItems: number;
  /** Total number of pages */
  totalPages: number;
  /** Start index for current page (0-indexed) */
  startIndex: number;
  /** End index for current page (exclusive) */
  endIndex: number;
  /** Whether there's a previous page */
  hasPreviousPage: boolean;
  /** Whether there's a next page */
  hasNextPage: boolean;
  /** Whether currently on first page */
  isFirstPage: boolean;
  /** Whether currently on last page */
  isLastPage: boolean;
}

/**
 * Pagination actions
 */
export interface PaginationActions {
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
  /** Change page size */
  setPageSize: (size: number) => void;
  /** Update total items count */
  setTotalItems: (total: number) => void;
  /** Reset to initial state */
  reset: () => void;
}

/**
 * Combined filter, sort, and pagination state
 */
export interface FilteredListState<
  TFilter = Record<string, unknown>,
  TSortColumn extends string = string
> {
  searchState: SearchState;
  filterState: FilterState<TFilter>;
  sortState: SortState<TSortColumn>;
  paginationState: PaginationState;
}

/**
 * Combined filter, sort, and pagination actions
 */
export interface FilteredListActions<
  TFilter = Record<string, unknown>,
  TSortColumn extends string = string
> {
  searchActions: SearchActions;
  filterActions: FilterActions<TFilter>;
  sortActions: SortActions<TSortColumn>;
  paginationActions: PaginationActions;
  /** Apply all filters, sort, and return paginated results */
  applyAll: <D>(data: D[]) => D[];
}

/**
 * Complete filtered list state with actions
 */
export interface FilteredListStateWithActions<
  TFilter = Record<string, unknown>,
  TSortColumn extends string = string
> extends FilteredListState<TFilter, TSortColumn>,
    FilteredListActions<TFilter, TSortColumn> {}

/**
 * Filter preset (saved filter configuration)
 */
export interface FilterPreset<T = Record<string, unknown>> {
  id: string;
  name: string;
  description?: string;
  filters: T;
  isDefault?: boolean;
  createdAt?: string;
}

/**
 * Filter preset manager
 */
export interface FilterPresetManager<T = Record<string, unknown>> {
  /** Available presets */
  presets: FilterPreset<T>[];
  /** Currently active preset */
  activePreset?: FilterPreset<T>;
  /** Apply preset */
  applyPreset: (presetId: string) => void;
  /** Save current filters as preset */
  savePreset: (name: string, description?: string) => void;
  /** Delete preset */
  deletePreset: (presetId: string) => void;
}
