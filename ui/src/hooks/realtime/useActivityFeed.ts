//! Real-time activity feed hook for Dashboard
//!
//! Provides live telemetry events and audit log data for the dashboard activity feed.
//! Replaces placeholder data with actual system events.
//!
//! # Citations
//! - Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"
//! - CONTRIBUTING.md L123: "Use `tracing` for logging (not `println!`)"
//! - Dashboard.tsx L220: Uses real-time activity feed from /v1/telemetry/events/recent
//! - ui/src/hooks/realtime/useSSEWithPollingFallback.ts: Unified SSE + polling implementation

import { useCallback, useMemo, useRef, useEffect } from 'react';
import { logger } from '@/utils/logger';
import apiClient from '@/api/client';
import type { RecentActivityEvent } from '@/api/types';
import { useSSEWithPollingFallback } from '@/hooks/realtime';

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
  isLoading: boolean;
  error: Error | null;
  refetch: () => Promise<void>;
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
 * * `isLoading` - Loading state
 * * `error` - Error message if any
 * * `refetch` - Function to manually refresh events
 *
 * # Policy Compliance
 *
 * - Policy Pack #9 (Telemetry): Uses canonical JSON structure
 * - Policy Pack #1 (Egress): Uses relative API paths only
 *
 * # Implementation Note
 *
 * This hook now uses the unified `useSSEWithPollingFallback` hook to consolidate
 * duplicate SSE + polling logic. The original ~450 LOC implementation has been
 * reduced to ~150 LOC by leveraging the shared abstraction.
 */
export function useActivityFeed(options: UseActivityFeedOptions = {}): UseActivityFeedReturn {
  const { enabled = true, maxEvents = 50, tenantId, userId, workspaceId, useSSE = true } = options;

  // Store latest values in refs to avoid recreating callbacks
  const maxEventsRef = useRef(maxEvents);
  const tenantIdRef = useRef(tenantId);
  const userIdRef = useRef(userId);
  const workspaceIdRef = useRef(workspaceId);

  useEffect(() => {
    maxEventsRef.current = maxEvents;
    tenantIdRef.current = tenantId;
    userIdRef.current = userId;
    workspaceIdRef.current = workspaceId;
  }, [maxEvents, tenantId, userId, workspaceId]);

  // ============================================================================
  // Event Mapping Functions
  // ============================================================================

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
      type: mapEventType(event.event_type ?? 'telemetry'),
      severity: mapSeverity(event.level ?? 'info'),
      message: event.message ?? 'Event',
      component: event.component,
      tenantId: event.tenant_id,
      userId: event.user_id,
      workspaceId,
      metadata: normalizedMetadata,
    };
  }, [mapEventType, mapSeverity]);

  // ============================================================================
  // Polling Function
  // ============================================================================

  const fetchEvents = useCallback(async (): Promise<ActivityEvent[]> => {
    const recentEvents = await apiClient.getRecentActivityEvents({
      limit: maxEventsRef.current,
    });

    const mapped = recentEvents.map(mapRecentEvent).slice(0, maxEventsRef.current);

    logger.info('Activity feed updated', {
      component: 'useActivityFeed',
      operation: 'fetchEvents',
      eventCount: mapped.length,
      tenantId: tenantIdRef.current,
      userId: userIdRef.current,
      workspaceId: workspaceIdRef.current,
    });

    return mapped;
  }, [mapRecentEvent]);

  // ============================================================================
  // SSE Transform Function
  // ============================================================================

  const transformSSE = useCallback((sseData: unknown): ActivityEvent[] => {
    const payload = sseData;
    const incoming = Array.isArray(payload) ? payload : [payload];

    const normalized: ActivityEvent[] = incoming.map((raw: unknown) => {
      const rawObj = raw as Record<string, unknown>;
      const recentEvent: RecentActivityEvent = {
        id: (rawObj.id as string | undefined) ?? `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        timestamp: (rawObj.timestamp as string | undefined) ?? new Date().toISOString(),
        event_type: (rawObj.event_type as string | undefined) ?? (rawObj.type as string | undefined) ?? 'telemetry',
        level: (rawObj.level as string | undefined) ?? (rawObj.severity as string | undefined) ?? 'info',
        message: (rawObj.message as string | undefined) ?? 'Event',
        component: rawObj.component as string | undefined,
        tenant_id: (rawObj.tenant_id as string | undefined) ?? (rawObj.tenantId as string | undefined),
        user_id: (rawObj.user_id as string | undefined) ?? (rawObj.userId as string | undefined),
        metadata: (rawObj.metadata ?? null) as Record<string, unknown>,
      };
      return mapRecentEvent(recentEvent);
    });

    return normalized;
  }, [mapRecentEvent]);

  // ============================================================================
  // Use Shared SSE + Polling Hook
  // ============================================================================

  const sseEndpoint = useMemo(() => {
    const params = new URLSearchParams();
    params.append('limit', maxEvents.toString());
    return `/v1/telemetry/events/recent/stream?${params.toString()}`;
  }, [maxEvents]);

  const {
    data: rawEvents,
    isLoading,
    error: hookError,
    refetch,
  } = useSSEWithPollingFallback<ActivityEvent[]>({
    sseEndpoint,
    sseEventType: 'activity',
    pollingFn: fetchEvents,
    pollingSpeed: 'normal',
    enabled,
    useSSE,
    transformSSE,
    onError: (err, source) => {
      logger.error(`Activity feed error from ${source}`, {
        component: 'useActivityFeed',
        operation: 'realtime',
        source,
        tenantId: tenantIdRef.current,
        userId: userIdRef.current,
        workspaceId: workspaceIdRef.current,
      }, err);
    },
    operationName: 'useActivityFeed',
    baselinePollingIntervalMs: 30000, // Baseline polling every 30s
    sseInitialBackoffMs: 500,
    sseMaxReconnectAttempts: 5,
  });

  // ============================================================================
  // Dedupe and Sort Events
  // ============================================================================

  const events = useMemo(() => {
    if (!rawEvents) return [];

    // Dedupe by ID
    const deduped: ActivityEvent[] = [];
    const seen = new Set<string>();
    for (const item of rawEvents) {
      if (seen.has(item.id)) continue;
      seen.add(item.id);
      deduped.push(item);
    }

    // Sort by timestamp descending
    deduped.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());

    // Limit to maxEvents
    return deduped.slice(0, maxEvents);
  }, [rawEvents, maxEvents]);

  return {
    events,
    isLoading,
    error: hookError ?? null,
    refetch,
  };
}
