"use client";

import * as React from "react";

import { TableRow, TableCell } from "../../ui/table";
import { Checkbox } from "../../ui/checkbox";
import { cn } from "../../ui/utils";
import type { CellContext, Column, ColumnDef } from "./types";

export interface DataTableRowProps<TData> {
  /** The row data */
  row: TData;
  /** Column definitions */
  columns: Column<TData>[] | ColumnDef<TData>[];
  /** Row index for accessibility */
  rowIndex: number;
  /** Function to get unique row ID */
  getRowId: (row: TData) => string;
  /** Whether to show selection checkbox */
  selectable?: boolean;
  /** Whether the row is selected */
  selected?: boolean;
  /** Callback when row selection changes */
  onSelectChange?: (selected: boolean) => void;
  /** Callback when row is clicked */
  onClick?: (row: TData, event: React.MouseEvent<HTMLTableRowElement>) => void;
  /** Callback when row is double-clicked */
  onDoubleClick?: (row: TData, event: React.MouseEvent<HTMLTableRowElement>) => void;
  /** Whether the row is clickable (shows pointer cursor) */
  clickable?: boolean;
  /** Whether the row is currently active/focused */
  active?: boolean;
  /** Additional class names for the row */
  className?: string;
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
 * Get cell value from row data based on column definition
 */
function getCellValue<TData>(
  column: Column<TData> | ColumnDef<TData>,
  row: TData,
  rowIndex: number
): unknown {
  if (column.accessorFn) {
    return column.accessorFn(row, rowIndex);
  }
  if (column.accessorKey) {
    return row[column.accessorKey];
  }
  return null;
}

/**
 * Format cell value for display
 */
function formatCellValue(value: unknown): React.ReactNode {
  if (value === null || value === undefined) {
    return null;
  }
  if (typeof value === "boolean") {
    return value ? "Yes" : "No";
  }
  if (value instanceof Date) {
    return value.toLocaleString();
  }
  if (React.isValidElement(value)) {
    return value;
  }
  return String(value);
}

/**
 * DataTableRow - Individual row component with hover states
 *
 * Renders a single table row with optional selection, click handling,
 * and hover states. Uses existing shadcn/Radix patterns.
 *
 * @example
 * ```tsx
 * <DataTableRow
 *   row={user}
 *   columns={columns}
 *   rowIndex={0}
 *   getRowId={(row) => row.id}
 *   selectable
 *   selected={selectedIds.has(user.id)}
 *   onSelectChange={(selected) => handleSelect(user.id, selected)}
 *   onClick={(row) => handleRowClick(row)}
 *   clickable
 * />
 * ```
 */
export function DataTableRow<TData>({
  row,
  columns,
  rowIndex,
  getRowId,
  selectable = false,
  selected = false,
  onSelectChange,
  onClick,
  onDoubleClick,
  clickable = false,
  active = false,
  className,
}: DataTableRowProps<TData>) {
  const rowId = getRowId(row);

  const handleClick = React.useCallback(
    (event: React.MouseEvent<HTMLTableRowElement>) => {
      onClick?.(row, event);
    },
    [onClick, row]
  );

  const handleDoubleClick = React.useCallback(
    (event: React.MouseEvent<HTMLTableRowElement>) => {
      onDoubleClick?.(row, event);
    },
    [onDoubleClick, row]
  );

  const handleSelectChange = React.useCallback(
    (checked: boolean | "indeterminate") => {
      onSelectChange?.(checked === true);
    },
    [onSelectChange]
  );

  const handleCheckboxClick = React.useCallback(
    (event: React.MouseEvent) => {
      // Prevent row click from firing when clicking checkbox
      event.stopPropagation();
    },
    []
  );

  // Filter out hidden columns
  const visibleColumns = columns.filter(isColumnVisible);

  return (
    <TableRow
      key={rowId}
      data-slot="data-table-row"
      data-state={selected ? "selected" : undefined}
      data-active={active ? "true" : undefined}
      className={cn(
        // Base hover state from table.tsx
        "hover:bg-muted/50 data-[state=selected]:bg-muted",
        // Clickable cursor
        clickable && "cursor-pointer",
        // Active state
        active && "bg-accent/50",
        // Custom classes
        className
      )}
      onClick={clickable || onClick ? handleClick : undefined}
      onDoubleClick={onDoubleClick ? handleDoubleClick : undefined}
      role="row"
      aria-rowindex={rowIndex + 1}
      aria-selected={selected}
      tabIndex={clickable ? 0 : undefined}
    >
      {selectable && (
        <TableCell className="w-12" onClick={handleCheckboxClick}>
          <Checkbox
            checked={selected}
            onCheckedChange={handleSelectChange}
            aria-label={`Select row ${rowIndex + 1}`}
          />
        </TableCell>
      )}
      {visibleColumns.map((column) => {
        const rawValue = getCellValue(column, row, rowIndex);

        // Build cell context for custom cell renderers
        // Create hybrid row object that supports both direct property access
        // and TanStack Table v8 API (row.original)
        const hybridRow = Object.assign(Object.create(Object.getPrototypeOf(row)), row, {
          original: row,
          index: rowIndex,
        }) as TData & { original: TData; index: number };

        const cellContext: CellContext<TData, unknown> = {
          row: hybridRow,
          rowIndex,
          value: rawValue,
          column: column as Column<TData, unknown>,
          getValue: () => rawValue,
        };

        // Use custom cell renderer if provided, otherwise format value
        const cellContent = column.cell
          ? column.cell(cellContext)
          : formatCellValue(rawValue);

        return (
          <TableCell
            key={column.id}
            className={cn(
              column.sticky === "left" && "sticky left-0 z-10 bg-background",
              column.sticky === "right" && "sticky right-0 z-10 bg-background",
              column.align === "center" && "text-center",
              column.align === "right" && "text-right",
              column.cellClassName
            )}
            style={{
              width: column.width,
              minWidth: column.minWidth,
              maxWidth: column.maxWidth,
            }}
          >
            {cellContent}
          </TableCell>
        );
      })}
    </TableRow>
  );
}

export default DataTableRow;
