//! Real-time activity feed hook for Dashboard
//!
//! Provides live telemetry events and audit log data for the dashboard activity feed.
//! Replaces placeholder data with actual system events.
//!
//! # Citations
//! - Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"
//! - CONTRIBUTING.md L123: "Use `tracing` for logging (not `println!`)"
//! - Dashboard.tsx L220: Uses real-time activity feed from /v1/telemetry/events/recent

import { useState, useEffect, useRef, useCallback } from 'react';
import { logger } from '@/utils/logger';
import apiClient from '@/api/client';
import type { RecentActivityEvent } from '@/api/types';

export interface ActivityEvent {
  id: string;
  timestamp: string;
  type: 'recovery' | 'policy' | 'build' | 'adapter' | 'telemetry' | 'security' | 'error' | 'collaboration';
  severity: 'info' | 'warning' | 'error' | 'critical';
  message: string;
  component?: string;
  tenantId?: string;
  userId?: string;
  workspaceId?: string;
  metadata?: Record<string, string | number | boolean>;
}

export interface UseActivityFeedOptions {
  enabled?: boolean;
  maxEvents?: number;
  tenantId?: string;
  userId?: string;
  workspaceId?: string;
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
  const { enabled = true, maxEvents = 50, tenantId, userId, workspaceId, useSSE = true } = options;

  const [events, setEvents] = useState<ActivityEvent[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const sseRef = useRef<EventSource | null>(null);
  const fallbackIntervalRef = useRef<NodeJS.Timeout | null>(null);
  const baselineIntervalRef = useRef<NodeJS.Timeout | null>(null);
  const reconnectTimerRef = useRef<NodeJS.Timeout | null>(null);
  const isMountedRef = useRef(true);
  const reconnectAttemptsRef = useRef(0);

  // Store latest values in refs to avoid recreating callbacks
  const enabledRef = useRef(enabled);
  const maxEventsRef = useRef(maxEvents);
  const tenantIdRef = useRef(tenantId);
  const userIdRef = useRef(userId);
  const workspaceIdRef = useRef(workspaceId);

  useEffect(() => {
    enabledRef.current = enabled;
    maxEventsRef.current = maxEvents;
    tenantIdRef.current = tenantId;
    userIdRef.current = userId;
    workspaceIdRef.current = workspaceId;
  }, [enabled, maxEvents, tenantId, userId, workspaceId]);

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

  const mapRecentEvent = useCallback((event: RecentActivityEvent): ActivityEvent => {
    let workspaceId: string | undefined;
    let normalizedMetadata: Record<string, string | number | boolean> | undefined;

    if (event.metadata && typeof event.metadata === 'object' && !Array.isArray(event.metadata)) {
      const metadataRecord = event.metadata as Record<string, unknown>;
      const filtered: Record<string, string | number | boolean> = {};
      for (const [key, value] of Object.entries(metadataRecord)) {
        if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
          filtered[key] = value;
        }
      }
      if (Object.keys(filtered).length > 0) {
        normalizedMetadata = filtered;
      }
      const candidateWorkspace = metadataRecord.workspace_id;
      if (typeof candidateWorkspace === 'string') {
        workspaceId = candidateWorkspace;
      }
    }

    return {
      id: event.id,
      timestamp: event.timestamp,
      type: mapEventType(event.event_type),
      severity: mapSeverity(event.level),
      message: event.message,
      component: event.component,
      tenantId: event.tenant_id,
      userId: event.user_id,
      workspaceId,
      metadata: normalizedMetadata,
    };
  }, [mapEventType, mapSeverity]);

  const fetchEvents = useCallback(async () => {
    if (!enabledRef.current || !isMountedRef.current) return;

    setLoading(true);
    setError(null);

    try {
      const recentEvents = await apiClient.getRecentActivityEvents({
        limit: maxEventsRef.current,
      });

      if (!isMountedRef.current) return;

      const mapped = recentEvents.map(mapRecentEvent).slice(0, maxEventsRef.current);
      setEvents(mapped);

      logger.info('Activity feed updated', {
        component: 'useActivityFeed',
        operation: 'fetchEvents',
        eventCount: mapped.length,
        tenantId: tenantIdRef.current,
        userId: userIdRef.current,
        workspaceId: workspaceIdRef.current,
      });
    } catch (err) {
      if (!isMountedRef.current) return;

      const errorMessage = err instanceof Error ? err.message : 'Recent activity unavailable';
      setError(errorMessage);

      logger.error('Failed to fetch recent activity', {
        component: 'useActivityFeed',
        operation: 'fetchEvents',
        tenantId: tenantIdRef.current,
        userId: userIdRef.current,
        workspaceId: workspaceIdRef.current,
      }, err instanceof Error ? err : new Error(String(err)));
    } finally {
      if (isMountedRef.current) {
        setLoading(false);
      }
    }
  }, [mapRecentEvent]);

  useEffect(() => {
    if (!enabled) {
      // Clean up everything if disabled
      if (baselineIntervalRef.current) {
        clearInterval(baselineIntervalRef.current);
        baselineIntervalRef.current = null;
      }
      if (fallbackIntervalRef.current) {
        clearInterval(fallbackIntervalRef.current);
        fallbackIntervalRef.current = null;
      }
      if (reconnectTimerRef.current) {
        clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }
      if (sseRef.current) {
        try {
          sseRef.current.close();
        } catch {}
        sseRef.current = null;
      }
      return;
    }

    isMountedRef.current = true;

    // Clean up any existing resources first
    if (baselineIntervalRef.current) {
      clearInterval(baselineIntervalRef.current);
      baselineIntervalRef.current = null;
    }
    if (fallbackIntervalRef.current) {
      clearInterval(fallbackIntervalRef.current);
      fallbackIntervalRef.current = null;
    }
    if (reconnectTimerRef.current) {
      clearTimeout(reconnectTimerRef.current);
      reconnectTimerRef.current = null;
    }
    if (sseRef.current) {
      try {
        sseRef.current.close();
      } catch {}
      sseRef.current = null;
    }

    fetchEvents();

    // Baseline polling every 30s
    baselineIntervalRef.current = setInterval(() => {
      if (isMountedRef.current && enabledRef.current) {
        fetchEvents();
      }
    }, 30000);

    // SSE live updates + reconnect with fallback polling
    reconnectAttemptsRef.current = 0;
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
      fallbackIntervalRef.current = setInterval(() => {
        if (isMountedRef.current && enabledRef.current) {
          fetchEvents();
        }
      }, 500);
    }

    function stopSSE() {
      if (sseRef.current) {
        try {
          sseRef.current.close();
        } catch {}
        sseRef.current = null;
      }
    }

    function clearReconnectTimer() {
      if (reconnectTimerRef.current) {
        clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }
    }

    function connectSSE() {
      if (!useSSE || !isMountedRef.current) return;
      try {
        // Cookie-based auth - cookies are sent automatically with EventSource
        const base = (import.meta as any)?.env?.VITE_SSE_URL
          ? `http://${(import.meta as any).env.VITE_SSE_URL}`
          : ((import.meta as any)?.env?.VITE_API_URL || '/api');
        const params = new URLSearchParams();
        params.append('limit', maxEventsRef.current.toString());
        const url = `${base}/v1/telemetry/events/recent/stream?${params.toString()}`;
        const es = new EventSource(url);
        sseRef.current = es;

        es.addEventListener('activity', (event) => {
          if (!isMountedRef.current) return;

          try {
            const payload = JSON.parse((event as MessageEvent).data);
            const incoming = Array.isArray(payload) ? payload : [payload];
            const normalized: ActivityEvent[] = incoming.map((raw: any) => {
              const recentEvent: RecentActivityEvent = {
                id: raw.id ?? `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
                timestamp: raw.timestamp ?? new Date().toISOString(),
                event_type: raw.event_type ?? raw.type ?? 'telemetry',
                level: raw.level ?? raw.severity ?? 'info',
                message: raw.message ?? 'Event',
                component: raw.component,
                tenant_id: raw.tenant_id ?? raw.tenantId,
                user_id: raw.user_id ?? raw.userId,
                metadata: raw.metadata ?? null,
              };
              return mapRecentEvent(recentEvent);
            });

            if (!isMountedRef.current) return;

            setEvents((prev) => {
              const merged = [...normalized, ...prev];
              const deduped: ActivityEvent[] = [];
              const seen = new Set<string>();
              for (const item of merged) {
                if (seen.has(item.id)) continue;
                seen.add(item.id);
                deduped.push(item);
              }
              deduped.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
              return deduped.slice(0, maxEventsRef.current);
            });
            reconnectAttemptsRef.current = 0;
            clearFallback();
          } catch (err) {
            logger.error('Failed to parse activity SSE payload', { component: 'useActivityFeed', operation: 'sse_activity_parse' }, err as Error);
          }
        });

        es.addEventListener('open', () => {
          if (!isMountedRef.current) return;
          reconnectAttemptsRef.current = 0;
          clearFallback();
        });

        es.addEventListener('error', (evt: any) => {
          if (!isMountedRef.current) return;

          reconnectAttemptsRef.current++;
          const unauthorized = evt?.status === 401 || evt?.code === 401;
          if (unauthorized) {
            setError('Unauthorized');
            logger.error('Activity SSE unauthorized', { component: 'useActivityFeed', operation: 'sse_error' }, new Error('Unauthorized'));
          }

          if (reconnectAttemptsRef.current >= maxReconnect) {
            logger.error('Max SSE reconnect threshold reached (activity)', {
              component: 'useActivityFeed',
              operation: 'sse_reconnect',
              reconnectAttempts: reconnectAttemptsRef.current,
              maxReconnect,
            });
            startFallbackPolling();
            stopSSE();
            return;
          }

          const delay = Math.min(baseDelay * Math.pow(2, reconnectAttemptsRef.current - 1), 30000);
          startFallbackPolling();
          stopSSE();

          clearReconnectTimer();
          reconnectTimerRef.current = setTimeout(() => {
            if (!isMountedRef.current) return;
            reconnectTimerRef.current = null;
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
      isMountedRef.current = false;

      if (baselineIntervalRef.current) {
        clearInterval(baselineIntervalRef.current);
        baselineIntervalRef.current = null;
      }
      clearFallback();
      clearReconnectTimer();
      stopSSE();
    };
  }, [enabled, useSSE, mapRecentEvent, fetchEvents]);

  return {
    events,
    loading,
    error,
    refresh: fetchEvents,
  };
}
