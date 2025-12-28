/**
 * Dashboard Metrics Hook
 *
 * Combines SSE real-time streaming with polling fallback for dashboard metrics.
 * Provides CPU, memory, disk usage, network bandwidth, and inference metrics.
 */

import { useCallback, useMemo } from 'react';
import { useSSE } from '@/hooks/realtime/useSSE';
import { usePolling } from '@/hooks/realtime/usePolling';
import { apiClient } from '@/api/services';
import { logger } from '@/utils/logger';

/**
 * Options for configuring the dashboard metrics hook
 */
export interface UseDashboardMetricsOptions {
  /** Currently selected tenant/workspace */
  selectedTenant?: string;
  /** Current user ID for logging context */
  userId?: string;
  /** Whether to enable metrics fetching (default: true) */
  enabled?: boolean;
}

/**
 * Raw SSE metrics data structure from the backend
 */
interface SSEMetricsData {
  // Backend returns these field names
  cpu_usage?: number;
  memory_usage?: number;
  disk_usage?: number;
  // SSE/legacy field names (fallback)
  cpu_usage_percent?: number;
  memory_usage_percent?: number;
  disk_usage_percent?: number;
  memory_usage_pct?: number;
  network_rx_bytes?: number;
  adapter_count?: number;
  active_sessions?: number;
  tokens_per_second?: number;
  latency_p95_ms?: number;
}

/**
 * Return type for the dashboard metrics hook
 */
export interface UseDashboardMetricsReturn {
  /** CPU usage percentage (0-100) */
  cpuUsage: number;
  /** Memory usage percentage (0-100) */
  memoryUsage: number;
  /** Disk usage percentage (0-100) */
  diskUsage: number;
  /** Network bandwidth in MB/s as string */
  networkBandwidth: string;
  /** Number of registered adapters */
  adapterCount: number;
  /** Number of active inference sessions */
  activeSessions: number;
  /** Tokens processed per second */
  tokensPerSecond: number;
  /** 95th percentile latency in milliseconds */
  latencyP95: number;
  /** Whether real-time connection is established */
  connected: boolean;
  /** SSE connection error, if any */
  sseError: Error | null;
  /** Polling error, if any */
  pollingError: Error | null;
  /** Whether data is currently loading */
  isLoading: boolean;
  /** Raw system metrics from polling */
  systemMetrics: SSEMetricsData | null;
  /** Reconnect the SSE stream */
  reconnect: () => void;
  /** Manually refetch metrics */
  refetch: () => Promise<void>;
}

/**
 * Hook for fetching and managing dashboard metrics with real-time updates.
 *
 * Uses Server-Sent Events (SSE) for real-time streaming when available,
 * with automatic fallback to polling when SSE is disconnected.
 *
 * @example
 * ```tsx
 * const {
 *   cpuUsage,
 *   memoryUsage,
 *   connected,
 *   reconnect
 * } = useDashboardMetrics({
 *   selectedTenant: 'my-workspace',
 *   userId: 'user-123'
 * });
 * ```
 */
export function useDashboardMetrics(
  options: UseDashboardMetricsOptions = {}
): UseDashboardMetricsReturn {
  const { selectedTenant, userId, enabled = true } = options;

  // SSE connection for real-time metrics updates
  const {
    data: sseMetrics,
    error: sseError,
    connected: sseConnected,
    reconnect: sseReconnect,
  } = useSSE<SSEMetricsData>('/v1/stream/metrics', {
    enabled,
    onError: () => {
      logger.error('Real-time metrics connection error', {
        component: 'useDashboardMetrics',
        operation: 'sse_connection',
        tenantId: selectedTenant,
        userId,
      }, new Error('SSE connection error'));
    },
  });

  // System metrics polling (disabled when SSE is connected)
  const fetchSystemMetrics = useCallback(async () => {
    const response = await apiClient.getSystemMetrics();
    return response as SSEMetricsData;
  }, []);

  const {
    data: systemMetrics,
    isLoading: metricsLoading,
    error: pollingError,
    refetch: refetchMetrics,
  } = usePolling(fetchSystemMetrics, 'normal', {
    enabled: enabled && !sseConnected, // Disable polling when SSE is connected
    operationName: 'system-metrics',
    onError: (err) => {
      logger.error('Failed to fetch system metrics', {
        component: 'useDashboardMetrics',
        operation: 'fetchSystemMetrics',
        tenantId: selectedTenant,
        userId,
      }, err);
    },
  });

  // Merge SSE and polling data - SSE takes priority for real-time updates
  const effectiveMetrics = sseMetrics || systemMetrics;

  // Compute derived values with proper fallbacks
  const computedMetrics = useMemo(() => {
    // Backend returns cpu_usage, memory_usage, disk_usage - fallback to _percent for SSE/legacy
    const cpuUsage = effectiveMetrics?.cpu_usage ?? effectiveMetrics?.cpu_usage_percent ?? 0;
    const memoryUsage =
      effectiveMetrics?.memory_usage ??
      effectiveMetrics?.memory_usage_percent ??
      effectiveMetrics?.memory_usage_pct ??
      0;
    const diskUsage = effectiveMetrics?.disk_usage ?? effectiveMetrics?.disk_usage_percent ?? 0;
    const networkBandwidth = effectiveMetrics?.network_rx_bytes
      ? (effectiveMetrics.network_rx_bytes / 1024 / 1024).toFixed(1)
      : '0';
    const adapterCount = effectiveMetrics?.adapter_count ?? 0;
    const activeSessions = effectiveMetrics?.active_sessions ?? 0;
    const tokensPerSecond = effectiveMetrics?.tokens_per_second ?? 0;
    const latencyP95 = effectiveMetrics?.latency_p95_ms ?? 0;

    return {
      cpuUsage,
      memoryUsage,
      diskUsage,
      networkBandwidth,
      adapterCount,
      activeSessions,
      tokensPerSecond,
      latencyP95,
    };
  }, [effectiveMetrics]);

  return {
    ...computedMetrics,
    connected: sseConnected,
    sseError,
    pollingError,
    isLoading: metricsLoading && !sseMetrics,
    systemMetrics,
    reconnect: sseReconnect,
    refetch: refetchMetrics,
  };
}
