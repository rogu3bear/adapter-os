import React from 'react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import { Search, Filter, X, SlidersHorizontal } from 'lucide-react';
import type { AdapterState, AdapterCategory } from '@/api/adapter-types';
import type { AdapterFilters as FilterValues } from './useAdapters';

interface AdapterFiltersProps {
  filters: FilterValues;
  onFiltersChange: (filters: FilterValues) => void;
  tenants?: string[];
}

const STATUS_OPTIONS: { value: AdapterState; label: string }[] = [
  { value: 'unloaded', label: 'Unloaded' },
  { value: 'cold', label: 'Cold' },
  { value: 'warm', label: 'Warm' },
  { value: 'hot', label: 'Hot' },
  { value: 'resident', label: 'Resident' },
];

const TIER_OPTIONS = [
  { value: 'persistent', label: 'Persistent' },
  { value: 'warm', label: 'Warm' },
  { value: 'ephemeral', label: 'Ephemeral' },
];

const CATEGORY_OPTIONS: { value: AdapterCategory; label: string }[] = [
  { value: 'code', label: 'Code' },
  { value: 'framework', label: 'Framework' },
  { value: 'codebase', label: 'Codebase' },
  { value: 'ephemeral', label: 'Ephemeral' },
];

export function AdapterFilters({
  filters,
  onFiltersChange,
  tenants = [],
}: AdapterFiltersProps) {
  const activeFilterCount = countActiveFilters(filters);

  const updateFilter = <K extends keyof FilterValues>(
    key: K,
    value: FilterValues[K]
  ) => {
    onFiltersChange({ ...filters, [key]: value });
  };

  const clearFilters = () => {
    onFiltersChange({});
  };

  const toggleStatusFilter = (status: AdapterState) => {
    const currentStatuses = filters.status || [];
    const newStatuses = currentStatuses.includes(status)
      ? currentStatuses.filter(s => s !== status)
      : [...currentStatuses, status];
    updateFilter('status', newStatuses.length > 0 ? newStatuses : undefined);
  };

  const toggleTierFilter = (tier: string) => {
    const currentTiers = filters.tier || [];
    const newTiers = currentTiers.includes(tier)
      ? currentTiers.filter(t => t !== tier)
      : [...currentTiers, tier];
    updateFilter('tier', newTiers.length > 0 ? newTiers : undefined);
  };

  const toggleCategoryFilter = (category: AdapterCategory) => {
    const currentCategories = filters.category || [];
    const newCategories = currentCategories.includes(category)
      ? currentCategories.filter(c => c !== category)
      : [...currentCategories, category];
    updateFilter('category', newCategories.length > 0 ? newCategories : undefined);
  };

  return (
    <div className="flex flex-col gap-4 mb-6">
      {/* Search and Quick Filters Row */}
      <div className="flex items-center gap-3 flex-wrap">
        {/* Search Input */}
        <div className="relative flex-1 min-w-[200px] max-w-md">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
          <Input
            placeholder="Search adapters by name, ID, or framework..."
            value={filters.search || ''}
            onChange={e => updateFilter('search', e.target.value || undefined)}
            className="pl-10"
          />
        </div>

        {/* Tenant Filter */}
        {tenants.length > 0 && (
          <Select
            value={filters.tenant || 'all'}
            onValueChange={value =>
              updateFilter('tenant', value === 'all' ? undefined : value)
            }
          >
            <SelectTrigger className="w-[160px]">
              <SelectValue placeholder="All Tenants" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Tenants</SelectItem>
              {tenants.map(tenant => (
                <SelectItem key={tenant} value={tenant}>
                  {tenant}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}

        {/* Advanced Filters Popover */}
        <Popover>
          <PopoverTrigger asChild>
            <Button variant="outline" className="gap-2">
              <SlidersHorizontal className="h-4 w-4" />
              Filters
              {activeFilterCount > 0 && (
                <Badge variant="secondary" className="ml-1 h-5 w-5 p-0 flex items-center justify-center">
                  {activeFilterCount}
                </Badge>
              )}
            </Button>
          </PopoverTrigger>
          <PopoverContent className="w-80" align="end">
            <div className="space-y-4">
              {/* Status Filter */}
              <div className="space-y-2">
                <Label className="text-sm font-medium">Status</Label>
                <div className="flex flex-wrap gap-2">
                  {STATUS_OPTIONS.map(option => (
                    <button
                      key={option.value}
                      onClick={() => toggleStatusFilter(option.value)}
                      className={`px-2 py-1 text-xs rounded-md border transition-colors ${
                        filters.status?.includes(option.value)
                          ? 'bg-primary text-primary-foreground border-primary'
                          : 'bg-background hover:bg-accent border-input'
                      }`}
                    >
                      {option.label}
                    </button>
                  ))}
                </div>
              </div>

              {/* Tier Filter */}
              <div className="space-y-2">
                <Label className="text-sm font-medium">Tier</Label>
                <div className="flex flex-wrap gap-2">
                  {TIER_OPTIONS.map(option => (
                    <button
                      key={option.value}
                      onClick={() => toggleTierFilter(option.value)}
                      className={`px-2 py-1 text-xs rounded-md border transition-colors ${
                        filters.tier?.includes(option.value)
                          ? 'bg-primary text-primary-foreground border-primary'
                          : 'bg-background hover:bg-accent border-input'
                      }`}
                    >
                      {option.label}
                    </button>
                  ))}
                </div>
              </div>

              {/* Category Filter */}
              <div className="space-y-2">
                <Label className="text-sm font-medium">Category</Label>
                <div className="flex flex-wrap gap-2">
                  {CATEGORY_OPTIONS.map(option => (
                    <button
                      key={option.value}
                      onClick={() => toggleCategoryFilter(option.value)}
                      className={`px-2 py-1 text-xs rounded-md border transition-colors ${
                        filters.category?.includes(option.value)
                          ? 'bg-primary text-primary-foreground border-primary'
                          : 'bg-background hover:bg-accent border-input'
                      }`}
                    >
                      {option.label}
                    </button>
                  ))}
                </div>
              </div>

              {/* Pinned Only Checkbox */}
              <div className="flex items-center space-x-2">
                <Checkbox
                  id="pinned-only"
                  checked={filters.pinned === true}
                  onCheckedChange={checked =>
                    updateFilter('pinned', checked ? true : undefined)
                  }
                />
                <Label htmlFor="pinned-only" className="text-sm font-medium cursor-pointer">
                  Pinned only
                </Label>
              </div>
            </div>
          </PopoverContent>
        </Popover>

        {/* Clear Filters Button */}
        {activeFilterCount > 0 && (
          <Button variant="ghost" size="sm" onClick={clearFilters} className="gap-1">
            <X className="h-4 w-4" />
            Clear filters
          </Button>
        )}
      </div>

      {/* Active Filter Badges */}
      {activeFilterCount > 0 && (
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-sm text-muted-foreground">Active filters:</span>

          {filters.status?.map(status => (
            <Badge key={status} variant="secondary" className="gap-1">
              Status: {status}
              <button
                onClick={() => toggleStatusFilter(status)}
                className="ml-1 hover:text-destructive"
              >
                <X className="h-3 w-3" />
              </button>
            </Badge>
          ))}

          {filters.tier?.map(tier => (
            <Badge key={tier} variant="secondary" className="gap-1">
              {tier.charAt(0).toUpperCase() + tier.slice(1)}
              <button
                onClick={() => toggleTierFilter(tier)}
                className="ml-1 hover:text-destructive"
              >
                <X className="h-3 w-3" />
              </button>
            </Badge>
          ))}

          {filters.category?.map(category => (
            <Badge key={category} variant="secondary" className="gap-1">
              {category}
              <button
                onClick={() => toggleCategoryFilter(category)}
                className="ml-1 hover:text-destructive"
              >
                <X className="h-3 w-3" />
              </button>
            </Badge>
          ))}

          {filters.tenant && (
            <Badge variant="secondary" className="gap-1">
              Tenant: {filters.tenant}
              <button
                onClick={() => updateFilter('tenant', undefined)}
                className="ml-1 hover:text-destructive"
              >
                <X className="h-3 w-3" />
              </button>
            </Badge>
          )}

          {filters.pinned && (
            <Badge variant="secondary" className="gap-1">
              Pinned
              <button
                onClick={() => updateFilter('pinned', undefined)}
                className="ml-1 hover:text-destructive"
              >
                <X className="h-3 w-3" />
              </button>
            </Badge>
          )}
        </div>
      )}
    </div>
  );
}

function countActiveFilters(filters: FilterValues): number {
  let count = 0;
  if (filters.status && filters.status.length > 0) count += filters.status.length;
  if (filters.tier && filters.tier.length > 0) count += filters.tier.length;
  if (filters.category && filters.category.length > 0) count += filters.category.length;
  if (filters.tenant) count += 1;
  if (filters.pinned) count += 1;
  if (filters.search) count += 1;
  return count;
}

export default AdapterFilters;
