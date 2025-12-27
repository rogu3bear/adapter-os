import { useEffect, useState, useRef, useCallback } from 'react';
import { logger, toError } from '@/utils/logger';
import { TENANT_SWITCH_EVENT } from '@/utils/tenant';

//! Strongly typed SSE hook options
//!
//! # Citations
//! - TypeScript best practices: Avoid `any` types for type safety
//! - CONTRIBUTING.md L118-122: "Follow Rust naming conventions"

// SSE keepalive timeout constants
const KEEPALIVE_TIMEOUT_MS = 60000; // 60 seconds
const KEEPALIVE_CHECK_INTERVAL_MS = 30000; // Check every 30 seconds

/**
 * Circuit breaker state for SSE connections
 * Prevents rapid reconnection attempts when service is down
 */
interface CircuitBreakerState {
  /** Number of consecutive errors */
  errorCount: number;
  /** Whether circuit is open (blocking connections) */
  isOpen: boolean;
  /** Timestamp when circuit was opened */
  openedAt: number | null;
  /** Last error message */
  lastError: string | null;
}

export interface UseSSEOptions<T = unknown> {
  enabled?: boolean;
  onError?: (error: Event) => void;
  onMessage?: (data: T) => void;
  /** Circuit breaker threshold (consecutive errors before opening) */
  circuitBreakerThreshold?: number;
  /** Circuit breaker recovery timeout in ms */
  circuitBreakerRecoveryMs?: number;
}

/**
 * Custom hook for SSE (Server-Sent Events) subscriptions
 * @param endpoint - The API endpoint path (e.g., '/v1/stream/metrics')
 * @param options - Configuration options
 * @returns The latest data received from the SSE stream with connection status
 */
export function useSSE<T = unknown>(
  endpoint: string,
  options: UseSSEOptions<T> = {}
): {
  data: T | null;
  error: Error | null;
  connected: boolean;
  reconnect: () => void;
  /** Whether circuit breaker is open (service unavailable) */
  circuitOpen: boolean;
  /** Number of reconnection attempts */
  reconnectAttempts: number;
} {
  const {
    enabled = true,
    onError,
    onMessage,
    circuitBreakerThreshold = 5,
    circuitBreakerRecoveryMs = 30000,
  } = options;
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [connected, setConnected] = useState(false);
  const createCircuitBreakerState = (): CircuitBreakerState => ({
    errorCount: 0,
    isOpen: false,
    openedAt: null,
    lastError: null,
  });
  const [circuitBreaker, setCircuitBreaker] = useState<CircuitBreakerState>(createCircuitBreakerState);
  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const circuitRecoveryTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const connectRef = useRef<(() => void) | null>(null);
  const lastActivityRef = useRef<number>(Date.now());
  const keepaliveIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const circuitBreakerRef = useRef<CircuitBreakerState>(createCircuitBreakerState());

  // Store callbacks in refs to avoid reconnection on every parent re-render
  const onErrorRef = useRef(onError);
  const onMessageRef = useRef(onMessage);
  onErrorRef.current = onError;
  onMessageRef.current = onMessage;

  const MAX_RECONNECT_ATTEMPTS = 10;
  const INITIAL_BACKOFF_MS = 1000;
  const MAX_BACKOFF_MS = 30000;
  const setCircuitBreakerState = useCallback(
    (updater: CircuitBreakerState | ((prev: CircuitBreakerState) => CircuitBreakerState)) => {
      setCircuitBreaker((prev) => {
        const next =
          typeof updater === 'function'
            ? (updater as (prevState: CircuitBreakerState) => CircuitBreakerState)(prev)
            : updater;
        circuitBreakerRef.current = next;
        return next;
      });
    },
    []
  );

  // Helper to update last activity timestamp
  const updateLastActivity = useCallback(() => {
    lastActivityRef.current = Date.now();
  }, []);

  // Circuit breaker helpers
  const recordSuccess = useCallback(() => {
    setCircuitBreakerState(createCircuitBreakerState());
  }, [setCircuitBreakerState]);

  const recordError = useCallback((errorMessage: string) => {
    setCircuitBreakerState((prev) => {
      const newErrorCount = prev.errorCount + 1;
      if (newErrorCount >= circuitBreakerThreshold) {
        logger.warn('SSE circuit breaker opened', {
          component: 'useSSE',
          endpoint,
          errorCount: newErrorCount,
          threshold: circuitBreakerThreshold,
        });
        return {
          errorCount: newErrorCount,
          isOpen: true,
          openedAt: Date.now(),
          lastError: errorMessage,
        };
      }
      return { ...prev, errorCount: newErrorCount, lastError: errorMessage };
    });
  }, [circuitBreakerThreshold, endpoint, setCircuitBreakerState]);

  const shouldAllowConnection = useCallback(() => {
    const breakerState = circuitBreakerRef.current;
    if (!breakerState.isOpen) {
      return true;
    }
    // Check if recovery timeout has passed
    if (breakerState.openedAt) {
      const elapsed = Date.now() - breakerState.openedAt;
      if (elapsed >= circuitBreakerRecoveryMs) {
        logger.info('SSE circuit breaker half-open, attempting recovery', {
          component: 'useSSE',
          endpoint,
        });
        return true;
      }
    }
    return false;
  }, [circuitBreakerRecoveryMs, endpoint]);

  useEffect(() => {
    if (!enabled) {
      return;
    }

    // Construct the full URL and use cookie-based session for auth
    const baseUrl = (import.meta as { env?: { VITE_API_URL?: string } }).env?.VITE_API_URL || '/api';
    const url = `${baseUrl}${endpoint}`;

    const connect = () => {
      // Check circuit breaker before connecting
      if (!shouldAllowConnection()) {
        const breakerState = circuitBreakerRef.current;
        const remainingMs = breakerState.openedAt
          ? circuitBreakerRecoveryMs - (Date.now() - breakerState.openedAt)
          : circuitBreakerRecoveryMs;
        setError(new Error(`Service unavailable (circuit open). Retry in ${Math.ceil(remainingMs / 1000)}s`));

        // Schedule recovery check
        if (circuitRecoveryTimeoutRef.current) {
          clearTimeout(circuitRecoveryTimeoutRef.current);
        }
        circuitRecoveryTimeoutRef.current = setTimeout(() => {
          if (shouldAllowConnection()) {
            connect();
          }
        }, Math.max(remainingMs, 1000));
        return;
      }

      try {
        // Close existing connection if any
        if (eventSourceRef.current) {
          eventSourceRef.current.close();
          eventSourceRef.current = null;
        }

        const eventSource = new EventSource(url, { withCredentials: true });
        eventSourceRef.current = eventSource;

        eventSource.onopen = () => {
          setConnected(true);
          setError(null);
          reconnectAttemptsRef.current = 0; // Reset attempts on successful connection
          recordSuccess(); // Reset circuit breaker on successful connection
          updateLastActivity(); // Update activity on connection open
        };

      eventSource.onmessage = (event) => {
        updateLastActivity(); // Update activity on message
        try {
          const parsed = JSON.parse(event.data);
          setData(parsed);
          if (onMessageRef.current) {
            onMessageRef.current(parsed);
          }
        } catch (e) {
          logger.error('Failed to parse default SSE message', {
            component: 'useSSE',
            endpoint,
          }, toError(e));
        }
      };

      // Handle custom event types (metrics, adapters, bundles, etc.)
      const handleCustomEvent = (event: MessageEvent) => {
        updateLastActivity(); // Update activity on custom event
        try {
          const parsed = JSON.parse(event.data);
          setData(parsed);
          if (onMessageRef.current) {
            onMessageRef.current(parsed);
          }
        } catch (e) {
          logger.error('Failed to parse custom SSE event', {
            component: 'useSSE',
            endpoint,
            eventType: event.type,
          }, toError(e));
        }
      };

      eventSource.addEventListener('metrics', handleCustomEvent);
      eventSource.addEventListener('adapters', handleCustomEvent);
      eventSource.addEventListener('bundles', handleCustomEvent);
      eventSource.addEventListener('keepalive', () => {
        updateLastActivity(); // Update activity on keepalive
      });

      // Consolidated error handler (previously had dual handlers which could conflict)
      eventSource.onerror = (event) => {
        setConnected(false);

        // Try to extract error details if available (from server error events)
        let errorMessage = 'Connection error';
        const messageEvent = event as MessageEvent;
        if (messageEvent.data && typeof messageEvent.data === 'string' && messageEvent.data.trim()) {
          try {
            const errorData = JSON.parse(messageEvent.data);
            errorMessage = errorData.error || errorMessage;
          } catch {
            // Not JSON, use the raw data as error message if it's a string
            errorMessage = messageEvent.data;
          }
        }

        // Record error for circuit breaker
        recordError(errorMessage);

        if (onErrorRef.current) {
          onErrorRef.current(event);
        }
        logger.error('SSE connection error', {
          component: 'useSSE',
          endpoint,
          errorMessage,
          circuitBreakerErrorCount: circuitBreakerRef.current.errorCount + 1,
        }, new Error(errorMessage));

        // Close current connection before reconnecting
        eventSource.close();
        eventSourceRef.current = null;

        // Attempt reconnection with exponential backoff (if circuit allows)
        if (reconnectAttemptsRef.current < MAX_RECONNECT_ATTEMPTS) {
          const backoffMs = Math.min(
            INITIAL_BACKOFF_MS * Math.pow(2, reconnectAttemptsRef.current),
            MAX_BACKOFF_MS
          );
          reconnectAttemptsRef.current += 1;
          setError(new Error(`${errorMessage}. Reconnecting in ${backoffMs / 1000}s (attempt ${reconnectAttemptsRef.current}/${MAX_RECONNECT_ATTEMPTS})`));

          // Clear existing timeout before creating new one to prevent accumulation
          if (reconnectTimeoutRef.current) {
            clearTimeout(reconnectTimeoutRef.current);
          }
          reconnectTimeoutRef.current = setTimeout(() => {
            connect();
          }, backoffMs);
        } else {
          setError(new Error(`SSE connection failed after ${MAX_RECONNECT_ATTEMPTS} attempts: ${errorMessage}`));
          logger.error('SSE max reconnection attempts exceeded', {
            component: 'useSSE',
            endpoint,
            attempts: MAX_RECONNECT_ATTEMPTS,
            errorMessage,
            circuitBreakerOpen: circuitBreakerRef.current.isOpen,
          }, new Error('Max reconnection attempts exceeded'));
        }
      };
    } catch (e) {
      setError(new Error('Failed to initialize SSE connection'));
      logger.error('Failed to initialize SSE connection', {
        component: 'useSSE',
        endpoint,
      }, toError(e));
    }
    };

    // Store connect function for manual reconnection
    connectRef.current = connect;

    connect();

    // Setup keepalive timeout check
    if (keepaliveIntervalRef.current) {
      clearInterval(keepaliveIntervalRef.current);
    }
    keepaliveIntervalRef.current = setInterval(() => {
      const elapsed = Date.now() - lastActivityRef.current;
      if (elapsed > KEEPALIVE_TIMEOUT_MS && eventSourceRef.current) {
        logger.warn('SSE connection stale, reconnecting', {
          component: 'useSSE',
          endpoint,
          lastActivityMs: elapsed,
        });
        if (eventSourceRef.current) {
          eventSourceRef.current.close();
          eventSourceRef.current = null;
        }
        setConnected(false);
        recordError('Connection stale - no keepalive received');
        // Trigger reconnection
        if (connectRef.current) {
          connectRef.current();
        }
      }
    }, KEEPALIVE_CHECK_INTERVAL_MS);

    return () => {
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
        eventSourceRef.current = null;
      }
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
        reconnectTimeoutRef.current = null;
      }
      if (circuitRecoveryTimeoutRef.current) {
        clearTimeout(circuitRecoveryTimeoutRef.current);
        circuitRecoveryTimeoutRef.current = null;
      }
      if (keepaliveIntervalRef.current) {
        clearInterval(keepaliveIntervalRef.current);
        keepaliveIntervalRef.current = null;
      }
      reconnectAttemptsRef.current = 0;
    };
    // onError and onMessage are stored in refs to avoid reconnection on parent re-renders
    // Circuit breaker functions are memoized with useCallback
  }, [endpoint, enabled, shouldAllowConnection, recordSuccess, recordError, updateLastActivity, circuitBreakerRecoveryMs]);

  // Manual reconnect function - resets attempts, circuit breaker, and reconnects
  const reconnect = useCallback(() => {
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = null;
    }
    if (circuitRecoveryTimeoutRef.current) {
      clearTimeout(circuitRecoveryTimeoutRef.current);
      circuitRecoveryTimeoutRef.current = null;
    }
    if (keepaliveIntervalRef.current) {
      clearInterval(keepaliveIntervalRef.current);
      keepaliveIntervalRef.current = null;
    }
    reconnectAttemptsRef.current = 0;
    setError(null);
    // Reset circuit breaker on manual reconnect
    setCircuitBreakerState(createCircuitBreakerState());
    updateLastActivity(); // Reset last activity on manual reconnect
    if (connectRef.current && enabled) {
      connectRef.current();
    }
  }, [enabled, updateLastActivity, setCircuitBreakerState]);

  useEffect(() => {
    const handleTenantSwitch = () => {
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
        eventSourceRef.current = null;
      }
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
        reconnectTimeoutRef.current = null;
      }
      if (circuitRecoveryTimeoutRef.current) {
        clearTimeout(circuitRecoveryTimeoutRef.current);
        circuitRecoveryTimeoutRef.current = null;
      }
      if (keepaliveIntervalRef.current) {
        clearInterval(keepaliveIntervalRef.current);
        keepaliveIntervalRef.current = null;
      }
      reconnectAttemptsRef.current = 0;
      setConnected(false);
      setData(null);
      setError(null);
      recordSuccess();
      updateLastActivity(); // Reset last activity on tenant switch
      if (connectRef.current && enabled) {
        connectRef.current();
      }
    };
    window.addEventListener(TENANT_SWITCH_EVENT, handleTenantSwitch);
    return () => window.removeEventListener(TENANT_SWITCH_EVENT, handleTenantSwitch);
  }, [enabled, recordSuccess, updateLastActivity]);

  return {
    data,
    error,
    connected,
    reconnect,
    circuitOpen: circuitBreaker.isOpen,
    reconnectAttempts: reconnectAttemptsRef.current,
  };
}
