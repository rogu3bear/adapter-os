/**
 * Hook for computing worker cache health from cache metrics.
 *
 * Thresholds:
 * - 0-74%: Healthy (green)
 * - 75-89%: Warning (amber)
 * - 90%+: Critical (red)
 */

import { useMemo } from 'react';
import type { WorkerResponse, WorkerCacheHealth } from '@/api/types';

export interface CacheHealthSummary {
  /** Number of workers with healthy cache */
  healthy: number;
  /** Number of workers with warning cache */
  warning: number;
  /** Number of workers with critical cache */
  critical: number;
  /** Number of workers with unknown cache (no metrics) */
  unknown: number;
  /** Total workers */
  total: number;
}

/**
 * Compute cache health status from utilization percentage.
 */
export function getCacheHealthStatus(utilizationPct: number): 'healthy' | 'warning' | 'critical' {
  if (utilizationPct >= 90) return 'critical';
  if (utilizationPct >= 75) return 'warning';
  return 'healthy';
}

/**
 * Compute cache utilization percentage from worker metrics.
 * Returns undefined if metrics are not available.
 */
export function computeCacheUtilization(worker: WorkerResponse): number | undefined {
  if (worker.cache_used_mb === undefined || worker.cache_max_mb === undefined) {
    return undefined;
  }
  if (worker.cache_max_mb === 0) {
    return 0;
  }
  return Math.round((worker.cache_used_mb / worker.cache_max_mb) * 100);
}

/**
 * Transform worker response into cache health object.
 * Returns undefined if cache metrics are not available.
 */
export function getWorkerCacheHealth(worker: WorkerResponse): WorkerCacheHealth | undefined {
  const utilizationPct = computeCacheUtilization(worker);
  if (utilizationPct === undefined) {
    return undefined;
  }

  return {
    worker_id: worker.id || worker.worker_id,
    utilization_pct: utilizationPct,
    status: getCacheHealthStatus(utilizationPct),
    cache_used_mb: worker.cache_used_mb!,
    cache_max_mb: worker.cache_max_mb!,
    cache_pinned_entries: worker.cache_pinned_entries ?? 0,
    cache_active_entries: worker.cache_active_entries ?? 0,
  };
}

/**
 * Hook to compute cache health for a list of workers.
 *
 * @param workers - List of worker responses from the API
 * @returns Object containing per-worker health and aggregate summary
 */
export function useWorkerCacheHealth(workers: WorkerResponse[] | undefined) {
  return useMemo(() => {
    if (!workers) {
      return {
        workerHealth: [] as WorkerCacheHealth[],
        summary: {
          healthy: 0,
          warning: 0,
          critical: 0,
          unknown: 0,
          total: 0,
        } as CacheHealthSummary,
      };
    }

    const workerHealth: WorkerCacheHealth[] = [];
    const summary: CacheHealthSummary = {
      healthy: 0,
      warning: 0,
      critical: 0,
      unknown: 0,
      total: workers.length,
    };

    for (const worker of workers) {
      const health = getWorkerCacheHealth(worker);
      if (health) {
        workerHealth.push(health);
        summary[health.status]++;
      } else {
        summary.unknown++;
      }
    }

    return { workerHealth, summary };
  }, [workers]);
}

/**
 * Hook to get cache health for a single worker.
 *
 * @param worker - Single worker response
 * @returns Cache health object or undefined if metrics not available
 */
export function useSingleWorkerCacheHealth(worker: WorkerResponse | undefined) {
  return useMemo(() => {
    if (!worker) return undefined;
    return getWorkerCacheHealth(worker);
  }, [worker]);
}
