import { useMemo } from 'react';
import { useDemoMode } from './DemoProvider';

export function useDemoActivity<T extends { id: string }>(events: T[] | null | undefined): T[] {
  const { enabled, seededActivity } = useDemoMode();

  return useMemo(() => {
    if (!enabled) return events ?? [];
    const base = events ?? [];
    if (base.length === 0) return seededActivity as unknown as T[];
    if (base.length >= 10) return base;
    const remaining = 10 - base.length;
    return [...base, ...(seededActivity.slice(0, remaining) as unknown as T[])];
  }, [enabled, events, seededActivity]);
}
