"use client";

import { useCallback, useMemo, useState, useEffect, useRef } from "react";
import { useQuery, useQueryClient, type UseQueryOptions } from "@tanstack/react-query";
import type {
  Column,
  ColumnFilter,
  FilterOperator,
  FilterState,
  PaginationMeta,
  PaginationState,
  SelectionState,
  ServerSideParams,
  ServerSideResponse,
  SortDirection,
  SortingState,
  UseDataTableReturn,
} from "./types";

// ============================================================================
// Configuration Types
// ============================================================================

/**
 * Configuration options for useDataTable hook
 */
export interface UseDataTableOptions<TData> {
  /** Initial data array */
  data: TData[];

  /** Column definitions */
  columns: Column<TData, unknown>[];

  /** Function to extract unique row ID */
  getRowId: (row: TData) => string;

  /** Initial sort state */
  initialSortState?: SortingState;

  /** Initial filter state */
  initialFilterState?: FilterState<TData>;

  /** Initial pagination state */
  initialPaginationState?: Partial<PaginationState>;

  /** Initial selection state */
  initialSelectionState?: SelectionState<string>;

  /** Default page size */
  defaultPageSize?: number;

  /** Available page sizes */
  pageSizeOptions?: number[];

  /** Enable pagination (client-side) */
  enablePagination?: boolean;

  /** Debounce delay for filter input (ms) */
  filterDebounce?: number;

  /** Callback when sort state changes */
  onSortChange?: (sort: SortingState) => void;

  /** Callback when filter state changes */
  onFilterChange?: (filter: FilterState<TData>) => void;

  /** Callback when pagination state changes */
  onPaginationChange?: (pagination: PaginationState) => void;

  /** Callback when selection state changes */
  onSelectionChange?: (selection: SelectionState<string>) => void;

  /** External loading state */
  isLoading?: boolean;
}

/**
 * Configuration for server-side operations with React Query
 */
export interface UseDataTableServerOptions<TData> {
  /** Column definitions */
  columns: Column<TData, unknown>[];

  /** Function to extract unique row ID */
  getRowId: (row: TData) => string;

  /** Query key base for React Query */
  queryKey: readonly unknown[];

  /** Query function for fetching data */
  queryFn: (params: ServerSideParams<TData>) => Promise<ServerSideResponse<TData>>;

  /** Initial sort state */
  initialSortState?: SortingState;

  /** Initial filter state */
  initialFilterState?: FilterState<TData>;

  /** Initial pagination state */
  initialPaginationState?: Partial<PaginationState>;

  /** Initial selection state */
  initialSelectionState?: SelectionState<string>;

  /** Default page size */
  defaultPageSize?: number;

  /** Available page sizes */
  pageSizeOptions?: number[];

  /** Debounce delay for filter input (ms) */
  filterDebounce?: number;

  /** React Query options */
  queryOptions?: Omit<UseQueryOptions<ServerSideResponse<TData>>, "queryKey" | "queryFn">;

  /** Callback when sort state changes */
  onSortChange?: (sort: SortingState) => void;

  /** Callback when filter state changes */
  onFilterChange?: (filter: FilterState<TData>) => void;

  /** Callback when pagination state changes */
  onPaginationChange?: (pagination: PaginationState) => void;

  /** Callback when selection state changes */
  onSelectionChange?: (selection: SelectionState<string>) => void;
}

// ============================================================================
// Default Values
// ============================================================================

const DEFAULT_SORT_STATE: SortingState = {
  columnId: null,
  direction: null,
};

const DEFAULT_FILTER_STATE: FilterState<unknown> = {
  columnFilters: [],
  globalFilter: undefined,
};

const DEFAULT_PAGE_SIZE = 10;
const DEFAULT_PAGE_SIZE_OPTIONS = [10, 25, 50, 100];
const DEFAULT_FILTER_DEBOUNCE = 300;

// ============================================================================
// Utility Functions
// ============================================================================

/**
 * Get cell value from row using column accessor
 */
function getCellValue<TData, TValue>(
  row: TData,
  column: Column<TData, TValue>,
  rowIndex: number
): TValue | null {
  if (column.accessorFn) {
    return column.accessorFn(row, rowIndex);
  }
  if (column.accessorKey) {
    return row[column.accessorKey] as TValue;
  }
  return null;
}

/**
 * Apply filter operator to compare values
 */
function applyFilterOperator(
  cellValue: unknown,
  filterValue: unknown,
  operator: FilterOperator = "contains"
): boolean {
  const cellStr = String(cellValue ?? "").toLowerCase();
  const filterStr = String(filterValue ?? "").toLowerCase();

  switch (operator) {
    case "equals":
      return cellStr === filterStr;
    case "notEquals":
      return cellStr !== filterStr;
    case "contains":
      return cellStr.includes(filterStr);
    case "notContains":
      return !cellStr.includes(filterStr);
    case "startsWith":
      return cellStr.startsWith(filterStr);
    case "endsWith":
      return cellStr.endsWith(filterStr);
    case "greaterThan":
      return Number(cellValue) > Number(filterValue);
    case "lessThan":
      return Number(cellValue) < Number(filterValue);
    case "greaterThanOrEqual":
      return Number(cellValue) >= Number(filterValue);
    case "lessThanOrEqual":
      return Number(cellValue) <= Number(filterValue);
    case "between": {
      const [min, max] = Array.isArray(filterValue) ? filterValue : [0, 0];
      const numValue = Number(cellValue);
      return numValue >= Number(min) && numValue <= Number(max);
    }
    case "isEmpty":
      return cellValue == null || cellStr === "";
    case "isNotEmpty":
      return cellValue != null && cellStr !== "";
    default:
      return cellStr.includes(filterStr);
  }
}

/**
 * Compare two values for sorting
 */
function compareValues(a: unknown, b: unknown, direction: SortDirection): number {
  if (a == null && b == null) return 0;
  if (a == null) return direction === "asc" ? 1 : -1;
  if (b == null) return direction === "asc" ? -1 : 1;

  let comparison = 0;

  if (typeof a === "string" && typeof b === "string") {
    comparison = a.localeCompare(b);
  } else if (typeof a === "number" && typeof b === "number") {
    comparison = a - b;
  } else if (a instanceof Date && b instanceof Date) {
    comparison = a.getTime() - b.getTime();
  } else if (typeof a === "boolean" && typeof b === "boolean") {
    comparison = (a ? 1 : 0) - (b ? 1 : 0);
  } else {
    comparison = String(a).localeCompare(String(b));
  }

  return direction === "asc" ? comparison : -comparison;
}

/**
 * Create an empty selection state
 */
function createEmptySelectionState(): SelectionState<string> {
  return {
    selectedIds: new Set<string>(),
    isAllSelected: false,
    isPartiallySelected: false,
  };
}

/**
 * Compute selection state from selected IDs
 */
function computeSelectionState(
  selectedIds: Set<string>,
  allRowIds: Set<string>
): SelectionState<string> {
  const isAllSelected =
    allRowIds.size > 0 && Array.from(allRowIds).every((id) => selectedIds.has(id));
  const isPartiallySelected =
    !isAllSelected && Array.from(allRowIds).some((id) => selectedIds.has(id));

  return {
    selectedIds,
    isAllSelected,
    isPartiallySelected,
  };
}

// ============================================================================
// Debounce Hook
// ============================================================================

function useDebouncedValue<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value);

  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedValue(value);
    }, delay);

    return () => {
      clearTimeout(timer);
    };
  }, [value, delay]);

  return debouncedValue;
}

// ============================================================================
// Main Hook: useDataTable (Client-side)
// ============================================================================

/**
 * Hook for managing DataTable state with client-side data processing
 *
 * @template TData - The type of data in each row
 *
 * @example
 * ```tsx
 * const {
 *   processedData,
 *   sortState,
 *   toggleSort,
 *   filterState,
 *   setGlobalFilter,
 *   paginationState,
 *   setPageIndex,
 *   selectionState,
 *   toggleRowSelection,
 * } = useDataTable({
 *   data: users,
 *   columns,
 *   getRowId: (row) => row.id,
 * });
 * ```
 */
export function useDataTable<TData>(
  options: UseDataTableOptions<TData>
): UseDataTableReturn<TData> {
  const {
    data,
    columns,
    getRowId,
    initialSortState = DEFAULT_SORT_STATE,
    initialFilterState = DEFAULT_FILTER_STATE as FilterState<TData>,
    initialPaginationState,
    initialSelectionState,
    defaultPageSize = DEFAULT_PAGE_SIZE,
    pageSizeOptions = DEFAULT_PAGE_SIZE_OPTIONS,
    enablePagination = true,
    filterDebounce = DEFAULT_FILTER_DEBOUNCE,
    onSortChange,
    onFilterChange,
    onPaginationChange,
    onSelectionChange,
    isLoading = false,
  } = options;

  // ---- Sort State ----
  const [sortState, setSortStateInternal] = useState<SortingState>(initialSortState);

  const setSortState = useCallback(
    (newSort: SortingState) => {
      setSortStateInternal(newSort);
      onSortChange?.(newSort);
    },
    [onSortChange]
  );

  const toggleSort = useCallback(
    (columnId: string) => {
      setSortStateInternal((prev) => {
        let newSort: SortingState;
        if (prev.columnId !== columnId) {
          newSort = { columnId, direction: "asc" };
        } else if (prev.direction === "asc") {
          newSort = { columnId, direction: "desc" };
        } else {
          newSort = { columnId: null, direction: null };
        }
        onSortChange?.(newSort);
        return newSort;
      });
    },
    [onSortChange]
  );

  const clearSort = useCallback(() => {
    setSortState({ columnId: null, direction: null });
  }, [setSortState]);

  // ---- Filter State ----
  const [filterState, setFilterStateInternal] = useState<FilterState<TData>>(initialFilterState);
  const debouncedFilterState = useDebouncedValue(filterState, filterDebounce);

  const setFilterState = useCallback(
    (newFilter: FilterState<TData>) => {
      setFilterStateInternal(newFilter);
      onFilterChange?.(newFilter);
    },
    [onFilterChange]
  );

  const setColumnFilter = useCallback(
    (columnId: string, value: unknown, operator?: FilterOperator) => {
      setFilterStateInternal((prev) => {
        const existingIndex = prev.columnFilters.findIndex((f) => f.id === columnId);
        const newFilter: ColumnFilter<TData> = { id: columnId, value, operator };

        let result: FilterState<TData>;
        if (value === "" || value == null) {
          // Remove filter if value is empty
          result = {
            ...prev,
            columnFilters: prev.columnFilters.filter((f) => f.id !== columnId),
          };
        } else if (existingIndex >= 0) {
          const newFilters = [...prev.columnFilters];
          newFilters[existingIndex] = newFilter;
          result = { ...prev, columnFilters: newFilters };
        } else {
          result = { ...prev, columnFilters: [...prev.columnFilters, newFilter] };
        }

        onFilterChange?.(result);
        return result;
      });
    },
    [onFilterChange]
  );

  const removeColumnFilter = useCallback(
    (columnId: string) => {
      setFilterStateInternal((prev) => {
        const result = {
          ...prev,
          columnFilters: prev.columnFilters.filter((f) => f.id !== columnId),
        };
        onFilterChange?.(result);
        return result;
      });
    },
    [onFilterChange]
  );

  const setGlobalFilter = useCallback(
    (value: string) => {
      setFilterStateInternal((prev) => {
        const result = {
          ...prev,
          globalFilter: value ? { value } : undefined,
        };
        onFilterChange?.(result);
        return result;
      });
    },
    [onFilterChange]
  );

  const clearFilters = useCallback(() => {
    setFilterState({ columnFilters: [], globalFilter: undefined });
  }, [setFilterState]);

  // ---- Pagination State ----
  const [paginationState, setPaginationStateInternal] = useState<PaginationState>({
    pageIndex: initialPaginationState?.pageIndex ?? 0,
    pageSize: initialPaginationState?.pageSize ?? defaultPageSize,
    totalRows: data.length,
    totalPages: Math.ceil(data.length / (initialPaginationState?.pageSize ?? defaultPageSize)),
    pageSizeOptions,
  });

  const setPaginationState = useCallback(
    (newPagination: PaginationState) => {
      setPaginationStateInternal(newPagination);
      onPaginationChange?.(newPagination);
    },
    [onPaginationChange]
  );

  const setPageIndex = useCallback(
    (index: number) => {
      setPaginationStateInternal((prev) => {
        const maxIndex = Math.max(0, (prev.totalPages ?? 1) - 1);
        const safeIndex = Math.max(0, Math.min(index, maxIndex));
        const result = { ...prev, pageIndex: safeIndex };
        onPaginationChange?.(result);
        return result;
      });
    },
    [onPaginationChange]
  );

  const setPageSize = useCallback(
    (size: number) => {
      setPaginationStateInternal((prev) => {
        const result = {
          ...prev,
          pageSize: size,
          pageIndex: 0, // Reset to first page
          totalPages: Math.ceil((prev.totalRows ?? 0) / size),
        };
        onPaginationChange?.(result);
        return result;
      });
    },
    [onPaginationChange]
  );

  const goToFirstPage = useCallback(() => setPageIndex(0), [setPageIndex]);
  const goToLastPage = useCallback(
    () => setPageIndex((paginationState.totalPages ?? 1) - 1),
    [setPageIndex, paginationState.totalPages]
  );
  const goToNextPage = useCallback(
    () => setPageIndex(paginationState.pageIndex + 1),
    [setPageIndex, paginationState.pageIndex]
  );
  const goToPreviousPage = useCallback(
    () => setPageIndex(paginationState.pageIndex - 1),
    [setPageIndex, paginationState.pageIndex]
  );

  // ---- Selection State ----
  const [selectionState, setSelectionStateInternal] = useState<SelectionState<string>>(
    initialSelectionState ?? createEmptySelectionState()
  );

  const allRowIds = useMemo(
    () => new Set(data.map((row) => getRowId(row))),
    [data, getRowId]
  );

  const setSelectionState = useCallback(
    (newSelection: SelectionState<string>) => {
      setSelectionStateInternal(newSelection);
      onSelectionChange?.(newSelection);
    },
    [onSelectionChange]
  );

  const selectRow = useCallback(
    (id: string) => {
      const newSelectedIds = new Set(selectionState.selectedIds);
      newSelectedIds.add(id);
      setSelectionState(computeSelectionState(newSelectedIds, allRowIds));
    },
    [selectionState.selectedIds, allRowIds, setSelectionState]
  );

  const deselectRow = useCallback(
    (id: string) => {
      const newSelectedIds = new Set(selectionState.selectedIds);
      newSelectedIds.delete(id);
      setSelectionState(computeSelectionState(newSelectedIds, allRowIds));
    },
    [selectionState.selectedIds, allRowIds, setSelectionState]
  );

  const toggleRowSelection = useCallback(
    (id: string) => {
      if (selectionState.selectedIds.has(id)) {
        deselectRow(id);
      } else {
        selectRow(id);
      }
    },
    [selectionState.selectedIds, selectRow, deselectRow]
  );

  const selectAll = useCallback(() => {
    setSelectionState(computeSelectionState(new Set(allRowIds), allRowIds));
  }, [allRowIds, setSelectionState]);

  const deselectAll = useCallback(() => {
    setSelectionState(createEmptySelectionState());
  }, [setSelectionState]);

  const toggleSelectAll = useCallback(() => {
    if (selectionState.isAllSelected) {
      deselectAll();
    } else {
      selectAll();
    }
  }, [selectionState.isAllSelected, selectAll, deselectAll]);

  const isRowSelected = useCallback(
    (id: string) => selectionState.selectedIds.has(id),
    [selectionState.selectedIds]
  );

  const getSelectedRows = useCallback(
    () => data.filter((row) => selectionState.selectedIds.has(getRowId(row))),
    [data, selectionState.selectedIds, getRowId]
  );

  // ---- Column Visibility ----
  const [hiddenColumnIds, setHiddenColumnIds] = useState<Set<string>>(
    new Set(columns.filter((col) => col.visible === false).map((col) => col.id))
  );

  const visibleColumns = useMemo(
    () => columns.filter((col) => !hiddenColumnIds.has(col.id)),
    [columns, hiddenColumnIds]
  );

  const toggleColumnVisibility = useCallback((columnId: string) => {
    setHiddenColumnIds((prev) => {
      const next = new Set(prev);
      if (next.has(columnId)) {
        next.delete(columnId);
      } else {
        next.add(columnId);
      }
      return next;
    });
  }, []);

  const setColumnVisibility = useCallback((columnId: string, visible: boolean) => {
    setHiddenColumnIds((prev) => {
      const next = new Set(prev);
      if (visible) {
        next.delete(columnId);
      } else {
        next.add(columnId);
      }
      return next;
    });
  }, []);

  const resetColumnVisibility = useCallback(() => {
    setHiddenColumnIds(
      new Set(columns.filter((col) => col.visible === false).map((col) => col.id))
    );
  }, [columns]);

  // ---- Data Processing ----
  const filteredData = useMemo(() => {
    let result = [...data];

    // Apply global filter
    const globalFilterValue = debouncedFilterState.globalFilter?.value;
    if (globalFilterValue) {
      const searchTerm = globalFilterValue.toLowerCase();
      const filterColumnIds = debouncedFilterState.globalFilter?.columnIds;

      result = result.filter((row) => {
        const columnsToSearch = filterColumnIds
          ? columns.filter((col) => filterColumnIds.includes(col.id))
          : columns.filter((col) => col.filterable !== false);

        return columnsToSearch.some((column) => {
          const value = getCellValue(row, column, 0);
          if (value == null) return false;
          return String(value).toLowerCase().includes(searchTerm);
        });
      });
    }

    // Apply column filters
    for (const filter of debouncedFilterState.columnFilters) {
      const column = columns.find((col) => col.id === filter.id);
      if (!column) continue;

      result = result.filter((row, rowIndex) => {
        if (filter.filterFn) {
          return filter.filterFn(row, filter.value);
        }

        const cellValue = getCellValue(row, column, rowIndex);

        if (column.filterMatcher) {
          return column.filterMatcher(row, String(filter.value), cellValue);
        }

        return applyFilterOperator(cellValue, filter.value, filter.operator);
      });
    }

    return result;
  }, [data, debouncedFilterState, columns]);

  // Apply sorting
  const sortedData = useMemo(() => {
    if (!sortState.columnId || !sortState.direction) {
      return filteredData;
    }

    const column = columns.find((col) => col.id === sortState.columnId);
    if (!column) return filteredData;

    return [...filteredData].sort((a, b) => {
      if (column.sortingFn) {
        return column.sortingFn(a, b, sortState.direction);
      }

      const aValue = getCellValue(a, column, 0);
      const bValue = getCellValue(b, column, 0);

      return compareValues(aValue, bValue, sortState.direction);
    });
  }, [filteredData, sortState, columns]);

  // Apply pagination
  const processedData = useMemo(() => {
    if (!enablePagination) {
      return sortedData;
    }

    const start = paginationState.pageIndex * paginationState.pageSize;
    const end = start + paginationState.pageSize;
    return sortedData.slice(start, end);
  }, [sortedData, paginationState, enablePagination]);

  // Update pagination totals when filtered data changes
  useEffect(() => {
    setPaginationStateInternal((prev) => ({
      ...prev,
      totalRows: sortedData.length,
      totalPages: Math.ceil(sortedData.length / prev.pageSize),
    }));
  }, [sortedData.length]);

  // Compute pagination metadata
  const paginationMeta: PaginationMeta = useMemo(() => {
    const totalRows = paginationState.totalRows ?? sortedData.length;
    const pageCount = Math.ceil(totalRows / paginationState.pageSize);
    const firstRowIndex = paginationState.pageIndex * paginationState.pageSize + 1;
    const lastRowIndex = Math.min(
      (paginationState.pageIndex + 1) * paginationState.pageSize,
      totalRows
    );

    return {
      pageCount,
      canPreviousPage: paginationState.pageIndex > 0,
      canNextPage: paginationState.pageIndex < pageCount - 1,
      firstRowIndex: totalRows > 0 ? firstRowIndex : 0,
      lastRowIndex,
    };
  }, [paginationState, sortedData.length]);

  // ---- Reset ----
  const reset = useCallback(() => {
    setSortStateInternal(initialSortState);
    setFilterStateInternal(initialFilterState);
    setPaginationStateInternal({
      pageIndex: 0,
      pageSize: defaultPageSize,
      totalRows: data.length,
      totalPages: Math.ceil(data.length / defaultPageSize),
      pageSizeOptions,
    });
    setSelectionStateInternal(initialSelectionState ?? createEmptySelectionState());
    resetColumnVisibility();
  }, [
    initialSortState,
    initialFilterState,
    initialSelectionState,
    defaultPageSize,
    pageSizeOptions,
    data.length,
    resetColumnVisibility,
  ]);

  // ---- Return ----
  return {
    // Processed data
    processedData,
    filteredData: sortedData,
    totalFilteredRows: sortedData.length,
    totalRows: data.length,

    // Sort
    sortState,
    setSortState,
    toggleSort,
    clearSort,

    // Filter
    filterState,
    setFilterState,
    setColumnFilter,
    removeColumnFilter,
    setGlobalFilter,
    clearFilters,

    // Pagination
    paginationState,
    setPaginationState,
    setPageIndex,
    setPageSize,
    goToFirstPage,
    goToLastPage,
    goToNextPage,
    goToPreviousPage,
    paginationMeta,

    // Selection
    selectionState,
    setSelectionState,
    selectRow,
    deselectRow,
    toggleRowSelection,
    selectAll,
    deselectAll,
    toggleSelectAll,
    isRowSelected,
    getSelectedRows,

    // Column visibility
    visibleColumns,
    hiddenColumnIds,
    toggleColumnVisibility,
    setColumnVisibility,
    resetColumnVisibility,

    // Utilities
    reset,
    isLoading,
    isEmpty: processedData.length === 0,
  };
}

// ============================================================================
// Server-Side Hook: useDataTableServer
// ============================================================================

/**
 * Hook for managing DataTable state with server-side data fetching via React Query
 *
 * @template TData - The type of data in each row
 *
 * @example
 * ```tsx
 * const {
 *   processedData,
 *   isLoading,
 *   sortState,
 *   toggleSort,
 *   paginationState,
 *   setPageIndex,
 *   refetch,
 * } = useDataTableServer({
 *   columns,
 *   getRowId: (row) => row.id,
 *   queryKey: ['users'],
 *   queryFn: async (params) => {
 *     const response = await api.getUsers(params);
 *     return { data: response.users, total: response.total, pageIndex: params.pagination.pageIndex, pageSize: params.pagination.pageSize };
 *   },
 * });
 * ```
 */
export function useDataTableServer<TData>(
  options: UseDataTableServerOptions<TData>
): UseDataTableReturn<TData> & {
  /** Refetch data */
  refetch: () => void;
  /** Whether data is being fetched */
  isFetching: boolean;
  /** Query error */
  error: Error | null;
} {
  const {
    columns,
    getRowId,
    queryKey,
    queryFn,
    initialSortState = DEFAULT_SORT_STATE,
    initialFilterState = DEFAULT_FILTER_STATE as FilterState<TData>,
    initialPaginationState,
    initialSelectionState,
    defaultPageSize = DEFAULT_PAGE_SIZE,
    pageSizeOptions = DEFAULT_PAGE_SIZE_OPTIONS,
    filterDebounce = DEFAULT_FILTER_DEBOUNCE,
    queryOptions,
    onSortChange,
    onFilterChange,
    onPaginationChange,
    onSelectionChange,
  } = options;

  const queryClient = useQueryClient();

  // ---- State ----
  const [sortState, setSortStateInternal] = useState<SortingState>(initialSortState);
  const [filterState, setFilterStateInternal] = useState<FilterState<TData>>(initialFilterState);
  const [paginationState, setPaginationStateInternal] = useState<PaginationState>({
    pageIndex: initialPaginationState?.pageIndex ?? 0,
    pageSize: initialPaginationState?.pageSize ?? defaultPageSize,
    totalRows: 0,
    totalPages: 0,
    pageSizeOptions,
  });
  const [selectionState, setSelectionStateInternal] = useState<SelectionState<string>>(
    initialSelectionState ?? createEmptySelectionState()
  );
  const [hiddenColumnIds, setHiddenColumnIds] = useState<Set<string>>(
    new Set(columns.filter((col) => col.visible === false).map((col) => col.id))
  );

  // Debounce filter for server requests
  const debouncedFilterState = useDebouncedValue(filterState, filterDebounce);

  // Build query params
  const queryParams: ServerSideParams<TData> = useMemo(
    () => ({
      sort: sortState.columnId ? sortState : null,
      filter: debouncedFilterState,
      pagination: paginationState,
    }),
    [sortState, debouncedFilterState, paginationState]
  );

  // React Query
  const {
    data: queryData,
    isLoading,
    isFetching,
    error,
    refetch,
  } = useQuery({
    queryKey: [...queryKey, queryParams],
    queryFn: () => queryFn(queryParams),
    ...queryOptions,
  });

  // Update pagination totals from server response
  useEffect(() => {
    if (queryData) {
      setPaginationStateInternal((prev) => ({
        ...prev,
        totalRows: queryData.total,
        totalPages: Math.ceil(queryData.total / prev.pageSize),
      }));
    }
  }, [queryData]);

  // ---- Sort Actions ----
  const setSortState = useCallback(
    (newSort: SortingState) => {
      setSortStateInternal(newSort);
      // Reset to first page on sort change
      setPaginationStateInternal((prev) => ({ ...prev, pageIndex: 0 }));
      onSortChange?.(newSort);
    },
    [onSortChange]
  );

  const toggleSort = useCallback(
    (columnId: string) => {
      setSortStateInternal((prev) => {
        let newSort: SortingState;
        if (prev.columnId !== columnId) {
          newSort = { columnId, direction: "asc" };
        } else if (prev.direction === "asc") {
          newSort = { columnId, direction: "desc" };
        } else {
          newSort = { columnId: null, direction: null };
        }
        // Reset to first page on sort change
        setPaginationStateInternal((p) => ({ ...p, pageIndex: 0 }));
        onSortChange?.(newSort);
        return newSort;
      });
    },
    [onSortChange]
  );

  const clearSort = useCallback(() => {
    setSortState({ columnId: null, direction: null });
  }, [setSortState]);

  // ---- Filter Actions ----
  const setFilterState = useCallback(
    (newFilter: FilterState<TData>) => {
      setFilterStateInternal(newFilter);
      // Reset to first page on filter change
      setPaginationStateInternal((prev) => ({ ...prev, pageIndex: 0 }));
      onFilterChange?.(newFilter);
    },
    [onFilterChange]
  );

  const setColumnFilter = useCallback(
    (columnId: string, value: unknown, operator?: FilterOperator) => {
      setFilterStateInternal((prev) => {
        const existingIndex = prev.columnFilters.findIndex((f) => f.id === columnId);
        const newFilter: ColumnFilter<TData> = { id: columnId, value, operator };

        let result: FilterState<TData>;
        if (value === "" || value == null) {
          result = {
            ...prev,
            columnFilters: prev.columnFilters.filter((f) => f.id !== columnId),
          };
        } else if (existingIndex >= 0) {
          const newFilters = [...prev.columnFilters];
          newFilters[existingIndex] = newFilter;
          result = { ...prev, columnFilters: newFilters };
        } else {
          result = { ...prev, columnFilters: [...prev.columnFilters, newFilter] };
        }

        // Reset to first page on filter change
        setPaginationStateInternal((p) => ({ ...p, pageIndex: 0 }));
        onFilterChange?.(result);
        return result;
      });
    },
    [onFilterChange]
  );

  const removeColumnFilter = useCallback(
    (columnId: string) => {
      setFilterStateInternal((prev) => {
        const result = {
          ...prev,
          columnFilters: prev.columnFilters.filter((f) => f.id !== columnId),
        };
        // Reset to first page on filter change
        setPaginationStateInternal((p) => ({ ...p, pageIndex: 0 }));
        onFilterChange?.(result);
        return result;
      });
    },
    [onFilterChange]
  );

  const setGlobalFilter = useCallback(
    (value: string) => {
      setFilterStateInternal((prev) => {
        const result = {
          ...prev,
          globalFilter: value ? { value } : undefined,
        };
        // Reset to first page on filter change
        setPaginationStateInternal((p) => ({ ...p, pageIndex: 0 }));
        onFilterChange?.(result);
        return result;
      });
    },
    [onFilterChange]
  );

  const clearFilters = useCallback(() => {
    setFilterState({ columnFilters: [], globalFilter: undefined });
  }, [setFilterState]);

  // ---- Pagination Actions ----
  const setPaginationState = useCallback(
    (newPagination: PaginationState) => {
      setPaginationStateInternal(newPagination);
      onPaginationChange?.(newPagination);
    },
    [onPaginationChange]
  );

  const setPageIndex = useCallback(
    (index: number) => {
      setPaginationStateInternal((prev) => {
        const maxIndex = Math.max(0, (prev.totalPages ?? 1) - 1);
        const safeIndex = Math.max(0, Math.min(index, maxIndex));
        const result = { ...prev, pageIndex: safeIndex };
        onPaginationChange?.(result);
        return result;
      });
    },
    [onPaginationChange]
  );

  const setPageSize = useCallback(
    (size: number) => {
      setPaginationStateInternal((prev) => {
        const result = {
          ...prev,
          pageSize: size,
          pageIndex: 0,
          totalPages: Math.ceil((prev.totalRows ?? 0) / size),
        };
        onPaginationChange?.(result);
        return result;
      });
    },
    [onPaginationChange]
  );

  const goToFirstPage = useCallback(() => setPageIndex(0), [setPageIndex]);
  const goToLastPage = useCallback(
    () => setPageIndex((paginationState.totalPages ?? 1) - 1),
    [setPageIndex, paginationState.totalPages]
  );
  const goToNextPage = useCallback(
    () => setPageIndex(paginationState.pageIndex + 1),
    [setPageIndex, paginationState.pageIndex]
  );
  const goToPreviousPage = useCallback(
    () => setPageIndex(paginationState.pageIndex - 1),
    [setPageIndex, paginationState.pageIndex]
  );

  // ---- Selection Actions ----
  const processedData = queryData?.data ?? [];
  const allRowIds = useMemo(
    () => new Set(processedData.map((row) => getRowId(row))),
    [processedData, getRowId]
  );

  const setSelectionState = useCallback(
    (newSelection: SelectionState<string>) => {
      setSelectionStateInternal(newSelection);
      onSelectionChange?.(newSelection);
    },
    [onSelectionChange]
  );

  const selectRow = useCallback(
    (id: string) => {
      const newSelectedIds = new Set(selectionState.selectedIds);
      newSelectedIds.add(id);
      setSelectionState(computeSelectionState(newSelectedIds, allRowIds));
    },
    [selectionState.selectedIds, allRowIds, setSelectionState]
  );

  const deselectRow = useCallback(
    (id: string) => {
      const newSelectedIds = new Set(selectionState.selectedIds);
      newSelectedIds.delete(id);
      setSelectionState(computeSelectionState(newSelectedIds, allRowIds));
    },
    [selectionState.selectedIds, allRowIds, setSelectionState]
  );

  const toggleRowSelection = useCallback(
    (id: string) => {
      if (selectionState.selectedIds.has(id)) {
        deselectRow(id);
      } else {
        selectRow(id);
      }
    },
    [selectionState.selectedIds, selectRow, deselectRow]
  );

  const selectAll = useCallback(() => {
    setSelectionState(computeSelectionState(new Set(allRowIds), allRowIds));
  }, [allRowIds, setSelectionState]);

  const deselectAll = useCallback(() => {
    setSelectionState(createEmptySelectionState());
  }, [setSelectionState]);

  const toggleSelectAll = useCallback(() => {
    if (selectionState.isAllSelected) {
      deselectAll();
    } else {
      selectAll();
    }
  }, [selectionState.isAllSelected, selectAll, deselectAll]);

  const isRowSelected = useCallback(
    (id: string) => selectionState.selectedIds.has(id),
    [selectionState.selectedIds]
  );

  const getSelectedRows = useCallback(
    () => processedData.filter((row) => selectionState.selectedIds.has(getRowId(row))),
    [processedData, selectionState.selectedIds, getRowId]
  );

  // ---- Column Visibility ----
  const visibleColumns = useMemo(
    () => columns.filter((col) => !hiddenColumnIds.has(col.id)),
    [columns, hiddenColumnIds]
  );

  const toggleColumnVisibility = useCallback((columnId: string) => {
    setHiddenColumnIds((prev) => {
      const next = new Set(prev);
      if (next.has(columnId)) {
        next.delete(columnId);
      } else {
        next.add(columnId);
      }
      return next;
    });
  }, []);

  const setColumnVisibility = useCallback((columnId: string, visible: boolean) => {
    setHiddenColumnIds((prev) => {
      const next = new Set(prev);
      if (visible) {
        next.delete(columnId);
      } else {
        next.add(columnId);
      }
      return next;
    });
  }, []);

  const resetColumnVisibility = useCallback(() => {
    setHiddenColumnIds(
      new Set(columns.filter((col) => col.visible === false).map((col) => col.id))
    );
  }, [columns]);

  // ---- Pagination Meta ----
  const paginationMeta: PaginationMeta = useMemo(() => {
    const totalRows = paginationState.totalRows ?? 0;
    const pageCount = Math.ceil(totalRows / paginationState.pageSize);
    const firstRowIndex = paginationState.pageIndex * paginationState.pageSize + 1;
    const lastRowIndex = Math.min(
      (paginationState.pageIndex + 1) * paginationState.pageSize,
      totalRows
    );

    return {
      pageCount,
      canPreviousPage: paginationState.pageIndex > 0,
      canNextPage: paginationState.pageIndex < pageCount - 1,
      firstRowIndex: totalRows > 0 ? firstRowIndex : 0,
      lastRowIndex,
    };
  }, [paginationState]);

  // ---- Reset ----
  const reset = useCallback(() => {
    setSortStateInternal(initialSortState);
    setFilterStateInternal(initialFilterState);
    setPaginationStateInternal({
      pageIndex: 0,
      pageSize: defaultPageSize,
      totalRows: 0,
      totalPages: 0,
      pageSizeOptions,
    });
    setSelectionStateInternal(initialSelectionState ?? createEmptySelectionState());
    resetColumnVisibility();
    queryClient.invalidateQueries({ queryKey });
  }, [
    initialSortState,
    initialFilterState,
    initialSelectionState,
    defaultPageSize,
    pageSizeOptions,
    resetColumnVisibility,
    queryClient,
    queryKey,
  ]);

  // ---- Return ----
  return {
    // Processed data
    processedData,
    filteredData: processedData, // For server-side, filtered data is same as processed
    totalFilteredRows: paginationState.totalRows ?? 0,
    totalRows: paginationState.totalRows ?? 0,

    // Sort
    sortState,
    setSortState,
    toggleSort,
    clearSort,

    // Filter
    filterState,
    setFilterState,
    setColumnFilter,
    removeColumnFilter,
    setGlobalFilter,
    clearFilters,

    // Pagination
    paginationState,
    setPaginationState,
    setPageIndex,
    setPageSize,
    goToFirstPage,
    goToLastPage,
    goToNextPage,
    goToPreviousPage,
    paginationMeta,

    // Selection
    selectionState,
    setSelectionState,
    selectRow,
    deselectRow,
    toggleRowSelection,
    selectAll,
    deselectAll,
    toggleSelectAll,
    isRowSelected,
    getSelectedRows,

    // Column visibility
    visibleColumns,
    hiddenColumnIds,
    toggleColumnVisibility,
    setColumnVisibility,
    resetColumnVisibility,

    // Utilities
    reset,
    isLoading,
    isEmpty: processedData.length === 0,

    // Server-specific
    refetch,
    isFetching,
    error: error as Error | null,
  };
}

// ============================================================================
// Export Default
// ============================================================================

export default useDataTable;
