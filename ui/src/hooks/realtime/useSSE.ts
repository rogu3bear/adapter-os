import { useEffect, useState, useRef, useCallback } from 'react';
import { logger, toError } from '@/utils/logger';
import { TENANT_SWITCH_EVENT } from '@/utils/tenant';

//! Strongly typed SSE hook options
//!
//! # Citations
//! - TypeScript best practices: Avoid `any` types for type safety
//! - CONTRIBUTING.md L118-122: "Follow Rust naming conventions"

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
  const [circuitBreaker, setCircuitBreaker] = useState<CircuitBreakerState>({
    errorCount: 0,
    isOpen: false,
    openedAt: null,
    lastError: null,
  });
  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const circuitRecoveryTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const connectRef = useRef<(() => void) | null>(null);

  // Store callbacks in refs to avoid reconnection on every parent re-render
  const onErrorRef = useRef(onError);
  const onMessageRef = useRef(onMessage);
  onErrorRef.current = onError;
  onMessageRef.current = onMessage;

  const MAX_RECONNECT_ATTEMPTS = 10;
  const INITIAL_BACKOFF_MS = 1000;
  const MAX_BACKOFF_MS = 30000;

  // Circuit breaker helpers
  const recordSuccess = useCallback(() => {
    setCircuitBreaker({
      errorCount: 0,
      isOpen: false,
      openedAt: null,
      lastError: null,
    });
  }, []);

  const recordError = useCallback((errorMessage: string) => {
    setCircuitBreaker((prev) => {
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
  }, [circuitBreakerThreshold, endpoint]);

  const shouldAllowConnection = useCallback(() => {
    if (!circuitBreaker.isOpen) {
      return true;
    }
    // Check if recovery timeout has passed
    if (circuitBreaker.openedAt) {
      const elapsed = Date.now() - circuitBreaker.openedAt;
      if (elapsed >= circuitBreakerRecoveryMs) {
        logger.info('SSE circuit breaker half-open, attempting recovery', {
          component: 'useSSE',
          endpoint,
        });
        return true;
      }
    }
    return false;
  }, [circuitBreaker.isOpen, circuitBreaker.openedAt, circuitBreakerRecoveryMs, endpoint]);

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
        const remainingMs = circuitBreaker.openedAt
          ? circuitBreakerRecoveryMs - (Date.now() - circuitBreaker.openedAt)
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
        };

      eventSource.onmessage = (event) => {
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
        // Just acknowledge keepalive
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
          circuitBreakerErrorCount: circuitBreaker.errorCount + 1,
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
            circuitBreakerOpen: circuitBreaker.isOpen,
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
      reconnectAttemptsRef.current = 0;
    };
    // onError and onMessage are stored in refs to avoid reconnection on parent re-renders
    // Circuit breaker functions are memoized with useCallback
  }, [endpoint, enabled, shouldAllowConnection, recordSuccess, recordError, circuitBreaker.openedAt, circuitBreakerRecoveryMs, circuitBreaker.errorCount, circuitBreaker.isOpen]);

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
    reconnectAttemptsRef.current = 0;
    setError(null);
    // Reset circuit breaker on manual reconnect
    setCircuitBreaker({
      errorCount: 0,
      isOpen: false,
      openedAt: null,
      lastError: null,
    });
    if (connectRef.current && enabled) {
      connectRef.current();
    }
  }, [enabled]);

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
      reconnectAttemptsRef.current = 0;
      setConnected(false);
      setData(null);
      setError(null);
      recordSuccess();
      if (connectRef.current && enabled) {
        connectRef.current();
      }
    };
    window.addEventListener(TENANT_SWITCH_EVENT, handleTenantSwitch);
    return () => window.removeEventListener(TENANT_SWITCH_EVENT, handleTenantSwitch);
  }, [enabled, recordSuccess]);

  return {
    data,
    error,
    connected,
    reconnect,
    circuitOpen: circuitBreaker.isOpen,
    reconnectAttempts: reconnectAttemptsRef.current,
  };
}
