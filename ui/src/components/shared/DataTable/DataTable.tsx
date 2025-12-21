"use client";

import * as React from "react";
import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
} from "react";
import { ArrowUpDown, ArrowUp, ArrowDown, Inbox } from "lucide-react";

import { cn } from "@/lib/utils";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Checkbox } from "@/components/ui/checkbox";
import { Skeleton } from "@/components/ui/skeleton";

import type {
  CellContext,
  Column,
  ColumnFilter,
  DataTableProps,
  DataTableRef,
  SortingState,
} from "./types";
import {
  usePagination,
  useProcessedData,
  useRowSelection,
  useSorting,
} from "./hooks";

/**
 * A flexible, reusable DataTable component with generic typing for row data.
 *
 * Features:
 * - Generic typing for row data
 * - Column definitions with sorting and filtering
 * - Selection support (single/multi)
 * - Responsive design with Tailwind
 * - Integration points for pagination
 * - Empty state handling
 * - Loading state
 *
 * @example
 * ```tsx
 * interface User {
 *   id: string;
 *   name: string;
 *   email: string;
 * }
 *
 * const columns: ColumnDef<User>[] = [
 *   { id: 'name', header: 'Name', accessorKey: 'name', enableSorting: true },
 *   { id: 'email', header: 'Email', accessorKey: 'email' },
 * ];
 *
 * <DataTable
 *   data={users}
 *   columns={columns}
 *   getRowId={(row) => row.id}
 *   selectionMode="multi"
 *   enableSorting
 * />
 * ```
 */
function DataTableInner<TData>(
  {
    data,
    columns,
    getRowId,

    // Selection
    selectionMode = "none",
    selectedIds: controlledSelectedIds,
    onSelectionChange,

    // Sorting
    enableSorting = true,
    sorting: controlledSorting,
    onSortingChange,

    // Filtering
    filters = [],
    globalFilter = "",
    onFilterChange: _onFilterChange,

    // Pagination
    enablePagination = false,
    pagination: controlledPagination,
    onPaginationChange,
    pageSizes: _pageSizes = [10, 20, 50, 100],

    // States
    isLoading = false,
    loadingRowCount = 5,

    // Empty state
    emptyState,
    emptyTitle = "No data",
    emptyDescription = "There are no items to display.",
    emptyStateMessage,

    // Row interactions
    onRowClick,
    onRowDoubleClick,
    rowClassName,

    // Styling
    className,
    dense = false,
    striped = false,
    bordered = false,
    stickyHeader = false,
    maxHeight,

    // Accessibility
    caption,
    ariaLabel,
  }: DataTableProps<TData>,
  ref: React.ForwardedRef<DataTableRef<TData>>
) {
  const tableContainerRef = useRef<HTMLDivElement>(null);

  // Selection state management
  const {
    selectedIds: internalSelectedIds,
    setSelectedIds: setInternalSelectedIds,
    rowSelection,
    toggleRow,
    selectSingle,
    selectAll,
    clearSelection,
    toggleAll,
    isRowSelected,
  } = useRowSelection(data, getRowId, controlledSelectedIds);

  // Use controlled or internal selection
  const selectedIds = controlledSelectedIds ?? internalSelectedIds;
  const isControlledSelection = controlledSelectedIds !== undefined;

  // Sync selection changes to parent
  useEffect(() => {
    if (!isControlledSelection && onSelectionChange) {
      onSelectionChange(internalSelectedIds);
    }
  }, [internalSelectedIds, onSelectionChange, isControlledSelection]);

  // Sorting state management
  const {
    sorting: internalSorting,
    setSorting: setInternalSorting,
    toggleSort,
    resetSorting,
  } = useSorting(controlledSorting);

  const sorting = controlledSorting ?? internalSorting;
  const isControlledSorting = controlledSorting !== undefined;

  useEffect(() => {
    if (!isControlledSorting && onSortingChange) {
      onSortingChange(internalSorting);
    }
  }, [internalSorting, onSortingChange, isControlledSorting]);

  // Pagination state management
  const {
    pagination: internalPagination,
    goToPage: _goToPage,
    nextPage: _nextPage,
    previousPage: _previousPage,
    firstPage: _firstPage,
    lastPage: _lastPage,
    changePageSize: _changePageSize,
  } = usePagination(data.length);

  const pagination = enablePagination
    ? (controlledPagination ?? internalPagination)
    : undefined;
  const isControlledPagination = controlledPagination !== undefined;

  useEffect(() => {
    if (enablePagination && !isControlledPagination && onPaginationChange) {
      onPaginationChange(internalPagination);
    }
  }, [enablePagination, internalPagination, onPaginationChange, isControlledPagination]);

  // Process data with sorting, filtering, and pagination
  const { processedData, totalFiltered } = useProcessedData(
    data,
    columns,
    sorting,
    filters,
    globalFilter,
    pagination
  );

  // Get visible columns
  const visibleColumns = useMemo(
    () => columns.filter((col) => col.visible !== false),
    [columns]
  );

  // Expose methods via ref
  useImperativeHandle(
    ref,
    () => ({
      clearSelection: () => {
        if (isControlledSelection && onSelectionChange) {
          onSelectionChange(new Set());
        } else {
          clearSelection();
        }
      },
      selectAll: () => {
        const allIds = new Set(data.map(getRowId));
        if (isControlledSelection && onSelectionChange) {
          onSelectionChange(allIds);
        } else {
          selectAll();
        }
      },
      toggleRow: (rowId: string) => {
        if (isControlledSelection && onSelectionChange) {
          const next = new Set(selectedIds);
          if (next.has(rowId)) {
            next.delete(rowId);
          } else {
            next.add(rowId);
          }
          onSelectionChange(next);
        } else {
          toggleRow(rowId);
        }
      },
      resetSorting: () => {
        if (isControlledSorting && onSortingChange) {
          onSortingChange({ columnId: null, direction: null });
        } else {
          resetSorting();
        }
      },
      resetFilters: () => {
        // Filters are controlled via props, so this is a no-op for controlled state
        // Parent should reset filters via onFilterChange
      },
      resetPagination: () => {
        if (!isControlledPagination && onPaginationChange) {
          onPaginationChange({
            pageIndex: 0,
            pageSize: internalPagination.pageSize,
            totalRows: data.length,
            totalPages: Math.ceil(data.length / internalPagination.pageSize),
          });
        }
      },
      reset: () => {
        // Reset all state
        if (isControlledSelection && onSelectionChange) {
          onSelectionChange(new Set());
        } else {
          clearSelection();
        }
        if (isControlledSorting && onSortingChange) {
          onSortingChange({ columnId: null, direction: null });
        } else {
          resetSorting();
        }
      },
      getVisibleData: () => processedData,
      getSelectedRows: () => {
        return data.filter((row) => selectedIds.has(getRowId(row)));
      },
      scrollToRow: (rowId: string) => {
        const rowElement = tableContainerRef.current?.querySelector(
          `[data-row-id="${rowId}"]`
        );
        if (rowElement) {
          rowElement.scrollIntoView({ behavior: "smooth", block: "center" });
        }
      },
    }),
    [
      data,
      getRowId,
      selectedIds,
      processedData,
      internalPagination,
      isControlledSelection,
      isControlledSorting,
      isControlledPagination,
      onSelectionChange,
      onSortingChange,
      onPaginationChange,
      clearSelection,
      selectAll,
      toggleRow,
      resetSorting,
    ]
  );

  // Handle row selection
  const handleRowSelect = useCallback(
    (row: TData) => {
      const rowId = getRowId(row);
      if (selectionMode === "single") {
        if (isControlledSelection && onSelectionChange) {
          onSelectionChange(new Set([rowId]));
        } else {
          selectSingle(rowId);
        }
      } else if (selectionMode === "multi") {
        if (isControlledSelection && onSelectionChange) {
          const next = new Set(selectedIds);
          if (next.has(rowId)) {
            next.delete(rowId);
          } else {
            next.add(rowId);
          }
          onSelectionChange(next);
        } else {
          toggleRow(rowId);
        }
      }
    },
    [
      getRowId,
      selectionMode,
      selectedIds,
      isControlledSelection,
      onSelectionChange,
      selectSingle,
      toggleRow,
    ]
  );

  // Handle select all
  const handleSelectAll = useCallback(() => {
    if (isControlledSelection && onSelectionChange) {
      if (rowSelection.isAllSelected) {
        onSelectionChange(new Set());
      } else {
        onSelectionChange(new Set(data.map(getRowId)));
      }
    } else {
      toggleAll();
    }
  }, [
    data,
    getRowId,
    rowSelection.isAllSelected,
    isControlledSelection,
    onSelectionChange,
    toggleAll,
  ]);

  // Handle column sort
  const handleSort = useCallback(
    (columnId: string) => {
      if (!enableSorting) return;

      const column = columns.find((col) => col.id === columnId);
      if (!column || column.sortable === false) return;

      if (isControlledSorting && onSortingChange) {
        const newDirection =
          sorting.columnId !== columnId
            ? "asc"
            : sorting.direction === "asc"
              ? "desc"
              : sorting.direction === "desc"
                ? null
                : "asc";
        onSortingChange({
          columnId: newDirection ? columnId : null,
          direction: newDirection,
        });
      } else {
        toggleSort(columnId);
      }
    },
    [
      enableSorting,
      columns,
      sorting,
      isControlledSorting,
      onSortingChange,
      toggleSort,
    ]
  );

  // Get cell value
  const getCellValue = useCallback(
    <TValue,>(row: TData, column: Column<TData, TValue>, rowIndex: number): TValue => {
      if (column.accessorFn) {
        return column.accessorFn(row, rowIndex);
      }
      if (column.accessorKey) {
        return row[column.accessorKey] as unknown as TValue;
      }
      return undefined as unknown as TValue;
    },
    []
  );

  // Render cell content
  const renderCell = useCallback(
    <TValue,>(
      row: TData,
      column: Column<TData, TValue>,
      rowIndex: number
    ): React.ReactNode => {
      const value = getCellValue(row, column, rowIndex);

      // Create hybrid row object that supports both direct property access
      // and TanStack Table v8 API (row.original)
      const hybridRow = Object.assign(Object.create(Object.getPrototypeOf(row)), row, {
        original: row,
        index: rowIndex,
      }) as TData & { original: TData; index: number };

      const cellContext: CellContext<TData, TValue> = {
        row: hybridRow,
        rowIndex,
        value,
        column,
        getValue: () => value,
      };

      if (column.cell) {
        return column.cell(cellContext);
      }

      // Default rendering
      if (value == null) return null;
      if (typeof value === "boolean") return value ? "Yes" : "No";
      if (value instanceof Date) return value.toLocaleDateString();
      return String(value);
    },
    [getCellValue]
  );

  // Render sort icon
  const renderSortIcon = useCallback(
    (columnId: string) => {
      if (sorting.columnId !== columnId) {
        return <ArrowUpDown className="ml-1 h-3.5 w-3.5 text-muted-foreground/50" />;
      }
      if (sorting.direction === "asc") {
        return <ArrowUp className="ml-1 h-3.5 w-3.5" />;
      }
      return <ArrowDown className="ml-1 h-3.5 w-3.5" />;
    },
    [sorting]
  );

  // Get row class name
  const getRowClassName = useCallback(
    (row: TData, index: number): string => {
      const baseClasses = cn(
        onRowClick && "cursor-pointer",
        striped && index % 2 === 1 && "bg-muted/50",
        selectedIds.has(getRowId(row)) && "bg-primary/5"
      );

      if (typeof rowClassName === "function") {
        return cn(baseClasses, rowClassName(row, index));
      }
      return cn(baseClasses, rowClassName);
    },
    [getRowId, onRowClick, selectedIds, striped, rowClassName]
  );

  // Loading skeleton
  if (isLoading) {
    return (
      <div
        ref={tableContainerRef}
        className={cn(
          "relative w-full overflow-auto",
          bordered && "border rounded-lg",
          className
        )}
        style={{ maxHeight }}
      >
        <Table>
          {caption && <caption className="sr-only">{caption}</caption>}
          <TableHeader className={cn(stickyHeader && "sticky top-0 z-10 bg-background")}>
            <TableRow>
              {selectionMode !== "none" && (
                <TableHead className="w-[40px]">
                  <Skeleton className="h-4 w-4" />
                </TableHead>
              )}
              {visibleColumns.map((column) => (
                <TableHead
                  key={column.id}
                  className={column.headerClassName}
                  style={{
                    width: column.width,
                    minWidth: column.minWidth,
                    maxWidth: column.maxWidth,
                  }}
                >
                  <Skeleton className="h-4 w-20" />
                </TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {Array.from({ length: loadingRowCount }).map((_, index) => (
              <TableRow key={index}>
                {selectionMode !== "none" && (
                  <TableCell className={cn(dense ? "py-1" : "py-2")}>
                    <Skeleton className="h-4 w-4" />
                  </TableCell>
                )}
                {visibleColumns.map((column) => (
                  <TableCell
                    key={column.id}
                    className={cn(dense ? "py-1" : "py-2", column.cellClassName)}
                  >
                    <Skeleton className="h-4 w-full" />
                  </TableCell>
                ))}
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    );
  }

  // Empty state
  if (processedData.length === 0) {
    if (emptyState) {
      return <>{emptyState}</>;
    }

    return (
      <div
        className={cn(
          "relative w-full overflow-auto",
          bordered && "border rounded-lg",
          className
        )}
      >
        <Table>
          {caption && <caption className="sr-only">{caption}</caption>}
          <TableHeader className={cn(stickyHeader && "sticky top-0 z-10 bg-background")}>
            <TableRow>
              {selectionMode !== "none" && (
                <TableHead className="w-[40px]" />
              )}
              {visibleColumns.map((column) => (
                <TableHead
                  key={column.id}
                  className={column.headerClassName}
                  style={{
                    width: column.width,
                    minWidth: column.minWidth,
                    maxWidth: column.maxWidth,
                  }}
                >
                  {typeof column.header === "function"
                    ? column.header()
                    : column.header}
                </TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableRow className="hover:bg-transparent">
              <TableCell
                colSpan={visibleColumns.length + (selectionMode !== "none" ? 1 : 0)}
                className="h-48"
              >
                <div className="flex flex-col items-center justify-center text-center">
                  <Inbox className="h-12 w-12 text-muted-foreground/50 mb-4" />
                  <h3 className="text-lg font-semibold text-foreground mb-1">
                    {emptyStateMessage || emptyTitle}
                  </h3>
                  {!emptyStateMessage && (
                    <p className="text-sm text-muted-foreground max-w-sm">
                      {emptyDescription}
                    </p>
                  )}
                </div>
              </TableCell>
            </TableRow>
          </TableBody>
        </Table>
      </div>
    );
  }

  return (
    <div
      ref={tableContainerRef}
      className={cn(
        "relative w-full overflow-auto",
        bordered && "border rounded-lg",
        className
      )}
      style={{ maxHeight }}
      aria-label={ariaLabel}
    >
      <Table>
        {caption && <caption className="sr-only">{caption}</caption>}
        <TableHeader className={cn(stickyHeader && "sticky top-0 z-10 bg-background")}>
          <TableRow>
            {selectionMode !== "none" && (
              <TableHead className="w-[40px]">
                {selectionMode === "multi" && (
                  <Checkbox
                    checked={
                      rowSelection.isAllSelected
                        ? true
                        : rowSelection.isPartiallySelected
                          ? "indeterminate"
                          : false
                    }
                    onCheckedChange={handleSelectAll}
                    aria-label="Select all rows"
                  />
                )}
              </TableHead>
            )}
            {visibleColumns.map((column) => {
              const isSortable =
                enableSorting && column.sortable !== false;

              return (
                <TableHead
                  key={column.id}
                  className={cn(
                    column.headerClassName,
                    isSortable && "cursor-pointer select-none"
                  )}
                  style={{
                    width: column.width,
                    minWidth: column.minWidth,
                    maxWidth: column.maxWidth,
                  }}
                  onClick={isSortable ? () => handleSort(column.id) : undefined}
                  aria-sort={
                    sorting.columnId === column.id
                      ? sorting.direction === "asc"
                        ? "ascending"
                        : "descending"
                      : undefined
                  }
                >
                  <div className="flex items-center">
                    {typeof column.header === "function"
                      ? column.header()
                      : column.header}
                    {isSortable && renderSortIcon(column.id)}
                  </div>
                </TableHead>
              );
            })}
          </TableRow>
        </TableHeader>
        <TableBody>
          {processedData.map((row, rowIndex) => {
            const rowId = getRowId(row);
            const isSelected = selectedIds.has(rowId);

            return (
              <TableRow
                key={rowId}
                data-row-id={rowId}
                data-state={isSelected ? "selected" : undefined}
                className={getRowClassName(row, rowIndex)}
                onClick={(e) => {
                  // Don't trigger row click if clicking checkbox
                  if ((e.target as HTMLElement).closest('[role="checkbox"]')) {
                    return;
                  }
                  onRowClick?.(row);
                }}
                onDoubleClick={() => onRowDoubleClick?.(row)}
              >
                {selectionMode !== "none" && (
                  <TableCell className={cn(dense ? "py-1" : "py-2")}>
                    <Checkbox
                      checked={isSelected}
                      onCheckedChange={() => handleRowSelect(row)}
                      aria-label={`Select row ${rowId}`}
                    />
                  </TableCell>
                )}
                {visibleColumns.map((column) => (
                  <TableCell
                    key={column.id}
                    className={cn(dense ? "py-1" : "py-2", column.cellClassName)}
                    style={{
                      width: column.width,
                      minWidth: column.minWidth,
                      maxWidth: column.maxWidth,
                    }}
                  >
                    {renderCell(row, column, rowIndex)}
                  </TableCell>
                ))}
              </TableRow>
            );
          })}
        </TableBody>
      </Table>

      {/* Selection count indicator */}
      {selectionMode !== "none" && selectedIds.size > 0 && (
        <div className="sticky bottom-0 left-0 right-0 bg-muted/90 backdrop-blur-sm border-t px-4 py-2 text-sm text-muted-foreground">
          {selectedIds.size} of {totalFiltered} row(s) selected
        </div>
      )}
    </div>
  );
}

/**
 * DataTable component with forwarded ref support for generic types.
 */
export const DataTable = forwardRef(DataTableInner) as <TData>(
  props: DataTableProps<TData> & { ref?: React.ForwardedRef<DataTableRef<TData>> }
) => ReturnType<typeof DataTableInner>;

export default DataTable;
