/**
 * Filter State Hook
 *
 * Manages filter state with optional URL synchronization.
 * Supports multiple filter types and operators.
 *
 * Usage:
 * ```tsx
 * const { filters, setFilter, clearFilters, filteredData } = useFilter({
 *   data: adapters,
 *   filterConfigs: {
 *     status: { type: 'select', options: ['active', 'inactive'] },
 *     name: { type: 'search' },
 *     createdAt: { type: 'dateRange' },
 *   },
 *   syncToUrl: true,
 * });
 * ```
 *
 * Citations:
 * - docs/UI_INTEGRATION.md - Filter patterns
 */

import { useState, useCallback, useMemo, useEffect } from 'react';

export type FilterType = 'search' | 'select' | 'multiSelect' | 'dateRange' | 'numberRange' | 'boolean';

export type FilterOperator = 'equals' | 'contains' | 'startsWith' | 'endsWith' | 'gt' | 'gte' | 'lt' | 'lte' | 'between' | 'in';

export interface FilterConfig {
  /** Type of filter */
  type: FilterType;
  /** Available options for select/multiSelect */
  options?: string[];
  /** Placeholder text */
  placeholder?: string;
  /** Default value */
  defaultValue?: FilterValue;
  /** Custom filter function */
  customFilter?: <T>(item: T, value: FilterValue) => boolean;
  /** Label for display */
  label?: string;
}

export type FilterValue =
  | string
  | string[]
  | boolean
  | number
  | { start: string | number; end: string | number }
  | null;

export interface ActiveFilter<K extends string = string> {
  key: K;
  value: FilterValue;
  operator: FilterOperator;
}

export interface UseFilterOptions<T, K extends string = string> {
  /** Data to filter */
  data: T[];
  /** Filter configuration for each filterable field */
  filterConfigs: Partial<Record<K, FilterConfig>>;
  /** Sync filter state to URL query parameters */
  syncToUrl?: boolean;
  /** URL parameter prefix (default: 'filter_') */
  urlPrefix?: string;
  /** Debounce delay for search filters in ms (default: 300) */
  searchDebounceMs?: number;
  /** Callback when filters change */
  onFiltersChange?: (filters: Record<K, FilterValue>) => void;
}

export interface UseFilterReturn<T, K extends string = string> {
  /** Current filter values */
  filters: Partial<Record<K, FilterValue>>;
  /** Active filters as array */
  activeFilters: ActiveFilter<K>[];
  /** Filtered data */
  filteredData: T[];
  /** Set a single filter value */
  setFilter: (key: K, value: FilterValue) => void;
  /** Set multiple filters at once */
  setFilters: (filters: Partial<Record<K, FilterValue>>) => void;
  /** Clear a single filter */
  clearFilter: (key: K) => void;
  /** Clear all filters */
  clearFilters: () => void;
  /** Reset filters to default values */
  resetFilters: () => void;
  /** Check if any filters are active */
  hasActiveFilters: boolean;
  /** Get filter config for a key */
  getFilterConfig: (key: K) => FilterConfig | undefined;
  /** Number of active filters */
  activeFilterCount: number;
}

/**
 * Parse filter value from URL string.
 */
function parseUrlValue(value: string, type: FilterType): FilterValue {
  switch (type) {
    case 'boolean':
      return value === 'true';
    case 'multiSelect':
      return value.split(',');
    case 'numberRange':
    case 'dateRange': {
      const [start, end] = value.split('|');
      return { start, end };
    }
    default:
      return value;
  }
}

/**
 * Serialize filter value to URL string.
 */
function serializeUrlValue(value: FilterValue): string {
  if (value === null || value === undefined) {
    return '';
  }
  if (Array.isArray(value)) {
    return value.join(',');
  }
  if (typeof value === 'object' && 'start' in value) {
    return `${value.start}|${value.end}`;
  }
  return String(value);
}

/**
 * Default filter matching function.
 */
function defaultFilterMatch<T>(
  item: T,
  key: string,
  value: FilterValue,
  type: FilterType
): boolean {
  if (value === null || value === undefined || value === '') {
    return true;
  }

  const itemValue = (item as Record<string, unknown>)[key];

  switch (type) {
    case 'search': {
      if (typeof value !== 'string') return true;
      const searchValue = value.toLowerCase();
      const targetValue = String(itemValue ?? '').toLowerCase();
      return targetValue.includes(searchValue);
    }

    case 'select': {
      return itemValue === value;
    }

    case 'multiSelect': {
      if (!Array.isArray(value) || value.length === 0) return true;
      return value.includes(String(itemValue));
    }

    case 'boolean': {
      return itemValue === value;
    }

    case 'numberRange': {
      if (typeof value !== 'object' || !('start' in value)) return true;
      const numValue = Number(itemValue);
      const { start, end } = value;
      if (start !== '' && start !== null && numValue < Number(start)) return false;
      if (end !== '' && end !== null && numValue > Number(end)) return false;
      return true;
    }

    case 'dateRange': {
      if (typeof value !== 'object' || !('start' in value)) return true;
      const dateValue = new Date(String(itemValue)).getTime();
      const { start, end } = value;
      if (start && dateValue < new Date(String(start)).getTime()) return false;
      if (end && dateValue > new Date(String(end)).getTime()) return false;
      return true;
    }

    default:
      return true;
  }
}

/**
 * Hook for managing filter state with optional URL synchronization.
 *
 * @param options - Filter configuration options
 * @returns Filter state and control functions
 */
export function useFilter<T, K extends string = string>(
  options: UseFilterOptions<T, K>
): UseFilterReturn<T, K> {
  const {
    data,
    filterConfigs,
    syncToUrl = false,
    urlPrefix = 'filter_',
    onFiltersChange,
  } = options;

  // Initialize filters from URL or defaults
  const getInitialFilters = useCallback((): Partial<Record<K, FilterValue>> => {
    const initial: Partial<Record<K, FilterValue>> = {};

    // Set defaults from config
    for (const [key, config] of Object.entries(filterConfigs) as [K, FilterConfig][]) {
      if (config.defaultValue !== undefined) {
        initial[key] = config.defaultValue;
      }
    }

    // Override with URL values if syncToUrl is enabled
    if (syncToUrl && typeof window !== 'undefined') {
      const params = new URLSearchParams(window.location.search);
      for (const [key, config] of Object.entries(filterConfigs) as [K, FilterConfig][]) {
        const urlKey = `${urlPrefix}${key}`;
        const urlValue = params.get(urlKey);
        if (urlValue) {
          initial[key] = parseUrlValue(urlValue, config.type);
        }
      }
    }

    return initial;
  }, [filterConfigs, syncToUrl, urlPrefix]);

  const [filters, setFiltersState] = useState<Partial<Record<K, FilterValue>>>(getInitialFilters);

  // Sync to URL when filters change
  useEffect(() => {
    if (!syncToUrl || typeof window === 'undefined') return;

    const params = new URLSearchParams(window.location.search);

    // Remove all existing filter params
    for (const key of Object.keys(filterConfigs)) {
      params.delete(`${urlPrefix}${key}`);
    }

    // Add current filter values
    for (const [key, value] of Object.entries(filters)) {
      if (value !== null && value !== undefined && value !== '') {
        const serialized = serializeUrlValue(value as FilterValue);
        if (serialized) {
          params.set(`${urlPrefix}${key}`, serialized);
        }
      }
    }

    const newUrl = params.toString()
      ? `${window.location.pathname}?${params.toString()}`
      : window.location.pathname;

    window.history.replaceState({}, '', newUrl);
  }, [filters, syncToUrl, urlPrefix, filterConfigs]);

  // Calculate filtered data
  const filteredData = useMemo(() => {
    return data.filter((item) => {
      for (const [key, value] of Object.entries(filters) as [K, FilterValue][]) {
        const config = filterConfigs[key];
        if (!config) continue;

        if (config.customFilter) {
          if (!config.customFilter(item, value)) return false;
        } else {
          if (!defaultFilterMatch(item, key, value, config.type)) return false;
        }
      }
      return true;
    });
  }, [data, filters, filterConfigs]);

  // Calculate active filters
  const activeFilters = useMemo((): ActiveFilter<K>[] => {
    return Object.entries(filters)
      .filter(([, value]) => {
        if (value === null || value === undefined || value === '') return false;
        if (Array.isArray(value) && value.length === 0) return false;
        return true;
      })
      .map(([key, value]) => ({
        key: key as K,
        value: value as FilterValue,
        operator: 'equals' as FilterOperator,
      }));
  }, [filters]);

  const setFilter = useCallback(
    (key: K, value: FilterValue) => {
      setFiltersState((prev) => {
        const newFilters = { ...prev, [key]: value };
        onFiltersChange?.(newFilters as Record<K, FilterValue>);
        return newFilters;
      });
    },
    [onFiltersChange]
  );

  const setFilters = useCallback(
    (newFilters: Partial<Record<K, FilterValue>>) => {
      setFiltersState((prev) => {
        const merged = { ...prev, ...newFilters };
        onFiltersChange?.(merged as Record<K, FilterValue>);
        return merged;
      });
    },
    [onFiltersChange]
  );

  const clearFilter = useCallback(
    (key: K) => {
      setFiltersState((prev) => {
        const newFilters = { ...prev };
        delete newFilters[key];
        onFiltersChange?.(newFilters as Record<K, FilterValue>);
        return newFilters;
      });
    },
    [onFiltersChange]
  );

  const clearFilters = useCallback(() => {
    setFiltersState({});
    onFiltersChange?.({} as Record<K, FilterValue>);
  }, [onFiltersChange]);

  const resetFilters = useCallback(() => {
    const defaults = getInitialFilters();
    setFiltersState(defaults);
    onFiltersChange?.(defaults as Record<K, FilterValue>);
  }, [getInitialFilters, onFiltersChange]);

  const getFilterConfig = useCallback(
    (key: K): FilterConfig | undefined => {
      return filterConfigs[key];
    },
    [filterConfigs]
  );

  return {
    filters,
    activeFilters,
    filteredData,
    setFilter,
    setFilters,
    clearFilter,
    clearFilters,
    resetFilters,
    hasActiveFilters: activeFilters.length > 0,
    getFilterConfig,
    activeFilterCount: activeFilters.length,
  };
}

export default useFilter;
