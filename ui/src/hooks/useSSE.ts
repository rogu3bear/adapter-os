import { useEffect, useState, useRef } from 'react';
import apiClient from '../api/client';

export interface UseSSEOptions {
  enabled?: boolean;
  onError?: (error: Event) => void;
  onMessage?: (data: any) => void;
}

/**
 * Custom hook for SSE (Server-Sent Events) subscriptions
 * @param endpoint - The API endpoint path (e.g., '/v1/stream/metrics')
 * @param options - Configuration options
 * @returns The latest data received from the SSE stream
 */
export function useSSE<T = any>(
  endpoint: string,
  options: UseSSEOptions = {}
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
    const baseUrl = (import.meta as any).env?.VITE_API_URL || '/api';
    const url = `${baseUrl}${endpoint}`;
    
    // Note: EventSource doesn't support custom headers
    // SSE endpoints are protected by cookie-based session auth from the initial page load

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
          console.error('Failed to parse SSE message:', e);
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
          console.error('Failed to parse SSE event:', e);
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
          const errorData = JSON.parse((event as any).data);
          setError(errorData.error || 'SSE error');
        } catch {
          setError('Connection error');
        }
      });

      eventSource.onerror = (event) => {
        setConnected(false);
        setError('SSE connection error');
        if (onError) {
          onError(event);
        }
      };

      return () => {
        eventSource.close();
        eventSourceRef.current = null;
      };
    } catch (e) {
      setError('Failed to initialize SSE connection');
      console.error('SSE initialization error:', e);
    }
  }, [endpoint, enabled, onError, onMessage]);

  return { data, error, connected };
}

