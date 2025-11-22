"use client";

import * as React from "react";
import { XIcon, SaveIcon, RotateCcwIcon, CheckIcon } from "lucide-react";

import { cn } from "../../ui/utils";
import { Button } from "../../ui/button";
import { Badge } from "../../ui/badge";
import { Input } from "../../ui/input";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "../../ui/popover";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "../../ui/select";

export interface FilterDefinition {
  id: string;
  label: string;
  type: "text" | "select" | "multi-select" | "date" | "date-range" | "number";
  options?: { value: string; label: string }[];
  placeholder?: string;
}

export interface ActiveFilter {
  id: string;
  label: string;
  value: string | string[];
  displayValue?: string;
}

export interface SavedFilterSet {
  id: string;
  name: string;
  filters: ActiveFilter[];
}

export interface DataTableFiltersProps {
  /** Available filter definitions */
  filterDefinitions?: FilterDefinition[];
  /** Currently active filters */
  activeFilters: ActiveFilter[];
  /** Callback when a filter is added or updated */
  onFilterChange: (filterId: string, value: string | string[] | null) => void;
  /** Callback when a filter is removed */
  onFilterRemove: (filterId: string) => void;
  /** Callback to clear all filters */
  onClearAll: () => void;
  /** Saved filter sets */
  savedFilterSets?: SavedFilterSet[];
  /** Callback to save current filter set */
  onSaveFilterSet?: (name: string) => void;
  /** Callback to apply a saved filter set */
  onApplyFilterSet?: (filterSet: SavedFilterSet) => void;
  /** Callback to delete a saved filter set */
  onDeleteFilterSet?: (filterSetId: string) => void;
  /** Whether filters are in compact chip mode */
  compactMode?: boolean;
  /** Additional class names */
  className?: string;
}

function FilterChip({
  filter,
  onRemove,
}: {
  filter: ActiveFilter;
  onRemove: () => void;
}) {
  const displayValue = filter.displayValue || (
    Array.isArray(filter.value)
      ? filter.value.join(", ")
      : String(filter.value)
  );

  return (
    <Badge
      variant="secondary"
      className="gap-1 pr-1 max-w-[200px]"
    >
      <span className="font-medium">{filter.label}:</span>
      <span className="truncate">{displayValue}</span>
      <Button
        variant="ghost"
        size="icon-xs"
        className="ml-1 size-4 rounded-full hover:bg-muted-foreground/20"
        onClick={(e) => {
          e.stopPropagation();
          onRemove();
        }}
        aria-label={`Remove ${filter.label} filter`}
      >
        <XIcon className="size-3" />
      </Button>
    </Badge>
  );
}

function FilterEditor({
  definition,
  value,
  onChange,
}: {
  definition: FilterDefinition;
  value: string | string[] | undefined;
  onChange: (value: string | string[] | null) => void;
}) {
  const stringValue = Array.isArray(value) ? value[0] || "" : value || "";

  switch (definition.type) {
    case "select":
      return (
        <Select
          value={stringValue}
          onValueChange={(v) => onChange(v || null)}
        >
          <SelectTrigger size="sm">
            <SelectValue placeholder={definition.placeholder || "Select..."} />
          </SelectTrigger>
          <SelectContent>
            {definition.options?.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      );

    case "multi-select":
      const multiValue = Array.isArray(value) ? value : value ? [value] : [];
      return (
        <div className="flex flex-wrap gap-1">
          {definition.options?.map((option) => {
            const isSelected = multiValue.includes(option.value);
            return (
              <Badge
                key={option.value}
                variant={isSelected ? "default" : "outline"}
                className="cursor-pointer"
                onClick={() => {
                  const newValue = isSelected
                    ? multiValue.filter((v) => v !== option.value)
                    : [...multiValue, option.value];
                  onChange(newValue.length > 0 ? newValue : null);
                }}
              >
                {isSelected && <CheckIcon className="size-3 mr-1" />}
                {option.label}
              </Badge>
            );
          })}
        </div>
      );

    case "number":
      return (
        <Input
          type="number"
          placeholder={definition.placeholder || "Enter number..."}
          value={stringValue}
          onChange={(e) => onChange(e.target.value || null)}
          className="h-8"
        />
      );

    case "date":
      return (
        <Input
          type="date"
          value={stringValue}
          onChange={(e) => onChange(e.target.value || null)}
          className="h-8"
        />
      );

    case "text":
    default:
      return (
        <Input
          type="text"
          placeholder={definition.placeholder || "Enter value..."}
          value={stringValue}
          onChange={(e) => onChange(e.target.value || null)}
          className="h-8"
        />
      );
  }
}

function DataTableFilters({
  filterDefinitions = [],
  activeFilters,
  onFilterChange,
  onFilterRemove,
  onClearAll,
  savedFilterSets = [],
  onSaveFilterSet,
  onApplyFilterSet,
  onDeleteFilterSet,
  compactMode = true,
  className,
}: DataTableFiltersProps) {
  const [saveDialogOpen, setSaveDialogOpen] = React.useState(false);
  const [filterSetName, setFilterSetName] = React.useState("");

  const hasActiveFilters = activeFilters.length > 0;

  const handleSaveFilterSet = () => {
    if (filterSetName.trim() && onSaveFilterSet) {
      onSaveFilterSet(filterSetName.trim());
      setFilterSetName("");
      setSaveDialogOpen(false);
    }
  };

  // Get current filter values for editing
  const getFilterValue = (filterId: string) => {
    const filter = activeFilters.find((f) => f.id === filterId);
    return filter?.value;
  };

  if (compactMode) {
    return (
      <div
        data-slot="data-table-filters"
        className={cn(
          "flex flex-wrap items-center gap-2",
          className
        )}
      >
        {/* Active filter chips */}
        {activeFilters.map((filter) => (
          <FilterChip
            key={filter.id}
            filter={filter}
            onRemove={() => onFilterRemove(filter.id)}
          />
        ))}

        {/* Clear all button */}
        {hasActiveFilters && (
          <Button
            variant="ghost"
            size="xs"
            onClick={onClearAll}
            className="text-muted-foreground hover:text-foreground"
          >
            <RotateCcwIcon className="size-3 mr-1" />
            Clear all
          </Button>
        )}

        {/* Save filter set */}
        {hasActiveFilters && onSaveFilterSet && (
          <Popover open={saveDialogOpen} onOpenChange={setSaveDialogOpen}>
            <PopoverTrigger asChild>
              <Button
                variant="ghost"
                size="xs"
                className="text-muted-foreground hover:text-foreground"
              >
                <SaveIcon className="size-3 mr-1" />
                Save
              </Button>
            </PopoverTrigger>
            <PopoverContent className="w-64">
              <div className="flex flex-col gap-3">
                <h4 className="font-medium text-sm">Save Filter Set</h4>
                <Input
                  placeholder="Filter set name..."
                  value={filterSetName}
                  onChange={(e) => setFilterSetName(e.target.value)}
                  className="h-8"
                />
                <div className="flex justify-end gap-2">
                  <Button
                    variant="ghost"
                    size="xs"
                    onClick={() => setSaveDialogOpen(false)}
                  >
                    Cancel
                  </Button>
                  <Button
                    size="xs"
                    onClick={handleSaveFilterSet}
                    disabled={!filterSetName.trim()}
                  >
                    Save
                  </Button>
                </div>
              </div>
            </PopoverContent>
          </Popover>
        )}

        {/* Saved filter sets */}
        {savedFilterSets.length > 0 && onApplyFilterSet && (
          <Select
            value=""
            onValueChange={(value) => {
              const filterSet = savedFilterSets.find((fs) => fs.id === value);
              if (filterSet) {
                onApplyFilterSet(filterSet);
              }
            }}
          >
            <SelectTrigger size="sm" className="w-[140px] h-7">
              <SelectValue placeholder="Saved filters" />
            </SelectTrigger>
            <SelectContent>
              {savedFilterSets.map((filterSet) => (
                <SelectItem key={filterSet.id} value={filterSet.id}>
                  <div className="flex items-center justify-between w-full">
                    <span>{filterSet.name}</span>
                    {onDeleteFilterSet && (
                      <Button
                        variant="ghost"
                        size="icon-xs"
                        className="ml-2 size-4"
                        onClick={(e) => {
                          e.stopPropagation();
                          onDeleteFilterSet(filterSet.id);
                        }}
                      >
                        <XIcon className="size-3" />
                      </Button>
                    )}
                  </div>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </div>
    );
  }

  // Extended filter panel mode
  return (
    <div
      data-slot="data-table-filters"
      className={cn(
        "flex flex-col gap-4 rounded-lg border bg-card p-4",
        className
      )}
    >
      <div className="flex items-center justify-between">
        <h4 className="font-medium">Filters</h4>
        {hasActiveFilters && (
          <Button
            variant="ghost"
            size="xs"
            onClick={onClearAll}
            className="text-muted-foreground hover:text-foreground"
          >
            <RotateCcwIcon className="size-3 mr-1" />
            Clear all
          </Button>
        )}
      </div>

      {/* Filter editors */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {filterDefinitions.map((definition) => (
          <div key={definition.id} className="flex flex-col gap-1.5">
            <label className="text-sm font-medium text-muted-foreground">
              {definition.label}
            </label>
            <FilterEditor
              definition={definition}
              value={getFilterValue(definition.id)}
              onChange={(value) => onFilterChange(definition.id, value)}
            />
          </div>
        ))}
      </div>

      {/* Active filter chips */}
      {hasActiveFilters && (
        <div className="flex flex-wrap items-center gap-2 pt-2 border-t">
          <span className="text-sm text-muted-foreground">Active:</span>
          {activeFilters.map((filter) => (
            <FilterChip
              key={filter.id}
              filter={filter}
              onRemove={() => onFilterRemove(filter.id)}
            />
          ))}
        </div>
      )}

      {/* Saved filter sets section */}
      {(onSaveFilterSet || savedFilterSets.length > 0) && (
        <div className="flex items-center justify-between pt-2 border-t">
          <div className="flex items-center gap-2">
            {savedFilterSets.length > 0 && onApplyFilterSet && (
              <>
                <span className="text-sm text-muted-foreground">
                  Saved filters:
                </span>
                <div className="flex flex-wrap gap-1">
                  {savedFilterSets.map((filterSet) => (
                    <Badge
                      key={filterSet.id}
                      variant="outline"
                      className="cursor-pointer gap-1 pr-1"
                      onClick={() => onApplyFilterSet(filterSet)}
                    >
                      {filterSet.name}
                      {onDeleteFilterSet && (
                        <Button
                          variant="ghost"
                          size="icon-xs"
                          className="ml-1 size-4 rounded-full hover:bg-muted-foreground/20"
                          onClick={(e) => {
                            e.stopPropagation();
                            onDeleteFilterSet(filterSet.id);
                          }}
                        >
                          <XIcon className="size-3" />
                        </Button>
                      )}
                    </Badge>
                  ))}
                </div>
              </>
            )}
          </div>

          {hasActiveFilters && onSaveFilterSet && (
            <Popover open={saveDialogOpen} onOpenChange={setSaveDialogOpen}>
              <PopoverTrigger asChild>
                <Button variant="outline" size="sm">
                  <SaveIcon className="size-4 mr-2" />
                  Save current filters
                </Button>
              </PopoverTrigger>
              <PopoverContent className="w-64">
                <div className="flex flex-col gap-3">
                  <h4 className="font-medium text-sm">Save Filter Set</h4>
                  <Input
                    placeholder="Filter set name..."
                    value={filterSetName}
                    onChange={(e) => setFilterSetName(e.target.value)}
                    className="h-8"
                  />
                  <div className="flex justify-end gap-2">
                    <Button
                      variant="ghost"
                      size="xs"
                      onClick={() => setSaveDialogOpen(false)}
                    >
                      Cancel
                    </Button>
                    <Button
                      size="xs"
                      onClick={handleSaveFilterSet}
                      disabled={!filterSetName.trim()}
                    >
                      Save
                    </Button>
                  </div>
                </div>
              </PopoverContent>
            </Popover>
          )}
        </div>
      )}
    </div>
  );
}

export { DataTableFilters, FilterChip };
