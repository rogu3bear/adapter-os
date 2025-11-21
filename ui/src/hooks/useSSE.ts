import { useEffect, useState, useRef } from 'react';
import apiClient from '../api/client';
import { logger, toError } from '../utils/logger';

//! Strongly typed SSE hook options
//!
//! # Citations
//! - TypeScript best practices: Avoid `any` types for type safety
//! - CONTRIBUTING.md L118-122: "Follow Rust naming conventions"

export interface UseSSEOptions<T = unknown> {
  enabled?: boolean;
  onError?: (error: Event) => void;
  onMessage?: (data: T) => void;
}

/**
 * Custom hook for SSE (Server-Sent Events) subscriptions
 * @param endpoint - The API endpoint path (e.g., '/v1/stream/metrics')
 * @param options - Configuration options
 * @returns The latest data received from the SSE stream
 */
export function useSSE<T = unknown>(
  endpoint: string,
  options: UseSSEOptions<T> = {}
): { data: T | null; error: string | null; connected: boolean; reconnect: () => void } {
  const { enabled = true, onError, onMessage } = options;
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [connected, setConnected] = useState(false);
  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const connectRef = useRef<(() => void) | null>(null);

  const MAX_RECONNECT_ATTEMPTS = 10;
  const INITIAL_BACKOFF_MS = 1000;
  const MAX_BACKOFF_MS = 30000;

  useEffect(() => {
    if (!enabled) {
      return;
    }

    // Construct the full URL
    const baseUrl = (import.meta as { env?: { VITE_API_URL?: string } }).env?.VITE_API_URL || '/api';

    const token = apiClient.getToken();

    // EventSource doesn't support custom headers, so we append the token as a query parameter
    // The server must validate this token in SSE endpoints
    const url = token ? `${baseUrl}${endpoint}?token=${encodeURIComponent(token)}` : `${baseUrl}${endpoint}`;

    // Note: SSE authentication requires token in query string since EventSource doesn't support Authorization headers
    // Server-side handlers must extract and validate the token from query parameters

    const connect = () => {
      try {
        // Close existing connection if any
        if (eventSourceRef.current) {
          eventSourceRef.current.close();
          eventSourceRef.current = null;
        }

        const eventSource = new EventSource(url);
        eventSourceRef.current = eventSource;

        eventSource.onopen = () => {
          setConnected(true);
          setError(null);
          reconnectAttemptsRef.current = 0; // Reset attempts on successful connection
        };

      eventSource.onmessage = (event) => {
        try {
          const parsed = JSON.parse(event.data);
          setData(parsed);
          if (onMessage) {
            onMessage(parsed);
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
          if (onMessage) {
            onMessage(parsed);
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

      eventSource.addEventListener('error', (event) => {
        try {
          const errorData = JSON.parse((event as MessageEvent).data);
          setError(errorData.error || 'SSE error');
        } catch (parseErr) {
          logger.warn('Failed to parse SSE error event data', {
            component: 'useSSE',
            endpoint,
            operation: 'parse_error_event',
          });
          setError('Connection error');
        }
      });

      eventSource.onerror = (event) => {
        setConnected(false);
        if (onError) {
          onError(event);
        }
        logger.error('SSE connection error', {
          component: 'useSSE',
          endpoint,
        }, new Error('SSE connection error'));

        // Close current connection before reconnecting
        eventSource.close();
        eventSourceRef.current = null;

        // Attempt reconnection with exponential backoff
        if (reconnectAttemptsRef.current < MAX_RECONNECT_ATTEMPTS) {
          const backoffMs = Math.min(
            INITIAL_BACKOFF_MS * Math.pow(2, reconnectAttemptsRef.current),
            MAX_BACKOFF_MS
          );
          reconnectAttemptsRef.current += 1;
          setError(`SSE connection error. Reconnecting in ${backoffMs / 1000}s (attempt ${reconnectAttemptsRef.current}/${MAX_RECONNECT_ATTEMPTS})`);

          reconnectTimeoutRef.current = setTimeout(() => {
            connect();
          }, backoffMs);
        } else {
          setError(`SSE connection failed after ${MAX_RECONNECT_ATTEMPTS} attempts`);
          logger.error('SSE max reconnection attempts exceeded', {
            component: 'useSSE',
            endpoint,
            attempts: MAX_RECONNECT_ATTEMPTS,
          }, new Error('Max reconnection attempts exceeded'));
        }
      };
    } catch (e) {
      setError('Failed to initialize SSE connection');
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
      reconnectAttemptsRef.current = 0;
    };
    // Note: onError and onMessage are intentionally in dependencies.
    // If callers want to avoid reconnections, they should memoize these callbacks.
  }, [endpoint, enabled, onError, onMessage]);

  // Manual reconnect function - resets attempts and reconnects
  const reconnect = () => {
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = null;
    }
    reconnectAttemptsRef.current = 0;
    setError(null);
    if (connectRef.current && enabled) {
      connectRef.current();
    }
  };

  return { data, error, connected, reconnect };
}
