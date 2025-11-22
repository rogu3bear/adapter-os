"use client";

import * as React from "react";

import { TableBody } from "../../ui/table";
import { cn } from "../../ui/utils";
import { DataTableRow } from "./DataTableRow";
import type { Column, ColumnDef } from "./types";

export interface DataTableBodyProps<TData> {
  /** Array of row data */
  data: TData[];
  /** Column definitions */
  columns: Column<TData>[] | ColumnDef<TData>[];
  /** Function to get unique row ID */
  getRowId: (row: TData) => string;
  /** Whether to show selection checkboxes */
  selectable?: boolean;
  /** Set of selected row keys */
  selectedKeys?: Set<string>;
  /** Callback when row selection changes */
  onSelectChange?: (key: string, selected: boolean) => void;
  /** Callback when a row is clicked */
  onRowClick?: (row: TData, event: React.MouseEvent<HTMLTableRowElement>) => void;
  /** Callback when a row is double-clicked */
  onRowDoubleClick?: (row: TData, event: React.MouseEvent<HTMLTableRowElement>) => void;
  /** Whether rows are clickable */
  clickable?: boolean;
  /** Currently active row key */
  activeKey?: string | null;
  /** Additional class names for the body */
  className?: string;
  /** Custom row class name function */
  getRowClassName?: (row: TData, index: number) => string;
  /** Component to render when data is empty */
  emptyContent?: React.ReactNode;
  /** Loading state */
  loading?: boolean;
  /** Loading placeholder component */
  loadingContent?: React.ReactNode;
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
 * Default empty state component
 */
function DefaultEmptyState({ columnCount }: { columnCount: number }) {
  return (
    <tr>
      <td
        colSpan={columnCount}
        className="h-24 text-center text-muted-foreground"
      >
        No data available
      </td>
    </tr>
  );
}

/**
 * Default loading state component
 */
function DefaultLoadingState({ columnCount }: { columnCount: number }) {
  return (
    <tr>
      <td
        colSpan={columnCount}
        className="h-24 text-center text-muted-foreground"
      >
        <div className="flex items-center justify-center gap-2">
          <div className="size-4 animate-spin rounded-full border-2 border-primary border-t-transparent" />
          <span>Loading...</span>
        </div>
      </td>
    </tr>
  );
}

/**
 * DataTableBody - Row rendering with selection checkboxes
 *
 * Renders the table body with rows, handling selection state,
 * click events, and empty/loading states. Uses existing shadcn/Radix patterns.
 *
 * @example
 * ```tsx
 * const [selectedKeys, setSelectedKeys] = useState<Set<string>>(new Set());
 *
 * <DataTableBody
 *   data={users}
 *   columns={columns}
 *   getRowId={(row) => row.id}
 *   selectable
 *   selectedKeys={selectedKeys}
 *   onSelectChange={(key, selected) => {
 *     const newSet = new Set(selectedKeys);
 *     if (selected) newSet.add(key);
 *     else newSet.delete(key);
 *     setSelectedKeys(newSet);
 *   }}
 *   clickable
 *   onRowClick={(row) => handleRowClick(row)}
 *   emptyContent={<NoUsersFound />}
 * />
 * ```
 */
export function DataTableBody<TData>({
  data,
  columns,
  getRowId,
  selectable = false,
  selectedKeys,
  onSelectChange,
  onRowClick,
  onRowDoubleClick,
  clickable = false,
  activeKey = null,
  className,
  getRowClassName,
  emptyContent,
  loading = false,
  loadingContent,
}: DataTableBodyProps<TData>) {
  // Calculate total column count including selection column
  // Filter out hidden columns for display
  const visibleColumns = columns.filter(isColumnVisible);
  const totalColumns = visibleColumns.length + (selectable ? 1 : 0);

  const handleSelectChange = React.useCallback(
    (row: TData) => (selected: boolean) => {
      const key = getRowId(row);
      onSelectChange?.(key, selected);
    },
    [getRowId, onSelectChange]
  );

  // Loading state
  if (loading) {
    return (
      <TableBody className={className} data-slot="data-table-body">
        {loadingContent || <DefaultLoadingState columnCount={totalColumns} />}
      </TableBody>
    );
  }

  // Empty state
  if (data.length === 0) {
    return (
      <TableBody className={className} data-slot="data-table-body">
        {emptyContent || <DefaultEmptyState columnCount={totalColumns} />}
      </TableBody>
    );
  }

  return (
    <TableBody className={cn(className)} data-slot="data-table-body">
      {data.map((row, index) => {
        const rowKey = getRowId(row);
        const isSelected = selectedKeys?.has(rowKey) ?? false;
        const isActive = activeKey === rowKey;
        const rowClassName = getRowClassName?.(row, index);

        return (
          <DataTableRow
            key={rowKey}
            row={row}
            columns={columns}
            rowIndex={index}
            getRowId={getRowId}
            selectable={selectable}
            selected={isSelected}
            onSelectChange={handleSelectChange(row)}
            onClick={onRowClick}
            onDoubleClick={onRowDoubleClick}
            clickable={clickable}
            active={isActive}
            className={rowClassName}
          />
        );
      })}
    </TableBody>
  );
}

export default DataTableBody;
