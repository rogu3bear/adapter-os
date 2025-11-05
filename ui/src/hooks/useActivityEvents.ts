//! Activity events hook
//!
//! Merges user activity events with telemetry events for comprehensive activity tracking.
//! Supports workspace filtering for collaboration events.
//!
//! Citation: ui/src/hooks/useActivityFeed.ts L113-L166 (fetchEvents pattern)
//! - Merge activity_events with telemetry events

import { useState, useEffect, useCallback } from 'react';
import { logger, toError } from '../utils/logger';
import apiClient from '../api/client';
import { ActivityEvent, CreateActivityEventRequest } from '../api/types';

export interface UseActivityEventsOptions {
  enabled?: boolean;
  maxEvents?: number;
  workspaceId?: string;
  userId?: string;
  tenantId?: string;
  eventType?: string;
}

export interface UseActivityEventsReturn {
  events: ActivityEvent[];
  loading: boolean;
  error: string | null;
  createEvent: (data: CreateActivityEventRequest) => Promise<ActivityEvent>;
  refresh: () => Promise<void>;
}

/**
 * Hook for activity events (collaboration + telemetry)
 *
 * # Arguments
 *
 * * `options` - Configuration options for activity events
 *
 * # Returns
 *
 * * `events` - Array of activity event objects
 * * `loading` - Loading state
 * * `error` - Error message if any
 * * `createEvent` - Function to create a new activity event
 * * `refresh` - Function to manually refresh events
 *
 * # Policy Compliance
 *
 * - Policy Pack #9 (Telemetry): Uses canonical JSON structure
 * - Policy Pack #1 (Egress): Uses relative API paths only
 */
export function useActivityEvents(options: UseActivityEventsOptions = {}): UseActivityEventsReturn {
  const { enabled = true, maxEvents = 50, workspaceId, userId, tenantId, eventType } = options;

  const [events, setEvents] = useState<ActivityEvent[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchEvents = useCallback(async () => {
    if (!enabled) return;

    setLoading(true);
    setError(null);

    try {
      // Fetch activity events
      const activityEvents = await apiClient.listActivityEvents({
        workspace_id: workspaceId,
        user_id: userId,
        tenant_id: tenantId,
        event_type: eventType,
        limit: maxEvents,
      });

      // If no workspace filter, also fetch user workspace activity for broader context
      let userWorkspaceEvents: ActivityEvent[] = [];
      if (!workspaceId) {
        userWorkspaceEvents = await apiClient.listUserWorkspaceActivity(maxEvents);
      }

      // Merge and deduplicate events
      const allEvents = [...activityEvents, ...userWorkspaceEvents];
      const uniqueEvents = allEvents.filter(
        (event, index, self) => self.findIndex(e => e.id === event.id) === index
      );

      // Sort by created_at (newest first)
      const sortedEvents = uniqueEvents.sort(
        (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
      );

      // Limit to maxEvents
      const limitedEvents = sortedEvents.slice(0, maxEvents);

      setEvents(limitedEvents);

      logger.info('Activity events updated', {
        component: 'useActivityEvents',
        operation: 'fetchEvents',
        eventCount: limitedEvents.length,
        workspaceId,
        userId,
        tenantId,
        eventType,
      });
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to fetch activity events';
      setError(errorMessage);

      logger.error('Failed to fetch activity events', {
        component: 'useActivityEvents',
        operation: 'fetchEvents',
        workspaceId,
        userId,
        tenantId,
      }, err instanceof Error ? err : new Error(String(err)));
    } finally {
      setLoading(false);
    }
  }, [enabled, maxEvents, workspaceId, userId, tenantId, eventType]);

  const createEvent = useCallback(async (data: CreateActivityEventRequest): Promise<ActivityEvent> => {
    try {
      const newEvent = await apiClient.createActivityEvent(data);

      // Add to local state
      setEvents(prev => [newEvent, ...prev.slice(0, maxEvents - 1)]);

      logger.info('Activity event created', {
        component: 'useActivityEvents',
        operation: 'createEvent',
        eventId: newEvent.id,
        eventType: newEvent.event_type,
        workspaceId: newEvent.workspace_id,
      });

      return newEvent;
    } catch (err) {
      logger.error('Failed to create activity event', {
        component: 'useActivityEvents',
        operation: 'createEvent',
        eventType: data.event_type,
      }, toError(err));
      throw err;
    }
  }, [maxEvents]);

  useEffect(() => {
    fetchEvents();

    // Polling for activity updates every 60s (less frequent than notifications)
    const interval = setInterval(fetchEvents, 60000);

    return () => {
      clearInterval(interval);
    };
  }, [fetchEvents]);

  return {
    events,
    loading,
    error,
    createEvent,
    refresh: fetchEvents,
  };
}
