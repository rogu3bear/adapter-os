/**
 * useLiveData - Unified hook for live data with SSE + polling fallback
 *
 * Combines SSE streaming with polling fallback, circuit breaker pattern,
 * and automatic token refresh handling.
 *
 * Features:
 * - SSE primary with automatic polling fallback
 * - Circuit breaker pattern (5 failures → pause 30s)
 * - Exponential backoff (1s to 30s for SSE, 2x up to 5x for polling)
 * - Auth token management (reconnects on token change)
 * - Consistent connection status reporting
 */

import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import apiClient from '@/api/client';
import { logger, toError } from '@/utils/logger';

// ============================================================================
// Types
// ============================================================================

export type PollingSpeed = 'fast' | 'normal' | 'slow';
export type ConnectionStatus = 'sse' | 'polling' | 'disconnected';
export type DataFreshnessLevel = 'live' | 'fresh' | 'recent' | 'stale' | 'very_stale';

const POLLING_INTERVALS: Record<PollingSpeed, number> = {
  fast: 2000,    // Real-time (training progress, alerts)
  normal: 5000,  // Standard (metrics, dashboard)
  slow: 30000,   // Background (system health, admin)
};

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

const SSE_MAX_RECONNECT_ATTEMPTS = 10;
const SSE_INITIAL_BACKOFF_MS = 1000;
const SSE_MAX_BACKOFF_MS = 30000;
const MAX_BACKOFF_MULTIPLIER = 5;

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

  const pollingIntervalMs = POLLING_INTERVALS[pollingSpeed];

  // State
  const [data, setData] = useState<T | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [sseConnected, setSseConnected] = useState(false);
  const [sseEnabled, setSseEnabled] = useState(!!sseEndpoint);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);

  // Refs for SSE
  const eventSourceRef = useRef<EventSource | null>(null);
  const sseReconnectAttemptsRef = useRef(0);
  const sseReconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const sseConnectRef = useRef<(() => void) | null>(null);

  // Refs for polling
  const pollingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pollingFailureCountRef = useRef(0);
  const pollingBackoffMultiplierRef = useRef(1);
  const circuitOpenTimeRef = useRef<Date | null>(null);
  const circuitBreakerTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Refs to avoid stale closures
  const mountedRef = useRef(true);
  const fetchFnRef = useRef(fetchFn);
  const transformSSERef = useRef(transformSSE);
  const onSSEMessageRef = useRef(onSSEMessage);
  const onErrorRef = useRef(onError);
  const onConnectionChangeRef = useRef(onConnectionChange);
  const dataRef = useRef<T | null>(null);

  // Update refs
  useEffect(() => {
    fetchFnRef.current = fetchFn;
    transformSSERef.current = transformSSE;
    onSSEMessageRef.current = onSSEMessage;
    onErrorRef.current = onError;
    onConnectionChangeRef.current = onConnectionChange;
  }, [fetchFn, transformSSE, onSSEMessage, onError, onConnectionChange]);

  // Keep data ref in sync
  useEffect(() => {
    dataRef.current = data;
  }, [data]);

  // Connection status derived
  const connectionStatus: ConnectionStatus = useMemo(() => {
    if (sseConnected) return 'sse';
    if (pollingTimeoutRef.current !== null || data !== null) return 'polling';
    return 'disconnected';
  }, [sseConnected, data]);

  // Freshness level derived
  const freshnessLevel = useMemo(
    () => calculateFreshness(lastUpdated, sseConnected),
    [lastUpdated, sseConnected]
  );

  // Notify connection status changes
  useEffect(() => {
    onConnectionChangeRef.current?.(connectionStatus);
  }, [connectionStatus]);

  // ============================================================================
  // Polling Logic
  // ============================================================================

  const scheduleNextPollRef = useRef<(() => void) | null>(null);

  const fetchData = useCallback(async () => {
    if (!mountedRef.current) return;

    // Check circuit breaker
    if (circuitOpenTimeRef.current) {
      const timeSinceOpen = Date.now() - circuitOpenTimeRef.current.getTime();
      if (timeSinceOpen < circuitBreakerResetMs) {
        return; // Circuit still open
      }
      // Reset circuit breaker
      circuitOpenTimeRef.current = null;
      pollingFailureCountRef.current = 0;
      pollingBackoffMultiplierRef.current = 1;
      logger.info('Circuit breaker reset - resuming polling', {
        component: 'useLiveData',
        operation: operationName,
      });
    }

    try {
      const result = await fetchFnRef.current();
      if (!mountedRef.current) return;

      setData(result);
      setLastUpdated(new Date());
      setError(null);
      setIsLoading(false);

      // Reset failure tracking
      pollingFailureCountRef.current = 0;
      pollingBackoffMultiplierRef.current = 1;

      // Schedule next poll if SSE not connected
      if (!sseConnected) {
        scheduleNextPollRef.current?.();
      }
    } catch (err) {
      if (!mountedRef.current) return;

      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);
      setIsLoading(false);

      // Increment failure count and apply backoff
      pollingFailureCountRef.current += 1;
      pollingBackoffMultiplierRef.current = Math.min(
        pollingBackoffMultiplierRef.current * 2,
        MAX_BACKOFF_MULTIPLIER
      );

      logger.error('Polling operation failed', {
        component: 'useLiveData',
        operation: operationName,
        failureCount: pollingFailureCountRef.current,
        backoffMultiplier: pollingBackoffMultiplierRef.current,
      }, toError(err));

      onErrorRef.current?.(error, 'polling');

      // Check circuit breaker threshold
      if (pollingFailureCountRef.current >= circuitBreakerThreshold) {
        circuitOpenTimeRef.current = new Date();
        logger.warn('Circuit breaker opened - pausing polling', {
          component: 'useLiveData',
          operation: operationName,
          failureCount: pollingFailureCountRef.current,
          resetAfterMs: circuitBreakerResetMs,
        });

        // Clear polling timeout
        if (pollingTimeoutRef.current) {
          clearTimeout(pollingTimeoutRef.current);
          pollingTimeoutRef.current = null;
        }

        // Schedule circuit breaker reset
        circuitBreakerTimeoutRef.current = setTimeout(() => {
          if (mountedRef.current && !sseConnected) {
            circuitOpenTimeRef.current = null;
            pollingFailureCountRef.current = 0;
            pollingBackoffMultiplierRef.current = 1;
            scheduleNextPollRef.current?.();
          }
        }, circuitBreakerResetMs);
      } else {
        scheduleNextPollRef.current?.();
      }
    }
  }, [operationName, circuitBreakerThreshold, circuitBreakerResetMs, sseConnected]);

  // Schedule next poll
  useEffect(() => {
    scheduleNextPollRef.current = () => {
      if (!mountedRef.current || sseConnected) return;

      if (pollingTimeoutRef.current) {
        clearTimeout(pollingTimeoutRef.current);
      }

      const interval = pollingIntervalMs * pollingBackoffMultiplierRef.current;
      pollingTimeoutRef.current = setTimeout(() => {
        if (mountedRef.current && !sseConnected) {
          fetchData();
        }
      }, interval);
    };
  }, [pollingIntervalMs, sseConnected, fetchData]);

  // ============================================================================
  // SSE Logic
  // ============================================================================

  const connectSSE = useCallback(() => {
    if (!sseEndpoint || !sseEnabled || !enabled) return;

    const baseUrl = (import.meta as { env?: { VITE_API_URL?: string } }).env?.VITE_API_URL || '/api';
    const token = apiClient.getToken();
    const url = token
      ? `${baseUrl}${sseEndpoint}?token=${encodeURIComponent(token)}`
      : `${baseUrl}${sseEndpoint}`;

    try {
      // Close existing connection
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
        eventSourceRef.current = null;
      }

      const eventSource = new EventSource(url);
      eventSourceRef.current = eventSource;

      eventSource.onopen = () => {
        if (!mountedRef.current) return;
        setSseConnected(true);
        setError(null);
        sseReconnectAttemptsRef.current = 0;

        // Stop polling when SSE connects
        if (pollingTimeoutRef.current) {
          clearTimeout(pollingTimeoutRef.current);
          pollingTimeoutRef.current = null;
        }

        logger.info('SSE connected', {
          component: 'useLiveData',
          operation: operationName,
          endpoint: sseEndpoint,
        });
      };

      // Handle default message event
      eventSource.onmessage = (event) => {
        if (!mountedRef.current) return;
        try {
          const parsed = JSON.parse(event.data);
          handleSSEData(parsed);
        } catch (e) {
          logger.error('Failed to parse SSE message', {
            component: 'useLiveData',
            endpoint: sseEndpoint,
          }, toError(e));
        }
      };

      // Handle custom event type if specified
      if (sseEventType) {
        eventSource.addEventListener(sseEventType, (event) => {
          if (!mountedRef.current) return;
          try {
            const parsed = JSON.parse((event as MessageEvent).data);
            handleSSEData(parsed);
          } catch (e) {
            logger.error('Failed to parse SSE custom event', {
              component: 'useLiveData',
              endpoint: sseEndpoint,
              eventType: sseEventType,
            }, toError(e));
          }
        });
      }

      // Handle common event types
      const commonEventTypes = ['metrics', 'adapters', 'bundles', 'telemetry'];
      commonEventTypes.forEach((eventType) => {
        if (eventType !== sseEventType) {
          eventSource.addEventListener(eventType, (event) => {
            if (!mountedRef.current) return;
            try {
              const parsed = JSON.parse((event as MessageEvent).data);
              // Only process if this matches our expected type or we accept all
              if (!sseEventType || eventType === sseEventType) {
                handleSSEData(parsed);
              }
            } catch (e) {
              // Silently ignore parse errors for non-matching event types
            }
          });
        }
      });

      // Handle keepalive
      eventSource.addEventListener('keepalive', () => {
        // Just acknowledge keepalive
      });

      eventSource.onerror = () => {
        if (!mountedRef.current) return;
        setSseConnected(false);
        eventSource.close();
        eventSourceRef.current = null;

        // Start polling as fallback
        if (!pollingTimeoutRef.current) {
          scheduleNextPollRef.current?.();
        }

        // Attempt reconnection with exponential backoff
        if (sseReconnectAttemptsRef.current < SSE_MAX_RECONNECT_ATTEMPTS) {
          const backoffMs = Math.min(
            SSE_INITIAL_BACKOFF_MS * Math.pow(2, sseReconnectAttemptsRef.current),
            SSE_MAX_BACKOFF_MS
          );
          sseReconnectAttemptsRef.current += 1;

          logger.warn('SSE connection error, reconnecting', {
            component: 'useLiveData',
            endpoint: sseEndpoint,
            attempt: sseReconnectAttemptsRef.current,
            backoffMs,
          });

          if (sseReconnectTimeoutRef.current) {
            clearTimeout(sseReconnectTimeoutRef.current);
          }
          sseReconnectTimeoutRef.current = setTimeout(() => {
            if (mountedRef.current && sseEnabled) {
              connectSSE();
            }
          }, backoffMs);
        } else {
          logger.error('SSE max reconnection attempts exceeded', {
            component: 'useLiveData',
            endpoint: sseEndpoint,
          }, new Error('Max reconnection attempts exceeded'));
          onErrorRef.current?.(new Error('SSE connection failed'), 'sse');
        }
      };
    } catch (e) {
      logger.error('Failed to initialize SSE connection', {
        component: 'useLiveData',
        endpoint: sseEndpoint,
      }, toError(e));
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sseEndpoint, sseEventType, sseEnabled, enabled, operationName]);

  // Store connect function for reconnect
  useEffect(() => {
    sseConnectRef.current = connectSSE;
  }, [connectSSE]);

  // Handle SSE data
  const handleSSEData = useCallback((parsed: unknown) => {
    onSSEMessageRef.current?.(parsed);

    if (transformSSERef.current) {
      const transformed = transformSSERef.current(parsed);
      if (mergeStrategy === 'merge' && dataRef.current) {
        setData({ ...dataRef.current, ...transformed } as T);
      } else {
        setData(transformed as T);
      }
    } else {
      setData(parsed as T);
    }

    setLastUpdated(new Date());
    setIsLoading(false);
    setError(null);
  }, [mergeStrategy]);

  // ============================================================================
  // Lifecycle
  // ============================================================================

  // Initial fetch and SSE connection
  useEffect(() => {
    mountedRef.current = true;

    if (!enabled) {
      setIsLoading(false);
      return;
    }

    // Initial fetch
    fetchData();

    // Connect SSE if endpoint provided
    if (sseEndpoint && sseEnabled) {
      connectSSE();
    }

    return () => {
      mountedRef.current = false;

      // Cleanup SSE
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
        eventSourceRef.current = null;
      }
      if (sseReconnectTimeoutRef.current) {
        clearTimeout(sseReconnectTimeoutRef.current);
        sseReconnectTimeoutRef.current = null;
      }

      // Cleanup polling
      if (pollingTimeoutRef.current) {
        clearTimeout(pollingTimeoutRef.current);
        pollingTimeoutRef.current = null;
      }
      if (circuitBreakerTimeoutRef.current) {
        clearTimeout(circuitBreakerTimeoutRef.current);
        circuitBreakerTimeoutRef.current = null;
      }
    };
  }, [enabled, sseEndpoint, sseEnabled, fetchData, connectSSE]);

  // ============================================================================
  // Public API
  // ============================================================================

  const refetch = useCallback(async () => {
    await fetchData();
  }, [fetchData]);

  const reconnect = useCallback(() => {
    // Reset SSE state
    if (sseReconnectTimeoutRef.current) {
      clearTimeout(sseReconnectTimeoutRef.current);
      sseReconnectTimeoutRef.current = null;
    }
    sseReconnectAttemptsRef.current = 0;

    // Reset polling state
    pollingFailureCountRef.current = 0;
    pollingBackoffMultiplierRef.current = 1;
    circuitOpenTimeRef.current = null;

    setError(null);

    if (sseConnectRef.current && sseEnabled) {
      sseConnectRef.current();
    } else {
      fetchData();
    }
  }, [sseEnabled, fetchData]);

  const toggleSSE = useCallback((enabled: boolean) => {
    setSseEnabled(enabled);
    if (!enabled && eventSourceRef.current) {
      eventSourceRef.current.close();
      eventSourceRef.current = null;
      setSseConnected(false);
      // Start polling
      scheduleNextPollRef.current?.();
    } else if (enabled && sseEndpoint) {
      connectSSE();
    }
  }, [sseEndpoint, connectSSE]);

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
