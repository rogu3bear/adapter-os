//! Real-time activity feed hook for Dashboard
//!
//! Provides live telemetry events and audit log data for the dashboard activity feed.
//! Replaces placeholder data with actual system events.
//!
//! # Citations
//! - Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"
//! - CONTRIBUTING.md L123: "Use `tracing` for logging (not `println!`)"
//! - Dashboard.tsx L220: "TODO: Replace with real-time activity feed from /v1/telemetry/events or audit log"

import { useState, useEffect } from 'react';
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
  const { enabled = true, maxEvents = 50, tenantId, userId } = options;
  
  const [events, setEvents] = useState<ActivityEvent[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchEvents = async () => {
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
  };

  // Map telemetry event types to activity types
  const mapEventType = (eventType: string): ActivityEvent['type'] => {
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
  };

  // Map log levels to severity levels
  const mapSeverity = (level: string): ActivityEvent['severity'] => {
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
  };

  useEffect(() => {
    fetchEvents();
    
    // Set up polling for real-time updates (every 30 seconds)
    const interval = setInterval(fetchEvents, 30000);
    
    return () => clearInterval(interval);
  }, [enabled, maxEvents, tenantId, userId]);

  return {
    events,
    loading,
    error,
    refresh: fetchEvents,
  };
}
