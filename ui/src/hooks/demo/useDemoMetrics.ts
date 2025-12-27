import { useMemo } from 'react';
import type { SystemMetrics } from '@/api/api-types';
import { useDemoMode } from './DemoProvider';

export function useDemoMetrics(metrics: SystemMetrics | null, lastUpdated: Date | null) {
  const { enabled, simulateTraffic, simulatedMetrics } = useDemoMode();

  const mergedMetrics = useMemo<SystemMetrics | null>(() => {
    if (!enabled || !simulateTraffic || !simulatedMetrics) return metrics;
    const base = metrics ?? {};
    return {
      ...base,
      ...simulatedMetrics.metrics,
      timestamp: new Date().toISOString(),
    };
  }, [enabled, metrics, simulateTraffic, simulatedMetrics]);

  const mergedLastUpdated = useMemo<Date | null>(() => {
    if (!enabled || !simulateTraffic) return lastUpdated;
    return simulatedMetrics?.updatedAt ?? lastUpdated ?? null;
  }, [enabled, lastUpdated, simulateTraffic, simulatedMetrics]);

  return { metrics: mergedMetrics, lastUpdated: mergedLastUpdated };
}
