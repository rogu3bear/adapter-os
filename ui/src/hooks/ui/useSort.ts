/**
 * Sorting State Hook
 *
 * Manages sorting state for lists and tables.
 * Supports single and multi-column sorting.
 *
 * Usage:
 * ```tsx
 * const { sortConfig, handleSort, sortedData } = useSort({
 *   data: adapters,
 *   defaultSort: { key: 'name', direction: 'asc' },
 * });
 *
 * // In your table header
 * <TableHeader
 *   columns={columns}
 *   sortConfig={sortConfig}
 *   onSort={handleSort}
 * />
 * ```
 *
 * Citations:
 * - docs/UI_INTEGRATION.md - Table sorting patterns
 */

import { useState, useCallback, useMemo } from 'react';

export type SortDirection = 'asc' | 'desc';

export interface SortConfig<K extends string = string> {
  /** Column/field key to sort by */
  key: K;
  /** Sort direction */
  direction: SortDirection;
}

export interface MultiSortConfig<K extends string = string> {
  /** Array of sort configurations (in priority order) */
  sorts: SortConfig<K>[];
  /** Maximum number of sort columns allowed */
  maxSorts: number;
}

export interface UseSortOptions<T, K extends string = string> {
  /** Data to sort */
  data: T[];
  /** Default sort configuration */
  defaultSort?: SortConfig<K>;
  /** Enable multi-column sorting */
  multiSort?: boolean;
  /** Maximum columns for multi-sort (default: 3) */
  maxMultiSortColumns?: number;
  /** Custom sort comparator for specific keys */
  comparators?: Partial<Record<K, (a: T, b: T) => number>>;
  /** Callback when sort changes */
  onSortChange?: (config: SortConfig<K> | SortConfig<K>[]) => void;
}

export interface UseSortReturn<T, K extends string = string> {
  /** Current sort configuration */
  sortConfig: SortConfig<K> | null;
  /** Multi-sort configurations (if enabled) */
  multiSortConfig: SortConfig<K>[];
  /** Sorted data */
  sortedData: T[];
  /** Handle sort toggle/change for a column */
  handleSort: (key: K) => void;
  /** Set sort configuration directly */
  setSort: (config: SortConfig<K> | null) => void;
  /** Clear all sorting */
  clearSort: () => void;
  /** Check if column is currently sorted */
  isSorted: (key: K) => boolean;
  /** Get sort direction for a column */
  getSortDirection: (key: K) => SortDirection | null;
  /** Get sort priority for multi-sort (1-based, null if not sorted) */
  getSortPriority: (key: K) => number | null;
  /** Toggle sort direction for currently sorted column */
  toggleDirection: () => void;
}

/**
 * Default comparator for sorting values.
 * Handles strings, numbers, dates, booleans, and null/undefined.
 */
function defaultComparator<T>(a: T, b: T, key: string): number {
  const aValue = (a as Record<string, unknown>)[key];
  const bValue = (b as Record<string, unknown>)[key];

  // Handle null/undefined
  if (aValue == null && bValue == null) return 0;
  if (aValue == null) return 1;
  if (bValue == null) return -1;

  // Handle different types
  if (typeof aValue === 'string' && typeof bValue === 'string') {
    return aValue.localeCompare(bValue, undefined, { sensitivity: 'base' });
  }

  if (typeof aValue === 'number' && typeof bValue === 'number') {
    return aValue - bValue;
  }

  if (typeof aValue === 'boolean' && typeof bValue === 'boolean') {
    return aValue === bValue ? 0 : aValue ? -1 : 1;
  }

  // Handle dates
  if (aValue instanceof Date && bValue instanceof Date) {
    return aValue.getTime() - bValue.getTime();
  }

  // Handle date strings
  if (typeof aValue === 'string' && typeof bValue === 'string') {
    const aDate = Date.parse(aValue);
    const bDate = Date.parse(bValue);
    if (!isNaN(aDate) && !isNaN(bDate)) {
      return aDate - bDate;
    }
  }

  // Fallback to string comparison
  return String(aValue).localeCompare(String(bValue));
}

/**
 * Hook for managing sorting state.
 *
 * @param options - Sort configuration options
 * @returns Sort state and control functions
 */
export function useSort<T, K extends string = string>(
  options: UseSortOptions<T, K>
): UseSortReturn<T, K> {
  const {
    data,
    defaultSort = null,
    multiSort = false,
    maxMultiSortColumns = 3,
    comparators = {},
    onSortChange,
  } = options;

  const [sortConfigs, setSortConfigs] = useState<SortConfig<K>[]>(
    defaultSort ? [defaultSort] : []
  );

  const sortConfig = sortConfigs.length > 0 ? sortConfigs[0] : null;

  const sortedData = useMemo(() => {
    if (sortConfigs.length === 0) {
      return data;
    }

    return [...data].sort((a, b) => {
      for (const config of sortConfigs) {
        const customComparator = (comparators as Record<string, ((a: T, b: T) => number) | undefined>)[config.key];
        let comparison: number;

        if (customComparator) {
          comparison = customComparator(a, b);
        } else {
          comparison = defaultComparator(a, b, config.key);
        }

        if (comparison !== 0) {
          return config.direction === 'desc' ? -comparison : comparison;
        }
      }
      return 0;
    });
  }, [data, sortConfigs, comparators]);

  const handleSort = useCallback(
    (key: K) => {
      setSortConfigs((prev) => {
        const existingIndex = prev.findIndex((s) => s.key === key);

        let newConfigs: SortConfig<K>[];

        if (multiSort) {
          if (existingIndex === -1) {
            // Add new sort (up to max)
            const newSort: SortConfig<K> = { key, direction: 'asc' };
            newConfigs = [...prev, newSort].slice(-maxMultiSortColumns);
          } else {
            const existing = prev[existingIndex];
            if (existing.direction === 'asc') {
              // Toggle to desc
              newConfigs = [
                ...prev.slice(0, existingIndex),
                { ...existing, direction: 'desc' as const },
                ...prev.slice(existingIndex + 1),
              ];
            } else {
              // Remove sort
              newConfigs = [
                ...prev.slice(0, existingIndex),
                ...prev.slice(existingIndex + 1),
              ];
            }
          }
        } else {
          // Single sort mode
          if (existingIndex === -1) {
            newConfigs = [{ key, direction: 'asc' }];
          } else if (prev[0].direction === 'asc') {
            newConfigs = [{ key, direction: 'desc' }];
          } else {
            newConfigs = [];
          }
        }

        const configToReport = multiSort ? newConfigs : (newConfigs[0] ?? null);
        if (configToReport) {
          onSortChange?.(configToReport as SortConfig<K> | SortConfig<K>[]);
        }

        return newConfigs;
      });
    },
    [multiSort, maxMultiSortColumns, onSortChange]
  );

  const setSort = useCallback(
    (config: SortConfig<K> | null) => {
      const newConfigs = config ? [config] : [];
      setSortConfigs(newConfigs);
      if (config) {
        onSortChange?.(config);
      }
    },
    [onSortChange]
  );

  const clearSort = useCallback(() => {
    setSortConfigs([]);
  }, []);

  const isSorted = useCallback(
    (key: K): boolean => {
      return sortConfigs.some((s) => s.key === key);
    },
    [sortConfigs]
  );

  const getSortDirection = useCallback(
    (key: K): SortDirection | null => {
      const config = sortConfigs.find((s) => s.key === key);
      return config?.direction ?? null;
    },
    [sortConfigs]
  );

  const getSortPriority = useCallback(
    (key: K): number | null => {
      const index = sortConfigs.findIndex((s) => s.key === key);
      return index === -1 ? null : index + 1;
    },
    [sortConfigs]
  );

  const toggleDirection = useCallback(() => {
    if (sortConfigs.length > 0) {
      const [first, ...rest] = sortConfigs;
      const newFirst: SortConfig<K> = {
        ...first,
        direction: first.direction === 'asc' ? 'desc' : 'asc',
      };
      setSortConfigs([newFirst, ...rest]);
      onSortChange?.(newFirst);
    }
  }, [sortConfigs, onSortChange]);

  return {
    sortConfig,
    multiSortConfig: sortConfigs,
    sortedData,
    handleSort,
    setSort,
    clearSort,
    isSorted,
    getSortDirection,
    getSortPriority,
    toggleDirection,
  };
}

export default useSort;
