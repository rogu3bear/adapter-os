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
import { useActivityEvents } from './useActivityEvents';

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

  const fetchEvents = useCallback(async () => {
    if (!enabledRef.current || !isMountedRef.current) return;

    setLoading(true);
    setError(null);

    try {
      // Fetch both telemetry events and activity events in parallel
      const [telemetryEvents, activityEventsResponse] = await Promise.all([
        // Fetch telemetry events from the audit log
        apiClient.getTelemetryEvents({
          limit: Math.floor(maxEventsRef.current / 2), // Split limit between sources
          tenantId: tenantIdRef.current,
          userId: userIdRef.current,
          startTime: new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString(), // Last 24 hours
        }),
        // Fetch activity events (collaboration events)
        apiClient.listActivityEvents({
          workspace_id: workspaceIdRef.current,
          user_id: userIdRef.current,
          tenant_id: tenantIdRef.current,
          limit: Math.floor(maxEventsRef.current / 2), // Split limit between sources
        }),
      ]);

      if (!isMountedRef.current) return;

      // Transform telemetry events to activity events
      const telemetryActivityEvents: ActivityEvent[] = telemetryEvents.map(event => ({
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

      // Transform activity events to unified format, adding collaboration event types
      const activityEvents: ActivityEvent[] = activityEventsResponse.map(event => {
        let eventType: ActivityEvent['type'] = 'telemetry'; // default

        switch (event.event_type) {
          case 'resource_created':
          case 'resource_updated':
          case 'resource_deleted':
            eventType = 'adapter'; // Map to adapter operations
            break;
          case 'user_joined':
          case 'user_left':
            eventType = 'recovery'; // Map to user management
            break;
          case 'message_sent':
          case 'message_edited':
            eventType = 'telemetry'; // Keep as telemetry for now
            break;
          case 'comment_added':
            eventType = 'build'; // Map to collaboration
            break;
          case 'resource_shared':
            eventType = 'adapter'; // Map to resource operations
            break;
          case 'policy_violation':
            eventType = 'policy';
            break;
          case 'system_alert':
            eventType = 'security';
            break;
          default:
            eventType = 'telemetry';
        }

        return {
          id: event.id,
          timestamp: event.created_at,
          type: eventType,
          severity: 'info' as const, // Default severity for activity events
          message: `Activity: ${event.event_type.replace('_', ' ')}`,
          component: event.target_type || 'activity',
          tenantId: event.tenant_id,
          userId: event.user_id || undefined,
          metadata: event.metadata_json ? JSON.parse(event.metadata_json) : undefined,
        };
      });

      // Merge and deduplicate events
      const allEvents = [...telemetryActivityEvents, ...activityEvents];
      const uniqueEvents = allEvents.filter(
        (event, index, self) => self.findIndex(e => e.id === event.id) === index
      );

      // Sort by timestamp (newest first)
      uniqueEvents.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());

      // Limit to maxEvents
      const limitedEvents = uniqueEvents.slice(0, maxEventsRef.current);

      if (!isMountedRef.current) return;

      setEvents(limitedEvents);

      logger.info('Activity feed updated', {
        component: 'useActivityFeed',
        operation: 'fetchEvents',
        eventCount: limitedEvents.length,
        telemetryCount: telemetryActivityEvents.length,
        activityCount: activityEvents.length,
        tenantId: tenantIdRef.current,
        userId: userIdRef.current,
        workspaceId: workspaceIdRef.current,
      });
    } catch (err) {
      if (!isMountedRef.current) return;
      
      const errorMessage = err instanceof Error ? err.message : 'Failed to fetch activity events';
      setError(errorMessage);

      logger.error('Failed to fetch activity events', {
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
  }, [mapEventType, mapSeverity]); // Removed other deps - use refs

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
      stopSSE();
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
    stopSSE();

    fetchEvents();

    // Baseline polling every 30s
    baselineIntervalRef.current = setInterval(() => {
      if (isMountedRef.current && enabledRef.current) {
        fetchEvents();
      }
    }, 30000);

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
        const url = `${base}/v1/stream/telemetry`;
        const es = new EventSource(url);
        sseRef.current = es;

        es.addEventListener('telemetry', (event) => {
          if (!isMountedRef.current) return;
          
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
            
            if (!isMountedRef.current) return;
            
            setEvents((prev) => {
              const merged = [...normalized, ...prev];
              merged.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
              return merged.slice(0, maxEventsRef.current);
            });
            reconnectAttempts = 0;
            clearFallback();
          } catch (err) {
            logger.error('Failed to parse activity SSE payload', { component: 'useActivityFeed', operation: 'sse_telemetry_parse' }, err as Error);
          }
        });

        es.addEventListener('open', () => {
          if (!isMountedRef.current) return;
          reconnectAttempts = 0;
          clearFallback();
        });

        es.addEventListener('error', (evt: any) => {
          if (!isMountedRef.current) return;
          
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
  }, [enabled, useSSE, mapEventType, mapSeverity]); // Removed fetchEvents and other deps

  return {
    events,
    loading,
    error,
    refresh: fetchEvents,
  };
}
