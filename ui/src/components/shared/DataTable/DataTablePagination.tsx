"use client";

import * as React from "react";
import {
  ChevronLeftIcon,
  ChevronRightIcon,
  ChevronsLeftIcon,
  ChevronsRightIcon,
} from "lucide-react";

import { cn } from "../../ui/utils";
import { Button } from "../../ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "../../ui/select";

export interface DataTablePaginationProps {
  /** Current page index (0-based) */
  pageIndex: number;
  /** Number of items per page */
  pageSize: number;
  /** Total number of items */
  totalItems: number;
  /** Total number of pages */
  pageCount: number;
  /** Callback when page changes */
  onPageChange: (pageIndex: number) => void;
  /** Callback when page size changes */
  onPageSizeChange: (pageSize: number) => void;
  /** Available page size options */
  pageSizeOptions?: number[];
  /** Number of selected rows (optional) */
  selectedCount?: number;
  /** Whether to show row selection count */
  showSelectedCount?: boolean;
  /** Additional class names */
  className?: string;
}

function DataTablePagination({
  pageIndex,
  pageSize,
  totalItems,
  pageCount,
  onPageChange,
  onPageSizeChange,
  pageSizeOptions = [10, 20, 30, 50, 100],
  selectedCount = 0,
  showSelectedCount = false,
  className,
}: DataTablePaginationProps) {
  const canGoPreviousPage = pageIndex > 0;
  const canGoNextPage = pageIndex < pageCount - 1;

  const startItem = totalItems === 0 ? 0 : pageIndex * pageSize + 1;
  const endItem = Math.min((pageIndex + 1) * pageSize, totalItems);

  return (
    <div
      data-slot="data-table-pagination"
      className={cn(
        "flex flex-col gap-4 px-2 py-4 sm:flex-row sm:items-center sm:justify-between",
        className
      )}
    >
      {/* Left side: Row count info */}
      <div className="flex flex-col gap-1 text-sm text-muted-foreground sm:flex-row sm:items-center sm:gap-4">
        {showSelectedCount && selectedCount > 0 && (
          <span className="font-medium">
            {selectedCount} of {totalItems} row(s) selected
          </span>
        )}
        <span>
          Showing {startItem} to {endItem} of {totalItems} result(s)
        </span>
      </div>

      {/* Right side: Controls */}
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:gap-6">
        {/* Page size selector */}
        <div className="flex items-center gap-2">
          <span className="text-sm text-muted-foreground whitespace-nowrap">
            Rows per page
          </span>
          <Select
            value={String(pageSize)}
            onValueChange={(value) => onPageSizeChange(Number(value))}
          >
            <SelectTrigger className="h-8 w-[70px]" size="sm">
              <SelectValue placeholder={pageSize} />
            </SelectTrigger>
            <SelectContent>
              {pageSizeOptions.map((size) => (
                <SelectItem key={size} value={String(size)}>
                  {size}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        {/* Page indicator */}
        <div className="flex items-center justify-center text-sm text-muted-foreground whitespace-nowrap">
          Page {pageIndex + 1} of {pageCount || 1}
        </div>

        {/* Navigation buttons */}
        <div className="flex items-center gap-1">
          <Button
            variant="outline"
            size="icon-sm"
            onClick={() => onPageChange(0)}
            disabled={!canGoPreviousPage}
            aria-label="Go to first page"
          >
            <ChevronsLeftIcon className="size-4" />
          </Button>
          <Button
            variant="outline"
            size="icon-sm"
            onClick={() => onPageChange(pageIndex - 1)}
            disabled={!canGoPreviousPage}
            aria-label="Go to previous page"
          >
            <ChevronLeftIcon className="size-4" />
          </Button>
          <Button
            variant="outline"
            size="icon-sm"
            onClick={() => onPageChange(pageIndex + 1)}
            disabled={!canGoNextPage}
            aria-label="Go to next page"
          >
            <ChevronRightIcon className="size-4" />
          </Button>
          <Button
            variant="outline"
            size="icon-sm"
            onClick={() => onPageChange(pageCount - 1)}
            disabled={!canGoNextPage}
            aria-label="Go to last page"
          >
            <ChevronsRightIcon className="size-4" />
          </Button>
        </div>
      </div>
    </div>
  );
}

export { DataTablePagination };
