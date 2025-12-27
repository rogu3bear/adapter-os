import { useMemo } from 'react';
import type { RecentActivityEvent } from '@/api/auth-types';
import { useDemoMode } from './DemoProvider';

export function useDemoActivity(events: RecentActivityEvent[] | null | undefined): RecentActivityEvent[] {
  const { enabled, seededActivity } = useDemoMode();

  return useMemo(() => {
    if (!enabled) return events ?? [];
    const base = events ?? [];
    if (base.length === 0) return seededActivity;
    if (base.length >= 10) return base;
    const remaining = 10 - base.length;
    return [...base, ...seededActivity.slice(0, remaining)];
  }, [enabled, events, seededActivity]);
}
