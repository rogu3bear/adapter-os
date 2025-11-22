"use client";

import * as React from "react";
import { ArrowUpIcon, ArrowDownIcon, ArrowUpDownIcon } from "lucide-react";

import { TableHeader, TableRow, TableHead } from "../../ui/table";
import { Button } from "../../ui/button";
import { Checkbox } from "../../ui/checkbox";
import { cn } from "../../ui/utils";
import type { Column, ColumnDef, SortDirection, SortingState } from "./types";

export interface DataTableHeaderProps<TData> {
  /** Column definitions */
  columns: Column<TData>[] | ColumnDef<TData>[];
  /** Current sort state */
  sortState?: SortingState;
  /** Callback when sort changes */
  onSortChange?: (columnId: string, direction: SortDirection) => void;
  /** Whether to show selection checkbox column */
  selectable?: boolean;
  /** Whether all rows are selected */
  allSelected?: boolean;
  /** Whether some rows are selected (indeterminate state) */
  someSelected?: boolean;
  /** Callback when select all is toggled */
  onSelectAll?: (selected: boolean) => void;
  /** Additional class names */
  className?: string;
}

/**
 * Sort indicator component for column headers
 */
function SortIndicator({ direction }: { direction: SortDirection }) {
  if (direction === "asc") {
    return <ArrowUpIcon className="ml-1 size-4" aria-label="Sorted ascending" />;
  }
  if (direction === "desc") {
    return <ArrowDownIcon className="ml-1 size-4" aria-label="Sorted descending" />;
  }
  return (
    <ArrowUpDownIcon
      className="ml-1 size-4 opacity-50"
      aria-label="Not sorted"
    />
  );
}

/**
 * Check if sorting is enabled for a column
 * Handles both new Column.sortable and deprecated ColumnDef.enableSorting
 */
function isSortingEnabled<TData>(column: Column<TData> | ColumnDef<TData>): boolean {
  // Check new property first
  if ("sortable" in column && column.sortable !== undefined) {
    return column.sortable;
  }
  // Fall back to deprecated property
  if ("enableSorting" in column && column.enableSorting !== undefined) {
    return column.enableSorting;
  }
  // Default: sorting enabled
  return true;
}

/**
 * Check if column is visible
 * Handles both new Column.visible and deprecated ColumnDef.hidden
 */
function isColumnVisible<TData>(column: Column<TData> | ColumnDef<TData>): boolean {
  // Check new property first
  if ("visible" in column && column.visible !== undefined) {
    return column.visible;
  }
  // Fall back to deprecated property (inverted)
  if ("hidden" in column && column.hidden !== undefined) {
    return !column.hidden;
  }
  // Default: visible
  return true;
}

/**
 * DataTableHeader - Sortable column headers with sort indicators
 *
 * Renders table headers with optional sorting and selection capabilities.
 * Uses existing shadcn/Radix patterns for consistent styling.
 *
 * @example
 * ```tsx
 * const columns: Column<User>[] = [
 *   { id: "name", header: "Name", accessorKey: "name", sortable: true },
 *   { id: "email", header: "Email", accessorKey: "email", sortable: true },
 * ];
 *
 * <DataTableHeader
 *   columns={columns}
 *   sortState={sorting}
 *   onSortChange={handleSortChange}
 *   selectable
 *   allSelected={allSelected}
 *   onSelectAll={handleSelectAll}
 * />
 * ```
 */
export function DataTableHeader<TData>({
  columns,
  sortState,
  onSortChange,
  selectable = false,
  allSelected = false,
  someSelected = false,
  onSelectAll,
  className,
}: DataTableHeaderProps<TData>) {
  const handleSort = React.useCallback(
    (columnId: string) => {
      if (!onSortChange) return;

      let newDirection: SortDirection;
      if (sortState?.columnId !== columnId) {
        // New column, start with ascending
        newDirection = "asc";
      } else if (sortState.direction === "asc") {
        // Toggle to descending
        newDirection = "desc";
      } else if (sortState.direction === "desc") {
        // Toggle to no sort
        newDirection = null;
      } else {
        // No sort, start with ascending
        newDirection = "asc";
      }

      onSortChange(columnId, newDirection);
    },
    [sortState, onSortChange]
  );

  const handleSelectAllChange = React.useCallback(
    (checked: boolean | "indeterminate") => {
      onSelectAll?.(checked === true);
    },
    [onSelectAll]
  );

  // Filter out hidden columns
  const visibleColumns = columns.filter(isColumnVisible);

  return (
    <TableHeader className={className} data-slot="data-table-header">
      <TableRow>
        {selectable && (
          <TableHead className="w-12">
            <Checkbox
              checked={allSelected ? true : someSelected ? "indeterminate" : false}
              onCheckedChange={handleSelectAllChange}
              aria-label="Select all rows"
            />
          </TableHead>
        )}
        {visibleColumns.map((column) => {
          const isSorted = sortState?.columnId === column.id;
          const sortDirection = isSorted ? sortState.direction : null;
          const headerText =
            typeof column.header === "function"
              ? column.header()
              : column.header;
          const canSort = isSortingEnabled(column);

          return (
            <TableHead
              key={column.id}
              className={cn(
                column.sticky === "left" && "sticky left-0 z-10 bg-background",
                column.sticky === "right" && "sticky right-0 z-10 bg-background",
                column.align === "center" && "text-center",
                column.align === "right" && "text-right",
                column.headerClassName
              )}
              style={{
                width: column.width,
                minWidth: column.minWidth,
                maxWidth: column.maxWidth,
              }}
            >
              {canSort && onSortChange ? (
                <Button
                  variant="ghost"
                  size="sm"
                  className={cn(
                    "-ml-3 h-8 data-[state=sorted]:bg-accent",
                    column.align === "center" && "justify-center",
                    column.align === "right" && "justify-end ml-auto -mr-3"
                  )}
                  onClick={() => handleSort(column.id)}
                  data-state={isSorted ? "sorted" : undefined}
                  aria-sort={
                    sortDirection === "asc"
                      ? "ascending"
                      : sortDirection === "desc"
                        ? "descending"
                        : "none"
                  }
                >
                  <span>{headerText}</span>
                  <SortIndicator direction={sortDirection} />
                </Button>
              ) : (
                <span>{headerText}</span>
              )}
            </TableHead>
          );
        })}
      </TableRow>
    </TableHeader>
  );
}

export default DataTableHeader;
