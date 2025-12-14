/**
 * Canonical state management hook
 *
 * Enforces deterministic ordering of state arrays to ensure reproducible UI rendering.
 *
 * @param initialData - The initial data array to sort
 * @param sortKeys - Array of keys to sort by (default: ['timestamp', 'id'])
 *
 * **IMPORTANT**: The `sortKeys` parameter is compared by reference in the dependency array.
 * Callers MUST memoize `sortKeys` (e.g., using `useMemo` or a constant array outside the component)
 * to avoid unnecessary re-renders and re-sorts on every render.
 *
 * @example
 * ```tsx
 * // Good: sortKeys is a constant
 * const SORT_KEYS = ['timestamp', 'id'] as const;
 * const [data, setData] = useCanonicalState(items, SORT_KEYS);
 *
 * // Good: sortKeys is memoized
 * const sortKeys = useMemo(() => ['timestamp', 'id'], []);
 * const [data, setData] = useCanonicalState(items, sortKeys);
 *
 * // Bad: new array reference on every render
 * const [data, setData] = useCanonicalState(items, ['timestamp', 'id']); // Will cause re-renders!
 * ```
 */

import { useState, useEffect } from 'react';
import { stableSort } from '@/components/ui/utils';

export function useCanonicalState<T extends { id?: string; hash?: string; timestamp?: string; created_at?: string }>(
  initialData: T[],
  sortKeys: (keyof T)[] = ['timestamp', 'id']
): [T[], (data: T[]) => void] {
  const [data, setData] = useState<T[]>([]);

  useEffect(() => {
    // Enforce canonical ordering on mount and updates
    const sorted = stableSort(initialData, sortKeys);
    setData(sorted);
  }, [initialData, sortKeys]);

  const setCanonicalData = (newData: T[]) => {
    const sorted = stableSort(newData, sortKeys);
    setData(sorted);
  };

  return [data, setCanonicalData];
}

