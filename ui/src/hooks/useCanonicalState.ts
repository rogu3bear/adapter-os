/**
 * Canonical state management hook
 * 
 * Enforces deterministic ordering of state arrays to ensure reproducible UI rendering.
 */

import { useState, useEffect } from 'react';
import { stableSort } from '../components/ui/utils';

export function useCanonicalState<T extends { id?: string; hash?: string; timestamp?: string; created_at?: string }>(
  initialData: T[],
  sortKeys: (keyof T)[] = ['timestamp', 'id']
): [T[], (data: T[]) => void] {
  const [data, setData] = useState<T[]>([]);

  useEffect(() => {
    // Enforce canonical ordering on mount and updates

    // Note: sortKeys is compared by reference. Callers should pass stable array references
    // (e.g., useMemo or constant array) to avoid unnecessary re-sorts.

>
    const sorted = stableSort(initialData, sortKeys);
    setData(sorted);
  }, [initialData, sortKeys]);

  const setCanonicalData = (newData: T[]) => {
    const sorted = stableSort(newData, sortKeys);
    setData(sorted);
  };

  return [data, setCanonicalData];
}

