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
): { data: T | null; error: string | null; connected: boolean } {
  const { enabled = true, onError, onMessage } = options;
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [connected, setConnected] = useState(false);
  const eventSourceRef = useRef<EventSource | null>(null);

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

    try {
      const eventSource = new EventSource(url);
      eventSourceRef.current = eventSource;

      eventSource.onopen = () => {
        setConnected(true);
        setError(null);
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
          }, toError(parseErr));
          setError('Connection error');
        }
      });

      eventSource.onerror = (event) => {
        setConnected(false);
        setError('SSE connection error');
        if (onError) {
          onError(event);
        }
        logger.error('SSE connection error', {
          component: 'useSSE',
          endpoint,
        }, new Error('SSE connection error'));
      };

      return () => {
        eventSource.close();
        eventSourceRef.current = null;
      };
    } catch (e) {
      setError('Failed to initialize SSE connection');
      logger.error('Failed to initialize SSE connection', {
        component: 'useSSE',
        endpoint,
      }, toError(e));
    }
    // Note: onError and onMessage are intentionally in dependencies.
    // If callers want to avoid reconnections, they should memoize these callbacks.
  }, [endpoint, enabled, onError, onMessage]);

  return { data, error, connected };
}
