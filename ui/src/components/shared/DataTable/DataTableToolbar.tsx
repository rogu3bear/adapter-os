"use client";

import * as React from "react";
import {
  SearchIcon,
  FilterIcon,
  DownloadIcon,
  XIcon,
  MoreHorizontalIcon,
  Trash2Icon,
  CopyIcon,
  ArchiveIcon,
} from "lucide-react";

import { cn } from "../../ui/utils";
import { Button } from "../../ui/button";
import { Input } from "../../ui/input";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "../../ui/dropdown-menu";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "../../ui/popover";

export interface FilterOption {
  id: string;
  label: string;
  value: string | string[];
}

export interface BulkAction {
  id: string;
  label: string;
  icon?: React.ReactNode;
  variant?: "default" | "destructive";
  onClick: (selectedIds: string[]) => void;
}

export interface ExportFormat {
  id: string;
  label: string;
  extension: string;
}

export interface DataTableToolbarProps {
  /** Search input value */
  searchValue?: string;
  /** Callback when search value changes */
  onSearchChange?: (value: string) => void;
  /** Search placeholder text */
  searchPlaceholder?: string;
  /** Active filters */
  activeFilters?: FilterOption[];
  /** Callback when filter button is clicked */
  onFilterClick?: () => void;
  /** Number of active filters */
  activeFilterCount?: number;
  /** Selected row IDs for bulk actions */
  selectedIds?: string[];
  /** Available bulk actions */
  bulkActions?: BulkAction[];
  /** Export formats available */
  exportFormats?: ExportFormat[];
  /** Callback when export is triggered */
  onExport?: (format: ExportFormat) => void;
  /** Custom actions to render */
  customActions?: React.ReactNode;
  /** Filter content component */
  filterContent?: React.ReactNode;
  /** Whether filters popover is open */
  filtersOpen?: boolean;
  /** Callback when filters popover open state changes */
  onFiltersOpenChange?: (open: boolean) => void;
  /** Additional class names */
  className?: string;
}

const defaultExportFormats: ExportFormat[] = [
  { id: "csv", label: "CSV", extension: ".csv" },
  { id: "json", label: "JSON", extension: ".json" },
  { id: "xlsx", label: "Excel", extension: ".xlsx" },
];

const defaultBulkActions: BulkAction[] = [
  {
    id: "copy",
    label: "Copy",
    icon: <CopyIcon className="size-4" />,
    onClick: () => {},
  },
  {
    id: "archive",
    label: "Archive",
    icon: <ArchiveIcon className="size-4" />,
    onClick: () => {},
  },
  {
    id: "delete",
    label: "Delete",
    icon: <Trash2Icon className="size-4" />,
    variant: "destructive",
    onClick: () => {},
  },
];

function DataTableToolbar({
  searchValue = "",
  onSearchChange,
  searchPlaceholder = "Search...",
  activeFilters = [],
  onFilterClick,
  activeFilterCount = 0,
  selectedIds = [],
  bulkActions = defaultBulkActions,
  exportFormats = defaultExportFormats,
  onExport,
  customActions,
  filterContent,
  filtersOpen,
  onFiltersOpenChange,
  className,
}: DataTableToolbarProps) {
  const hasSelection = selectedIds.length > 0;
  const hasActiveFilters = activeFilterCount > 0 || activeFilters.length > 0;
  const filterCount = activeFilterCount || activeFilters.length;

  return (
    <div
      data-slot="data-table-toolbar"
      className={cn(
        "flex flex-col gap-4 px-2 py-4 sm:flex-row sm:items-center sm:justify-between",
        className
      )}
    >
      {/* Left side: Search and filters */}
      <div className="flex flex-1 flex-col gap-2 sm:flex-row sm:items-center sm:gap-2">
        {/* Search input */}
        {onSearchChange && (
          <div className="relative flex-1 sm:max-w-sm">
            <SearchIcon className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              placeholder={searchPlaceholder}
              value={searchValue}
              onChange={(e) => onSearchChange(e.target.value)}
              className="pl-9 pr-9"
            />
            {searchValue && (
              <Button
                variant="ghost"
                size="icon-xs"
                className="absolute right-1 top-1/2 -translate-y-1/2"
                onClick={() => onSearchChange("")}
                aria-label="Clear search"
              >
                <XIcon className="size-3" />
              </Button>
            )}
          </div>
        )}

        {/* Filter button */}
        {(onFilterClick || filterContent) && (
          filterContent ? (
            <Popover open={filtersOpen} onOpenChange={onFiltersOpenChange}>
              <PopoverTrigger asChild>
                <Button
                  variant="outline"
                  size="sm"
                  className={cn(
                    "gap-2",
                    hasActiveFilters && "border-primary text-primary"
                  )}
                >
                  <FilterIcon className="size-4" />
                  <span className="hidden sm:inline">Filters</span>
                  {hasActiveFilters && (
                    <span className="flex size-5 items-center justify-center rounded-full bg-primary text-xs text-primary-foreground">
                      {filterCount}
                    </span>
                  )}
                </Button>
              </PopoverTrigger>
              <PopoverContent align="start" className="w-80">
                {filterContent}
              </PopoverContent>
            </Popover>
          ) : (
            <Button
              variant="outline"
              size="sm"
              onClick={onFilterClick}
              className={cn(
                "gap-2",
                hasActiveFilters && "border-primary text-primary"
              )}
            >
              <FilterIcon className="size-4" />
              <span className="hidden sm:inline">Filters</span>
              {hasActiveFilters && (
                <span className="flex size-5 items-center justify-center rounded-full bg-primary text-xs text-primary-foreground">
                  {filterCount}
                </span>
              )}
            </Button>
          )
        )}
      </div>

      {/* Right side: Actions */}
      <div className="flex items-center gap-2">
        {/* Bulk actions (shown when items selected) */}
        {hasSelection && bulkActions.length > 0 && (
          <div className="flex items-center gap-2">
            <span className="text-sm text-muted-foreground">
              {selectedIds.length} selected
            </span>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="outline" size="sm" className="gap-2">
                  <MoreHorizontalIcon className="size-4" />
                  <span className="hidden sm:inline">Actions</span>
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuLabel>Bulk Actions</DropdownMenuLabel>
                <DropdownMenuSeparator />
                {bulkActions.map((action) => (
                  <DropdownMenuItem
                    key={action.id}
                    onClick={() => action.onClick(selectedIds)}
                    variant={action.variant}
                  >
                    {action.icon}
                    {action.label}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        )}

        {/* Custom actions */}
        {customActions}

        {/* Export button */}
        {onExport && exportFormats.length > 0 && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline" size="sm" className="gap-2">
                <DownloadIcon className="size-4" />
                <span className="hidden sm:inline">Export</span>
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuLabel>Export as</DropdownMenuLabel>
              <DropdownMenuSeparator />
              {exportFormats.map((format) => (
                <DropdownMenuItem
                  key={format.id}
                  onClick={() => onExport(format)}
                >
                  {format.label} ({format.extension})
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        )}
      </div>
    </div>
  );
}

export { DataTableToolbar };
