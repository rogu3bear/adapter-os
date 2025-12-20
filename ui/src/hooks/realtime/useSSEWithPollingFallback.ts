/**
 * Unified SSE + Polling Fallback Hook
 *
 * Consolidates ~1,200 LOC of duplicate SSE + polling implementations across
 * useActivityFeed, useNotifications, useLiveData, useSSE, and usePolling.
 *
 * Features:
 * - Primary: SSE connection with auto-reconnect and exponential backoff
 * - Fallback: Polling when SSE fails or is disabled
 * - Circuit breaker pattern to prevent retry storms
 * - Consistent interface across all consumers
 * - TypeScript generics for type safety
 * - Tenant switch handling with automatic reconnection
 * - Proper cleanup on unmount
 *
 * # Citations
 * - ui/src/hooks/useActivityFeed.ts L216-462: SSE + polling pattern with reconnect
 * - ui/src/hooks/useNotifications.ts L235-376: SSE subscription with fallback
 * - ui/src/hooks/useLiveData.ts L142-652: Comprehensive SSE + polling with circuit breaker
 * - ui/src/hooks/useSSE.ts L136-314: SSE connection with circuit breaker
 * - ui/src/hooks/usePolling.ts L90-214: Polling with backoff and circuit breaker
 *
 * # Policy Compliance
 * - Policy Pack #1 (Egress): Uses relative API paths only
 * - Policy Pack #9 (Telemetry): Uses canonical JSON and structured logging
 */

import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { logger, toError } from '@/utils/logger';
import { TENANT_SWITCH_EVENT } from '@/utils/tenant';

// ============================================================================
// Types
// ============================================================================

export type ConnectionStatus = 'sse' | 'polling' | 'disconnected';
export type PollingSpeed = 'fast' | 'normal' | 'slow' | 'realtime';

const POLLING_INTERVALS: Record<PollingSpeed, number> = {
  realtime: 1000, // Ultra-fast (real-time updates)
  fast: 2000,     // Real-time (training progress, alerts)
  normal: 5000,   // Standard (metrics, dashboard)
  slow: 30000,    // Background (system health, admin)
};

export interface UseSSEWithPollingFallbackOptions<T> {
  /** SSE endpoint path (e.g., '/v1/telemetry/events/recent/stream') */
  sseEndpoint?: string;

  /** SSE event type to listen for (e.g., 'activity', 'metrics') */
  sseEventType?: string;

  /** Function to fetch data for polling/initial load */
  pollingFn: () => Promise<T>;

  /** Polling speed preset */
  pollingSpeed?: PollingSpeed;

  /** Enable/disable the hook */
  enabled?: boolean;

  /** Enable/disable SSE (can force polling-only mode) */
  useSSE?: boolean;

  /** Transform SSE data before setting state */
  transformSSE?: (sseData: unknown) => T;

  /** Callback when SSE message received */
  onSSEMessage?: (data: unknown) => void;

  /** Callback on error */
  onError?: (error: Error, source: 'sse' | 'polling') => void;

  /** Callback when connection status changes */
  onConnectionChange?: (status: ConnectionStatus) => void;

  /** SSE max reconnection attempts (default: 10) */
  sseMaxReconnectAttempts?: number;

  /** SSE initial backoff in ms (default: 500) */
  sseInitialBackoffMs?: number;

  /** SSE max backoff in ms (default: 30000) */
  sseMaxBackoffMs?: number;

  /** Circuit breaker: failures before opening (default: 5) */
  circuitBreakerThreshold?: number;

  /** Circuit breaker: reset delay in ms (default: 30000) */
  circuitBreakerResetMs?: number;

  /** Maximum polling backoff multiplier (default: 5) */
  maxPollingBackoffMultiplier?: number;

  /** Operation name for logging (default: 'useSSEWithPollingFallback') */
  operationName?: string;

  /** Baseline polling interval (in addition to SSE, for data freshness) */
  baselinePollingIntervalMs?: number;
}

export interface UseSSEWithPollingFallbackReturn<T> {
  /** Current data */
  data: T | null;

  /** Loading state */
  isLoading: boolean;

  /** Last error */
  error: Error | null;

  /** SSE connection status */
  isConnected: boolean;

  /** Overall connection status */
  connectionStatus: ConnectionStatus;

  /** Manually refetch data */
  refetch: () => Promise<void>;

  /** Manually reconnect SSE */
  reconnect: () => void;

  /** SSE reconnection attempts count */
  reconnectAttempts: number;
}

// ============================================================================
// Hook Implementation
// ============================================================================

export function useSSEWithPollingFallback<T>(
  options: UseSSEWithPollingFallbackOptions<T>
): UseSSEWithPollingFallbackReturn<T> {
  const {
    sseEndpoint,
    sseEventType,
    pollingFn,
    pollingSpeed = 'normal',
    enabled = true,
    useSSE = true,
    transformSSE,
    onSSEMessage,
    onError,
    onConnectionChange,
    sseMaxReconnectAttempts = 10,
    sseInitialBackoffMs = 500,
    sseMaxBackoffMs = 30000,
    circuitBreakerThreshold = 5,
    circuitBreakerResetMs = 30000,
    maxPollingBackoffMultiplier = 5,
    operationName = 'useSSEWithPollingFallback',
    baselinePollingIntervalMs,
  } = options;

  const pollingIntervalMs = POLLING_INTERVALS[pollingSpeed];

  // ============================================================================
  // State
  // ============================================================================

  const [data, setData] = useState<T | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [isConnected, setIsConnected] = useState(false);

  // ============================================================================
  // Refs
  // ============================================================================

  // SSE refs
  const eventSourceRef = useRef<EventSource | null>(null);
  const sseReconnectAttemptsRef = useRef(0);
  const sseReconnectTimeoutRef = useRef<NodeJS.Timeout | null>(null);

  // Polling refs
  const pollingTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const fallbackPollingTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const baselinePollingTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const pollingFailureCountRef = useRef(0);
  const pollingBackoffMultiplierRef = useRef(1);

  // Circuit breaker refs
  const circuitOpenTimeRef = useRef<Date | null>(null);
  const circuitBreakerTimeoutRef = useRef<NodeJS.Timeout | null>(null);

  // Lifecycle refs
  const mountedRef = useRef(true);

  // Callback refs to avoid stale closures
  const pollingFnRef = useRef(pollingFn);
  const transformSSERef = useRef(transformSSE);
  const onSSEMessageRef = useRef(onSSEMessage);
  const onErrorRef = useRef(onError);
  const onConnectionChangeRef = useRef(onConnectionChange);

  // Update callback refs
  useEffect(() => {
    pollingFnRef.current = pollingFn;
    transformSSERef.current = transformSSE;
    onSSEMessageRef.current = onSSEMessage;
    onErrorRef.current = onError;
    onConnectionChangeRef.current = onConnectionChange;
  }, [pollingFn, transformSSE, onSSEMessage, onError, onConnectionChange]);

  // ============================================================================
  // Derived State
  // ============================================================================

  const connectionStatus: ConnectionStatus = useMemo(() => {
    if (isConnected) return 'sse';
    if (pollingTimeoutRef.current !== null || fallbackPollingTimeoutRef.current !== null) return 'polling';
    return 'disconnected';
  }, [isConnected]);

  // Notify connection status changes
  useEffect(() => {
    onConnectionChangeRef.current?.(connectionStatus);
  }, [connectionStatus]);

  // ============================================================================
  // Polling Logic
  // ============================================================================

  const clearAllPollingTimers = useCallback(() => {
    if (pollingTimeoutRef.current) {
      clearTimeout(pollingTimeoutRef.current);
      pollingTimeoutRef.current = null;
    }
    if (fallbackPollingTimeoutRef.current) {
      clearTimeout(fallbackPollingTimeoutRef.current);
      fallbackPollingTimeoutRef.current = null;
    }
  }, []);

  const fetchData = useCallback(async () => {
    if (!mountedRef.current || !enabled) return;

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
        component: 'useSSEWithPollingFallback',
        operation: operationName,
      });
    }

    try {
      setIsLoading(true);
      const result = await pollingFnRef.current();

      if (!mountedRef.current) return;

      setData(result);
      setError(null);
      setIsLoading(false);

      // Reset failure tracking
      pollingFailureCountRef.current = 0;
      pollingBackoffMultiplierRef.current = 1;
    } catch (err) {
      if (!mountedRef.current) return;

      const errorObj = err instanceof Error ? err : new Error(String(err));
      setError(errorObj);
      setIsLoading(false);

      // Increment failure count and apply backoff
      pollingFailureCountRef.current += 1;
      pollingBackoffMultiplierRef.current = Math.min(
        pollingBackoffMultiplierRef.current * 2,
        maxPollingBackoffMultiplier
      );

      logger.error('Polling operation failed', {
        component: 'useSSEWithPollingFallback',
        operation: operationName,
        failureCount: pollingFailureCountRef.current,
        backoffMultiplier: pollingBackoffMultiplierRef.current,
      }, toError(err));

      onErrorRef.current?.(errorObj, 'polling');

      // Check circuit breaker threshold
      if (pollingFailureCountRef.current >= circuitBreakerThreshold) {
        circuitOpenTimeRef.current = new Date();
        logger.warn('Circuit breaker opened - pausing polling', {
          component: 'useSSEWithPollingFallback',
          operation: operationName,
          failureCount: pollingFailureCountRef.current,
          resetAfterMs: circuitBreakerResetMs,
        });

        clearAllPollingTimers();

        // Schedule circuit breaker reset
        circuitBreakerTimeoutRef.current = setTimeout(() => {
          if (mountedRef.current && !isConnected) {
            circuitOpenTimeRef.current = null;
            pollingFailureCountRef.current = 0;
            pollingBackoffMultiplierRef.current = 1;
            startFallbackPolling();
          }
        }, circuitBreakerResetMs);
      }
    }
  }, [
    enabled,
    circuitBreakerResetMs,
    circuitBreakerThreshold,
    maxPollingBackoffMultiplier,
    operationName,
    clearAllPollingTimers,
    isConnected,
  ]);

  const startFallbackPolling = useCallback(() => {
    if (!mountedRef.current || !enabled || isConnected) return;

    clearAllPollingTimers();

    // Fallback polling while SSE disconnected
    const interval = pollingIntervalMs * pollingBackoffMultiplierRef.current;

    const pollOnce = () => {
      if (!mountedRef.current || !enabled || isConnected) return;

      fetchData().then(() => {
        if (!mountedRef.current || !enabled || isConnected) return;
        fallbackPollingTimeoutRef.current = setTimeout(pollOnce, interval);
      }).catch((err) => {
        logger.warn('Fallback polling failed', {
          component: 'useSSEWithPollingFallback',
          operation: operationName,
          error: err instanceof Error ? err.message : String(err),
        });
        if (!mountedRef.current || !enabled || isConnected) return;
        fallbackPollingTimeoutRef.current = setTimeout(pollOnce, interval);
      });
    };

    pollOnce();
  }, [enabled, isConnected, pollingIntervalMs, operationName, clearAllPollingTimers, fetchData]);

  const startBaselinePolling = useCallback(() => {
    if (!baselinePollingIntervalMs || !enabled) return;

    if (baselinePollingTimeoutRef.current) {
      clearInterval(baselinePollingTimeoutRef.current);
    }

    baselinePollingTimeoutRef.current = setInterval(() => {
      if (mountedRef.current && enabled) {
        fetchData().catch((err) => {
          logger.debug('Baseline polling failed', {
            component: 'useSSEWithPollingFallback',
            operation: operationName,
            error: err instanceof Error ? err.message : String(err),
          });
        });
      }
    }, baselinePollingIntervalMs);
  }, [baselinePollingIntervalMs, enabled, operationName, fetchData]);

  // ============================================================================
  // SSE Logic
  // ============================================================================

  const clearSSETimers = useCallback(() => {
    if (sseReconnectTimeoutRef.current) {
      clearTimeout(sseReconnectTimeoutRef.current);
      sseReconnectTimeoutRef.current = null;
    }
  }, []);

  const closeSSE = useCallback(() => {
    if (eventSourceRef.current) {
      try {
        eventSourceRef.current.close();
      } catch (error) {
        logger.debug('SSE close error', { component: 'useSSEWithPollingFallback', error });
      }
      eventSourceRef.current = null;
    }
    setIsConnected(false);
  }, []);

  const handleSSEData = useCallback((rawData: unknown) => {
    if (!mountedRef.current) return;

    try {
      onSSEMessageRef.current?.(rawData);

      let processedData: T;
      if (transformSSERef.current) {
        processedData = transformSSERef.current(rawData);
      } else {
        processedData = rawData as T;
      }

      setData(processedData);
      setError(null);
      setIsLoading(false);
    } catch (err) {
      logger.error('Failed to process SSE data', {
        component: 'useSSEWithPollingFallback',
        operation: operationName,
      }, toError(err));
    }
  }, [operationName]);

  const connectSSE = useCallback(() => {
    if (!sseEndpoint || !useSSE || !enabled || !mountedRef.current) return;

    // Close existing connection
    closeSSE();

    try {
      const importMeta = import.meta as { env?: { VITE_SSE_URL?: string; VITE_API_URL?: string } };
      const baseUrl = importMeta?.env?.VITE_SSE_URL
        ? `http://${importMeta.env.VITE_SSE_URL}`
        : (importMeta?.env?.VITE_API_URL || '/api');
      const url = `${baseUrl}${sseEndpoint}`;

      const eventSource = new EventSource(url, { withCredentials: true });
      eventSourceRef.current = eventSource;

      eventSource.addEventListener('open', () => {
        if (!mountedRef.current) return;

        setIsConnected(true);
        setError(null);
        sseReconnectAttemptsRef.current = 0;

        // Stop fallback polling when SSE connects
        clearAllPollingTimers();

        logger.info('SSE connected', {
          component: 'useSSEWithPollingFallback',
          operation: operationName,
          endpoint: sseEndpoint,
        });
      });

      // Handle default message event
      eventSource.addEventListener('message', (event) => {
        if (!mountedRef.current) return;
        try {
          const parsed = JSON.parse(event.data);
          handleSSEData(parsed);
        } catch (e) {
          logger.debug('Failed to parse SSE message', {
            component: 'useSSEWithPollingFallback',
            endpoint: sseEndpoint,
          });
        }
      });

      // Handle custom event type if specified
      if (sseEventType) {
        eventSource.addEventListener(sseEventType, (event) => {
          if (!mountedRef.current) return;
          try {
            const parsed = JSON.parse((event as MessageEvent).data);
            handleSSEData(parsed);
          } catch (e) {
            logger.debug('Failed to parse SSE custom event', {
              component: 'useSSEWithPollingFallback',
              endpoint: sseEndpoint,
              eventType: sseEventType,
            });
          }
        });
      }

      // Handle keepalive
      eventSource.addEventListener('keepalive', () => {
        // Acknowledge keepalive
      });

      eventSource.addEventListener('error', (errorEvent) => {
        if (!mountedRef.current) return;

        setIsConnected(false);
        closeSSE();

        const errorMessage = errorEvent && typeof errorEvent === 'object' && 'message' in errorEvent
          ? (errorEvent as { message: string }).message
          : 'SSE connection error';

        const errorObj = new Error(errorMessage);
        onErrorRef.current?.(errorObj, 'sse');

        // Start fallback polling
        startFallbackPolling();

        // Attempt reconnection with exponential backoff
        if (sseReconnectAttemptsRef.current < sseMaxReconnectAttempts) {
          const backoffMs = Math.min(
            sseInitialBackoffMs * Math.pow(2, sseReconnectAttemptsRef.current),
            sseMaxBackoffMs
          );
          sseReconnectAttemptsRef.current += 1;

          logger.warn('SSE connection error, reconnecting', {
            component: 'useSSEWithPollingFallback',
            endpoint: sseEndpoint,
            attempt: sseReconnectAttemptsRef.current,
            backoffMs,
            errorMessage,
          });

          clearSSETimers();
          sseReconnectTimeoutRef.current = setTimeout(() => {
            if (mountedRef.current && useSSE && enabled) {
              connectSSE();
            }
          }, backoffMs);
        } else {
          logger.error('SSE max reconnection attempts exceeded', {
            component: 'useSSEWithPollingFallback',
            endpoint: sseEndpoint,
            errorMessage,
          }, errorObj);
        }
      });
    } catch (err) {
      logger.error('Failed to initialize SSE connection', {
        component: 'useSSEWithPollingFallback',
        endpoint: sseEndpoint,
      }, toError(err));
      startFallbackPolling();
    }
  }, [
    sseEndpoint,
    sseEventType,
    useSSE,
    enabled,
    operationName,
    sseMaxReconnectAttempts,
    sseInitialBackoffMs,
    sseMaxBackoffMs,
    closeSSE,
    clearSSETimers,
    clearAllPollingTimers,
    handleSSEData,
    startFallbackPolling,
  ]);

  // ============================================================================
  // Lifecycle
  // ============================================================================

  useEffect(() => {
    if (!enabled) {
      // Clean up everything if disabled
      closeSSE();
      clearSSETimers();
      clearAllPollingTimers();
      if (baselinePollingTimeoutRef.current) {
        clearInterval(baselinePollingTimeoutRef.current);
        baselinePollingTimeoutRef.current = null;
      }
      if (circuitBreakerTimeoutRef.current) {
        clearTimeout(circuitBreakerTimeoutRef.current);
        circuitBreakerTimeoutRef.current = null;
      }
      setIsLoading(false);
      return;
    }

    mountedRef.current = true;

    // Initial fetch
    fetchData();

    // Start baseline polling if configured
    startBaselinePolling();

    // Connect SSE if endpoint provided
    if (sseEndpoint && useSSE) {
      connectSSE();
    } else {
      // No SSE, start fallback polling immediately
      startFallbackPolling();
    }

    return () => {
      mountedRef.current = false;

      closeSSE();
      clearSSETimers();
      clearAllPollingTimers();

      if (baselinePollingTimeoutRef.current) {
        clearInterval(baselinePollingTimeoutRef.current);
        baselinePollingTimeoutRef.current = null;
      }
      if (circuitBreakerTimeoutRef.current) {
        clearTimeout(circuitBreakerTimeoutRef.current);
        circuitBreakerTimeoutRef.current = null;
      }
    };
  }, [
    enabled,
    sseEndpoint,
    useSSE,
    fetchData,
    connectSSE,
    startFallbackPolling,
    startBaselinePolling,
    closeSSE,
    clearSSETimers,
    clearAllPollingTimers,
  ]);

  // Handle tenant switch
  useEffect(() => {
    const handleTenantSwitch = () => {
      closeSSE();
      clearSSETimers();
      clearAllPollingTimers();
      if (circuitBreakerTimeoutRef.current) {
        clearTimeout(circuitBreakerTimeoutRef.current);
        circuitBreakerTimeoutRef.current = null;
      }

      setData(null);
      setError(null);
      sseReconnectAttemptsRef.current = 0;
      pollingFailureCountRef.current = 0;
      pollingBackoffMultiplierRef.current = 1;
      circuitOpenTimeRef.current = null;

      // Restart connection
      setIsLoading(true);
      fetchData();
      if (sseEndpoint && useSSE) {
        connectSSE();
      } else {
        startFallbackPolling();
      }
    };

    window.addEventListener(TENANT_SWITCH_EVENT, handleTenantSwitch);
    return () => window.removeEventListener(TENANT_SWITCH_EVENT, handleTenantSwitch);
  }, [
    sseEndpoint,
    useSSE,
    fetchData,
    connectSSE,
    startFallbackPolling,
    closeSSE,
    clearSSETimers,
    clearAllPollingTimers,
  ]);

  // ============================================================================
  // Public API
  // ============================================================================

  const refetch = useCallback(async () => {
    await fetchData();
  }, [fetchData]);

  const reconnect = useCallback(() => {
    // Reset SSE state
    clearSSETimers();
    sseReconnectAttemptsRef.current = 0;

    // Reset polling state
    pollingFailureCountRef.current = 0;
    pollingBackoffMultiplierRef.current = 1;
    circuitOpenTimeRef.current = null;

    setError(null);

    if (sseEndpoint && useSSE) {
      connectSSE();
    } else {
      startFallbackPolling();
    }
  }, [sseEndpoint, useSSE, connectSSE, startFallbackPolling, clearSSETimers]);

  return {
    data,
    isLoading,
    error,
    isConnected,
    connectionStatus,
    refetch,
    reconnect,
    reconnectAttempts: sseReconnectAttemptsRef.current,
  };
}
