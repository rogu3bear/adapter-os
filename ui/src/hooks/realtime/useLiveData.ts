/**
 * useLiveData - Enhanced wrapper around useSSEWithPollingFallback
 *
 * Provides additional features on top of the unified SSE + polling hook:
 * - Data freshness calculation ('live', 'fresh', 'recent', 'stale', 'very_stale')
 * - Last updated timestamp tracking
 * - Dynamic SSE enable/disable toggle
 * - Merge strategies for SSE data updates
 *
 * @deprecated Consider migrating to useSSEWithPollingFallback directly if you don't
 * need freshness tracking or merge strategies.
 *
 * Features inherited from useSSEWithPollingFallback:
 * - SSE primary with automatic polling fallback
 * - Circuit breaker pattern (5 failures → pause 30s)
 * - Exponential backoff (1s to 30s for SSE, 2x up to 5x for polling)
 * - Auth token management (reconnects on token change)
 * - Consistent connection status reporting
 */

import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { useSSEWithPollingFallback } from './useSSEWithPollingFallback';
import type { ConnectionStatus, PollingSpeed } from './useSSEWithPollingFallback';

// ============================================================================
// Types
// ============================================================================

// Re-export types for backwards compatibility
export type { PollingSpeed, ConnectionStatus };

export type DataFreshnessLevel = 'live' | 'fresh' | 'recent' | 'stale' | 'very_stale';

export interface UseLiveDataOptions<T> {
  /** SSE endpoint path (e.g., '/v1/stream/metrics') */
  sseEndpoint?: string;

  /** SSE event type to listen for (e.g., 'metrics', 'adapters') */
  sseEventType?: string;

  /** Function to fetch data for polling/initial load */
  fetchFn: () => Promise<T>;

  /** Polling speed preset */
  pollingSpeed?: PollingSpeed;

  /** Enable/disable the hook */
  enabled?: boolean;

  /** Transform SSE data before merging with state */
  transformSSE?: (sseData: unknown) => Partial<T>;

  /** How to combine SSE data with existing data */
  mergeStrategy?: 'replace' | 'merge';

  /** Callback when SSE message received */
  onSSEMessage?: (data: unknown) => void;

  /** Callback on error */
  onError?: (error: Error, source: 'sse' | 'polling') => void;

  /** Callback when connection status changes */
  onConnectionChange?: (status: ConnectionStatus) => void;

  /** Circuit breaker: failures before opening (default: 5) */
  circuitBreakerThreshold?: number;

  /** Circuit breaker: reset delay in ms (default: 30000) */
  circuitBreakerResetMs?: number;

  /** Operation name for logging */
  operationName?: string;
}

export interface UseLiveDataReturn<T> {
  /** Current data */
  data: T | null;

  /** Loading state */
  isLoading: boolean;

  /** Last error */
  error: Error | null;

  /** SSE connection status */
  sseConnected: boolean;

  /** Overall connection status */
  connectionStatus: ConnectionStatus;

  /** Timestamp of last update */
  lastUpdated: Date | null;

  /** Data freshness level */
  freshnessLevel: DataFreshnessLevel;

  /** Manually refetch data */
  refetch: () => Promise<void>;

  /** Manually reconnect SSE */
  reconnect: () => void;

  /** Toggle SSE on/off */
  toggleSSE: (enabled: boolean) => void;
}

// ============================================================================
// Constants
// ============================================================================

// Freshness thresholds in milliseconds
const FRESHNESS_THRESHOLDS = {
  fresh: 10_000,      // < 10s = fresh
  recent: 60_000,     // < 1m = recent
  stale: 300_000,     // < 5m = stale
  // > 5m = very_stale
};

// ============================================================================
// Helper Functions
// ============================================================================

function calculateFreshness(lastUpdated: Date | null, sseConnected: boolean): DataFreshnessLevel {
  if (sseConnected) return 'live';
  if (!lastUpdated) return 'stale';

  const ageMs = Date.now() - lastUpdated.getTime();

  if (ageMs < FRESHNESS_THRESHOLDS.fresh) return 'fresh';
  if (ageMs < FRESHNESS_THRESHOLDS.recent) return 'recent';
  if (ageMs < FRESHNESS_THRESHOLDS.stale) return 'stale';
  return 'very_stale';
}

// ============================================================================
// Hook Implementation
// ============================================================================

export function useLiveData<T>(options: UseLiveDataOptions<T>): UseLiveDataReturn<T> {
  const {
    sseEndpoint,
    sseEventType,
    fetchFn,
    pollingSpeed = 'normal',
    enabled = true,
    transformSSE,
    mergeStrategy = 'replace',
    onSSEMessage,
    onError,
    onConnectionChange,
    circuitBreakerThreshold = 5,
    circuitBreakerResetMs = 30000,
    operationName = 'useLiveData',
  } = options;

  // State for additional features not in base hook
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [sseEnabled, setSseEnabled] = useState(!!sseEndpoint);
  const dataRef = useRef<T | null>(null);

  // Transform function that handles merge strategy and timestamp tracking
  const transformSSEWithMerge = useCallback((sseData: unknown): T => {
    let result: T;

    if (transformSSE) {
      const transformed = transformSSE(sseData);
      if (mergeStrategy === 'merge' && dataRef.current) {
        result = { ...dataRef.current, ...transformed } as T;
      } else {
        result = transformed as T;
      }
    } else {
      result = sseData as T;
    }

    // Update timestamp
    setLastUpdated(new Date());

    return result;
  }, [transformSSE, mergeStrategy]);

  // Polling function that updates timestamp
  const pollingFnWithTimestamp = useCallback(async (): Promise<T> => {
    const result = await fetchFn();
    setLastUpdated(new Date());
    return result;
  }, [fetchFn]);

  // Use base hook with our wrapped functions
  const {
    data,
    isLoading,
    error,
    isConnected: sseConnected,
    connectionStatus,
    refetch,
    reconnect,
  } = useSSEWithPollingFallback<T>({
    sseEndpoint,
    sseEventType,
    pollingFn: pollingFnWithTimestamp,
    pollingSpeed,
    enabled,
    useSSE: sseEnabled,
    transformSSE: transformSSEWithMerge,
    onSSEMessage,
    onError,
    onConnectionChange,
    circuitBreakerThreshold,
    circuitBreakerResetMs,
    operationName,
    sseInitialBackoffMs: 1000,
  });

  // Keep data ref in sync for merge strategy
  useEffect(() => {
    dataRef.current = data;
  }, [data]);

  // Calculate freshness level
  const freshnessLevel = useMemo(
    () => calculateFreshness(lastUpdated, sseConnected),
    [lastUpdated, sseConnected]
  );

  // Toggle SSE function
  const toggleSSE = useCallback((enabled: boolean) => {
    setSseEnabled(enabled);
  }, []);

  return {
    data,
    isLoading,
    error,
    sseConnected,
    connectionStatus,
    lastUpdated,
    freshnessLevel,
    refetch,
    reconnect,
    toggleSSE,
  };
}

export default useLiveData;
