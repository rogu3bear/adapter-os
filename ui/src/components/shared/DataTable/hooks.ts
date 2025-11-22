"use client";

import { useCallback, useMemo, useState } from "react";
import type {
  Column,
  ColumnFilter,
  PaginationState,
  RowSelection,
  SortDirection,
  SortingState,
} from "./types";

/**
 * Hook for managing row selection state
 */
export function useRowSelection<TData>(
  data: TData[],
  getRowId: (row: TData) => string,
  initialSelectedIds?: Set<string>
) {
  const [selectedIds, setSelectedIds] = useState<Set<string>>(
    initialSelectedIds ?? new Set()
  );

  const allRowIds = useMemo(
    () => new Set(data.map((row) => getRowId(row))),
    [data, getRowId]
  );

  const rowSelection: RowSelection = useMemo(() => {
    const isAllSelected =
      allRowIds.size > 0 &&
      Array.from(allRowIds).every((id) => selectedIds.has(id));
    const isPartiallySelected =
      !isAllSelected &&
      Array.from(allRowIds).some((id) => selectedIds.has(id));

    return {
      selectedIds,
      isAllSelected,
      isPartiallySelected,
    };
  }, [selectedIds, allRowIds]);

  const toggleRow = useCallback((rowId: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(rowId)) {
        next.delete(rowId);
      } else {
        next.add(rowId);
      }
      return next;
    });
  }, []);

  const selectSingle = useCallback((rowId: string) => {
    setSelectedIds(new Set([rowId]));
  }, []);

  const selectAll = useCallback(() => {
    setSelectedIds(new Set(allRowIds));
  }, [allRowIds]);

  const clearSelection = useCallback(() => {
    setSelectedIds(new Set());
  }, []);

  const toggleAll = useCallback(() => {
    if (rowSelection.isAllSelected) {
      clearSelection();
    } else {
      selectAll();
    }
  }, [rowSelection.isAllSelected, clearSelection, selectAll]);

  const isRowSelected = useCallback(
    (rowId: string) => selectedIds.has(rowId),
    [selectedIds]
  );

  return {
    selectedIds,
    setSelectedIds,
    rowSelection,
    toggleRow,
    selectSingle,
    selectAll,
    clearSelection,
    toggleAll,
    isRowSelected,
  };
}

/**
 * Hook for managing sorting state
 */
export function useSorting(initialSorting?: SortingState) {
  const [sorting, setSorting] = useState<SortingState>(
    initialSorting ?? { columnId: null, direction: null }
  );

  const toggleSort = useCallback((columnId: string) => {
    setSorting((prev) => {
      if (prev.columnId !== columnId) {
        return { columnId, direction: "asc" };
      }
      if (prev.direction === "asc") {
        return { columnId, direction: "desc" };
      }
      if (prev.direction === "desc") {
        return { columnId: null, direction: null };
      }
      return { columnId, direction: "asc" };
    });
  }, []);

  const setSort = useCallback(
    (columnId: string, direction: SortDirection) => {
      setSorting({ columnId, direction });
    },
    []
  );

  const resetSorting = useCallback(() => {
    setSorting({ columnId: null, direction: null });
  }, []);

  return {
    sorting,
    setSorting,
    toggleSort,
    setSort,
    resetSorting,
  };
}

/**
 * Hook for managing pagination state
 */
export function usePagination(
  totalRows: number,
  initialPageSize: number = 10,
  initialPageIndex: number = 0
) {
  const [pageIndex, setPageIndex] = useState(initialPageIndex);
  const [pageSize, setPageSize] = useState(initialPageSize);

  const totalPages = Math.ceil(totalRows / pageSize);

  const pagination: PaginationState = useMemo(
    () => ({
      pageIndex,
      pageSize,
      totalRows,
      totalPages,
    }),
    [pageIndex, pageSize, totalRows, totalPages]
  );

  const canPreviousPage = pageIndex > 0;
  const canNextPage = pageIndex < totalPages - 1;

  const goToPage = useCallback(
    (index: number) => {
      const safeIndex = Math.max(0, Math.min(index, totalPages - 1));
      setPageIndex(safeIndex);
    },
    [totalPages]
  );

  const nextPage = useCallback(() => {
    if (canNextPage) {
      setPageIndex((prev) => prev + 1);
    }
  }, [canNextPage]);

  const previousPage = useCallback(() => {
    if (canPreviousPage) {
      setPageIndex((prev) => prev - 1);
    }
  }, [canPreviousPage]);

  const firstPage = useCallback(() => {
    setPageIndex(0);
  }, []);

  const lastPage = useCallback(() => {
    setPageIndex(totalPages - 1);
  }, [totalPages]);

  const changePageSize = useCallback(
    (newSize: number) => {
      setPageSize(newSize);
      // Reset to first page when changing page size
      setPageIndex(0);
    },
    []
  );

  return {
    pagination,
    canPreviousPage,
    canNextPage,
    goToPage,
    nextPage,
    previousPage,
    firstPage,
    lastPage,
    changePageSize,
    setPageIndex,
    setPageSize,
  };
}

/**
 * Hook for processing table data with sorting, filtering, and pagination
 */
export function useProcessedData<TData>(
  data: TData[],
  columns: Column<TData>[],
  sorting: SortingState,
  filters: ColumnFilter<TData>[],
  globalFilter: string,
  pagination?: PaginationState
) {
  // Apply global filter
  const globallyFiltered = useMemo(() => {
    if (!globalFilter) return data;

    const searchTerm = globalFilter.toLowerCase();
    return data.filter((row, rowIndex) => {
      return columns.some((column) => {
        const value = column.accessorFn
          ? column.accessorFn(row, rowIndex)
          : column.accessorKey
            ? row[column.accessorKey]
            : null;

        if (value == null) return false;
        return String(value).toLowerCase().includes(searchTerm);
      });
    });
  }, [data, columns, globalFilter]);

  // Apply column filters
  const columnFiltered = useMemo(() => {
    if (filters.length === 0) return globallyFiltered;

    return globallyFiltered.filter((row, rowIndex) => {
      return filters.every((filter) => {
        const column = columns.find((col) => col.id === filter.id);
        if (!column) return true;

        if (filter.filterFn) {
          return filter.filterFn(row, filter.value);
        }

        const value = column.accessorFn
          ? column.accessorFn(row, rowIndex)
          : column.accessorKey
            ? row[column.accessorKey]
            : null;

        if (filter.value == null || filter.value === "") return true;
        return String(value).toLowerCase().includes(String(filter.value).toLowerCase());
      });
    });
  }, [globallyFiltered, filters, columns]);

  // Apply sorting
  const sorted = useMemo(() => {
    if (!sorting.columnId || !sorting.direction) return columnFiltered;

    const column = columns.find((col) => col.id === sorting.columnId);
    if (!column) return columnFiltered;

    return [...columnFiltered].sort((a, b) => {
      if (column.sortingFn) {
        return column.sortingFn(a, b, sorting.direction);
      }

      // For sorting, we use rowIndex 0 as placeholder since we're comparing values, not accessing by index
      const aValue = column.accessorFn
        ? column.accessorFn(a, 0)
        : column.accessorKey
          ? a[column.accessorKey]
          : null;
      const bValue = column.accessorFn
        ? column.accessorFn(b, 0)
        : column.accessorKey
          ? b[column.accessorKey]
          : null;

      // Handle null/undefined
      if (aValue == null && bValue == null) return 0;
      if (aValue == null) return sorting.direction === "asc" ? 1 : -1;
      if (bValue == null) return sorting.direction === "asc" ? -1 : 1;

      // Compare values
      let comparison = 0;
      if (typeof aValue === "string" && typeof bValue === "string") {
        comparison = aValue.localeCompare(bValue);
      } else if (typeof aValue === "number" && typeof bValue === "number") {
        comparison = aValue - bValue;
      } else if (aValue instanceof Date && bValue instanceof Date) {
        comparison = aValue.getTime() - bValue.getTime();
      } else {
        comparison = String(aValue).localeCompare(String(bValue));
      }

      return sorting.direction === "asc" ? comparison : -comparison;
    });
  }, [columnFiltered, sorting, columns]);

  // Apply pagination
  const paginated = useMemo(() => {
    if (!pagination) return sorted;

    const start = pagination.pageIndex * pagination.pageSize;
    const end = start + pagination.pageSize;
    return sorted.slice(start, end);
  }, [sorted, pagination]);

  return {
    processedData: paginated,
    totalFiltered: sorted.length,
    totalRows: data.length,
  };
}
