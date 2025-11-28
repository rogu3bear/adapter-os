/**
 * useSystemState Hook
 *
 * Provides access to the ground truth system state endpoint via polling.
 *
 * Features:
 * - Polling-based updates (no SSE stream exists for system state)
 * - Memory pressure and top adapters visibility
 * - Hierarchical state: Node -> Tenant -> Stack -> Adapter
 *
 * Note: isLive is always false since there's no dedicated SSE stream
 * for system state. The UI should show "Updated Xs ago" instead of "Live".
 */

import { useState, useMemo, useEffect } from 'react';
import { usePolling, type PollingSpeed } from './usePolling';
import apiClient from '@/api/client';
import type {
  SystemStateResponse,
  SystemStateQuery,
  MemoryPressureLevel,
} from '@/api/system-state-types';

export interface UseSystemStateOptions {
  /** Enable data fetching (default: true) */
  enabled?: boolean;
  /** Polling interval in ms (default: 10000) */
  pollingInterval?: number;
  /** Number of top adapters to fetch (default: 10) */
  topAdapters?: number;
  /** Filter to specific tenant */
  tenantId?: string;
  /** Callback when pressure level changes */
  onPressureChange?: (level: MemoryPressureLevel) => void;
}

export interface UseSystemStateReturn {
  /** System state data */
  data: SystemStateResponse | null;
  /** Loading state */
  isLoading: boolean;
  /** Error if any */
  error: Error | null;
  /** Whether using live SSE connection (always false - no SSE stream for system state) */
  isLive: boolean;
  /** Last update timestamp */
  lastUpdated: Date | null;
  /** Manual refresh function */
  refetch: () => Promise<void>;
}

/**
 * Hook for accessing ground truth system state
 *
 * @example
 * ```tsx
 * const { data, lastUpdated } = useSystemState({
 *   topAdapters: 5,
 * });
 *
 * return (
 *   <div>
 *     <span>Updated: {formatTimeSince(lastUpdated)}</span>
 *     <span>Memory: {data?.memory.pressure_level}</span>
 *   </div>
 * );
 * ```
 */
export function useSystemState(
  options: UseSystemStateOptions = {}
): UseSystemStateReturn {
  const {
    enabled = true,
    pollingInterval = 10000,
    topAdapters = 10,
    tenantId,
    onPressureChange,
  } = options;

  const [lastPressureLevel, setLastPressureLevel] = useState<MemoryPressureLevel | null>(null);

  // Build query params
  const queryParams: SystemStateQuery = useMemo(
    () => ({
      include_adapters: true,
      top_adapters: topAdapters,
      tenant_id: tenantId,
    }),
    [topAdapters, tenantId]
  );

  // Use polling for system state data (no SSE stream exists for system state)
  const pollingSpeed: PollingSpeed = pollingInterval <= 5000 ? 'fast' : pollingInterval <= 15000 ? 'normal' : 'slow';

  const { data, isLoading, error, lastUpdated, refetch } = usePolling<SystemStateResponse>(
    () => apiClient.getSystemState(queryParams),
    pollingSpeed,
    {
      enabled,
      operationName: 'getSystemState',
      enableCircuitBreaker: true,
    }
  );

  // Track pressure level changes
  useEffect(() => {
    if (data?.memory.pressure_level && onPressureChange) {
      const currentLevel = data.memory.pressure_level;
      if (currentLevel !== lastPressureLevel) {
        setLastPressureLevel(currentLevel);
        onPressureChange(currentLevel);
      }
    }
  }, [data?.memory.pressure_level, lastPressureLevel, onPressureChange]);

  return {
    data,
    isLoading,
    error,
    isLive: false, // No SSE stream for system state - always polling
    lastUpdated,
    refetch,
  };
}

/**
 * Hook for memory pressure specifically
 * Lightweight version focusing on memory state
 */
export function useMemoryPressure(
  options: Pick<UseSystemStateOptions, 'enabled' | 'onPressureChange'> = {}
) {
  const { data, isLoading, error, isLive, lastUpdated, refetch } = useSystemState({
    ...options,
    topAdapters: 5,
  });

  return useMemo(
    () => ({
      memory: data?.memory ?? null,
      pressureLevel: data?.memory?.pressure_level ?? null,
      headroomPercent: data?.memory?.headroom_percent ?? null,
      topAdapters: data?.memory?.top_adapters ?? [],
      isLoading,
      error,
      isLive,
      lastUpdated,
      refetch,
    }),
    [data?.memory, isLoading, error, isLive, lastUpdated, refetch]
  );
}

export default useSystemState;
