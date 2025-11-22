import * as React from "react";
import { cn } from "../../ui/utils";
import { Skeleton } from "./Skeleton";

export interface TableSkeletonProps extends React.HTMLAttributes<HTMLDivElement> {
  /** Number of rows to render */
  rows?: number;
  /** Number of columns to render */
  columns?: number;
  /** Show header row */
  showHeader?: boolean;
  /** Column widths (array of percentages or CSS values) */
  columnWidths?: (string | number)[];
  /** Enable row hover effect placeholder */
  showRowDividers?: boolean;
  /** Show checkbox column */
  showCheckbox?: boolean;
  /** Show action column (last column) */
  showActions?: boolean;
}

/**
 * Skeleton placeholder for data tables.
 * Renders a configurable grid of skeleton rows and columns.
 */
export function TableSkeleton({
  rows = 5,
  columns = 4,
  showHeader = true,
  columnWidths,
  showRowDividers = true,
  showCheckbox = false,
  showActions = false,
  className,
  ...props
}: TableSkeletonProps) {
  const totalColumns = columns + (showCheckbox ? 1 : 0) + (showActions ? 1 : 0);

  // Generate default column widths if not provided
  const widths = columnWidths || Array.from({ length: columns }).map((_, i) => {
    // First column typically wider for names/titles
    if (i === 0) return "30%";
    return `${Math.floor(70 / (columns - 1))}%`;
  });

  const renderCell = (colIndex: number, isHeader: boolean = false) => {
    const height = isHeader ? "h-4" : "h-3.5";
    const width = widths[colIndex] || "auto";

    return (
      <div
        key={colIndex}
        className="px-2 py-3"
        style={{ width: typeof width === "number" ? `${width}px` : width }}
      >
        <Skeleton
          className={cn(
            height,
            isHeader ? "w-3/4" : colIndex === 0 ? "w-full" : "w-4/5"
          )}
        />
      </div>
    );
  };

  const renderCheckboxCell = (isHeader: boolean = false) => (
    <div className="px-2 py-3 w-10 flex-shrink-0">
      <Skeleton className={cn("h-4 w-4 rounded-sm", isHeader && "opacity-70")} />
    </div>
  );

  const renderActionsCell = () => (
    <div className="px-2 py-3 w-20 flex-shrink-0 flex justify-end gap-1">
      <Skeleton className="h-7 w-7 rounded-md" />
      <Skeleton className="h-7 w-7 rounded-md" />
    </div>
  );

  const renderRow = (rowIndex: number, isHeader: boolean = false) => (
    <div
      key={rowIndex}
      className={cn(
        "flex items-center",
        showRowDividers && !isHeader && "border-b border-border",
        isHeader && "border-b border-border bg-muted/30"
      )}
    >
      {showCheckbox && renderCheckboxCell(isHeader)}
      {Array.from({ length: columns }).map((_, colIndex) =>
        renderCell(colIndex, isHeader)
      )}
      {showActions && !isHeader && renderActionsCell()}
      {showActions && isHeader && <div className="w-20 flex-shrink-0" />}
    </div>
  );

  return (
    <div
      className={cn(
        "w-full rounded-lg border border-border overflow-hidden",
        className
      )}
      role="status"
      aria-label="Loading table data"
      {...props}
    >
      {showHeader && renderRow(-1, true)}
      <div className="divide-y divide-border">
        {Array.from({ length: rows }).map((_, rowIndex) =>
          renderRow(rowIndex)
        )}
      </div>
    </div>
  );
}

/**
 * Compact table skeleton for smaller data displays.
 */
export function CompactTableSkeleton({
  rows = 3,
  columns = 3,
  className,
  ...props
}: Omit<TableSkeletonProps, "showHeader" | "showRowDividers">) {
  return (
    <TableSkeleton
      rows={rows}
      columns={columns}
      showHeader={false}
      showRowDividers={false}
      className={cn("border-0", className)}
      {...props}
    />
  );
}
