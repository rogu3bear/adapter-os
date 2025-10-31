//! Real-time activity feed hook for Dashboard
//!
//! Provides live telemetry events and audit log data for the dashboard activity feed.
//! Replaces placeholder data with actual system events.
//!
//! # Citations
//! - Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"
//! - CONTRIBUTING.md L123: "Use `tracing` for logging (not `println!`)"
//! - Dashboard.tsx L220: "TODO: Replace with real-time activity feed from /v1/telemetry/events or audit log"

import { useState, useEffect, useRef, useCallback } from 'react';
import { logger } from '../utils/logger';
import apiClient from '../api/client';

export interface ActivityEvent {
  id: string;
  timestamp: string;
  type: 'recovery' | 'policy' | 'build' | 'adapter' | 'telemetry' | 'security' | 'error';
  severity: 'info' | 'warning' | 'error' | 'critical';
  message: string;
  component?: string;
  tenantId?: string;
  userId?: string;
  metadata?: Record<string, string | number | boolean>;
}

export interface UseActivityFeedOptions {
  enabled?: boolean;
  maxEvents?: number;
  tenantId?: string;
  userId?: string;
  useSSE?: boolean;
}

export interface UseActivityFeedReturn {
  events: ActivityEvent[];
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

/**
 * Hook for fetching real-time activity events from telemetry and audit logs
 *
 * # Arguments
 *
 * * `options` - Configuration options for the activity feed
 *
 * # Returns
 *
 * * `events` - Array of activity events
 * * `loading` - Loading state
 * * `error` - Error message if any
 * * `refresh` - Function to manually refresh events
 *
 * # Policy Compliance
 *
 * - Policy Pack #9 (Telemetry): Uses canonical JSON structure
 * - Policy Pack #1 (Egress): Uses relative API paths only
 */
export function useActivityFeed(options: UseActivityFeedOptions = {}): UseActivityFeedReturn {
  const { enabled = true, maxEvents = 50, tenantId, userId, useSSE = true } = options;
  
  const [events, setEvents] = useState<ActivityEvent[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const sseRef = useRef<EventSource | null>(null);
  const fallbackIntervalRef = useRef<number | null>(null);

  const mapEventType = useCallback((eventType: string): ActivityEvent['type'] => {
    switch (eventType) {
      case 'node_recovery':
      case 'worker_recovery':
        return 'recovery';
      case 'policy_update':
      case 'policy_sign':
        return 'policy';
      case 'build_complete':
      case 'plan_compile':
        return 'build';
      case 'adapter_register':
      case 'adapter_deploy':
        return 'adapter';
      case 'telemetry_export':
      case 'audit_log':
        return 'telemetry';
      case 'security_violation':
      case 'access_denied':
        return 'security';
      case 'error':
      case 'exception':
        return 'error';
      default:
        return 'telemetry';
    }
  }, []);

  const mapSeverity = useCallback((level: string): ActivityEvent['severity'] => {
    switch (level.toLowerCase()) {
      case 'error':
        return 'error';
      case 'warn':
      case 'warning':
        return 'warning';
      case 'critical':
      case 'fatal':
        return 'critical';
      default:
        return 'info';
    }
  }, []);

  const fetchEvents = useCallback(async () => {
    if (!enabled) return;

    setLoading(true);
    setError(null);

    try {
      // Fetch telemetry events from the audit log
      const telemetryEvents = await apiClient.getTelemetryEvents({
        limit: maxEvents,
        tenantId,
        userId,
        startTime: new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString(), // Last 24 hours
      });

      // Transform telemetry events to activity events
      const activityEvents: ActivityEvent[] = telemetryEvents.map(event => ({
        id: event.id,
        timestamp: event.timestamp,
        type: mapEventType(event.event_type),
        severity: mapSeverity(event.level),
        message: event.message,
        component: event.component,
        tenantId: event.tenant_id,
        userId: event.user_id,
        metadata: event.metadata,
      }));

      // Sort by timestamp (newest first)
      activityEvents.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());

      setEvents(activityEvents);

      logger.info('Activity feed updated', {
        component: 'useActivityFeed',
        operation: 'fetchEvents',
        eventCount: activityEvents.length,
        tenantId,
        userId
      });
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to fetch activity events';
      setError(errorMessage);

      logger.error('Failed to fetch activity events', {
        component: 'useActivityFeed',
        operation: 'fetchEvents',
        tenantId,
        userId
      }, err instanceof Error ? err : new Error(String(err)));
    } finally {
      setLoading(false);
    }
  }, [enabled, mapEventType, mapSeverity, maxEvents, tenantId, userId]);

  useEffect(() => {
    fetchEvents();

    // Baseline polling every 30s
    const interval = setInterval(fetchEvents, 30000);

    // SSE live updates + reconnect with fallback polling
    let reconnectAttempts = 0;
    const maxReconnect = 5;
    const baseDelay = 500;

    function clearFallback() {
      if (fallbackIntervalRef.current) {
        clearInterval(fallbackIntervalRef.current);
        fallbackIntervalRef.current = null;
      }
    }

    function startFallbackPolling() {
      clearFallback();
      // quick polling while disconnected
      fallbackIntervalRef.current = setInterval(fetchEvents, 500) as unknown as number;
    }

    function stopSSE() {
      if (sseRef.current) {
        try { sseRef.current.close(); } catch {}
        sseRef.current = null;
      }
    }

    function connectSSE() {
      if (!useSSE) return;
      try {
        const token = typeof localStorage !== 'undefined' ? localStorage.getItem('aos_token') : null;
        const base = (import.meta as any)?.env?.VITE_SSE_URL
          ? `http://${(import.meta as any).env.VITE_SSE_URL}`
          : ((import.meta as any)?.env?.VITE_API_URL || '/api');
        const url = `${base}/v1/stream/telemetry${token ? `?token=${encodeURIComponent(token)}` : ''}`;
        const es = new EventSource(url);
        sseRef.current = es;

        es.addEventListener('telemetry', (event) => {
          try {
            const payload = JSON.parse((event as MessageEvent).data);
            const incoming = Array.isArray(payload) ? payload : [payload];
            const normalized: ActivityEvent[] = incoming.map((ev: any) => ({
              id: ev.id ?? `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
              timestamp: ev.timestamp ?? new Date().toISOString(),
              type: ev.type ?? (ev.event_type ? mapEventType(ev.event_type) : 'telemetry'),
              severity: ev.severity ?? (ev.level ? mapSeverity(ev.level) : 'info'),
              message: ev.message ?? 'Event',
              component: ev.component,
              tenantId: ev.tenantId ?? ev.tenant_id,
              userId: ev.userId ?? ev.user_id,
              metadata: ev.metadata,
            }));
            setEvents((prev) => {
              const merged = [...normalized, ...prev];
              merged.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
              return merged.slice(0, maxEvents);
            });
            reconnectAttempts = 0;
            clearFallback();
          } catch (err) {
            logger.error('Failed to parse activity SSE payload', { component: 'useActivityFeed', operation: 'sse_telemetry_parse' }, err as Error);
          }
        });

        es.addEventListener('open', () => {
          reconnectAttempts = 0;
        });

        es.addEventListener('error', (evt: any) => {
          reconnectAttempts++;
          const unauthorized = evt?.status === 401 || evt?.code === 401;
          if (unauthorized) {
            setError('Unauthorized');
            logger.error('Activity SSE unauthorized', { component: 'useActivityFeed', operation: 'sse_error' }, new Error('Unauthorized'));
          }

          if (reconnectAttempts >= maxReconnect) {
            logger.error('Max SSE reconnect threshold reached (activity)', {
              component: 'useActivityFeed',
              operation: 'sse_reconnect',
              reconnectAttempts,
              maxReconnect,
            });
            startFallbackPolling();
            stopSSE();
            return;
          }

          const delay = Math.min(baseDelay * Math.pow(2, reconnectAttempts - 1), 30000);
          startFallbackPolling();
          stopSSE();
          setTimeout(() => {
            clearFallback();
            connectSSE();
          }, delay);
        });
      } catch (err) {
        logger.error('Failed to initialize activity SSE', { component: 'useActivityFeed', operation: 'sse_init' }, err as Error);
        startFallbackPolling();
      }
    }

    connectSSE();

    return () => {
      clearInterval(interval);
      clearFallback();
      stopSSE();
    };
  }, [enabled, fetchEvents, mapEventType, mapSeverity, maxEvents, tenantId, userId, useSSE]);

  return {
    events,
    loading,
    error,
    refresh: fetchEvents,
  };
}
