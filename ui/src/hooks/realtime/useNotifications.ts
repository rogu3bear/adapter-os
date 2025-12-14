//! Real-time notifications hook
//!
//! Provides live notification updates with SSE and polling fallback.
//! Now uses the unified useSSEWithPollingFallback hook for transport layer.
//!
//! # Citations
//! - ui/src/hooks/realtime/useActivityFeed.ts L1-L289: Reference implementation using shared hook
//! - ui/src/hooks/realtime/useSSEWithPollingFallback.ts: Unified SSE + polling implementation
//!
//! # Implementation Note
//!
//! This hook was refactored to use the unified `useSSEWithPollingFallback` hook,
//! reducing code from ~388 LOC to ~260 LOC by delegating transport concerns
//! to the shared abstraction while maintaining notification-specific logic.

import { useState, useEffect, useRef, useCallback } from 'react';
import { logger, toError } from '@/utils/logger';
import apiClient from '@/api/client';
import { Notification, NotificationSummary } from '@/api/types';
import { useSSEWithPollingFallback } from '@/hooks/realtime';

export interface UseNotificationsOptions {
  enabled?: boolean;
  maxNotifications?: number;
  workspaceId?: string;
  useSSE?: boolean;
}

export interface UseNotificationsReturn {
  notifications: Notification[];
  summary: NotificationSummary | null;
  isLoading: boolean;
  error: Error | null;
  refetch: () => Promise<void>;
  markRead: (notificationId: string) => Promise<void>;
  markAllRead: () => Promise<void>;
}

interface NotificationData {
  notifications: Notification[];
  summary: NotificationSummary;
}

/**
 * Hook for managing real-time notifications
 *
 * # Arguments
 *
 * * `options` - Configuration options for notifications
 *
 * # Returns
 *
 * * `notifications` - Array of notification objects
 * * `summary` - Summary with unread count
 * * `loading` - Loading state
 * * `error` - Error message if any
 * * `refresh` - Function to manually refresh notifications
 * * `markRead` - Function to mark a notification as read
 * * `markAllRead` - Function to mark all notifications as read
 *
 * # Policy Compliance
 *
 * - Policy Pack #9 (Telemetry): Uses canonical JSON structure
 * - Policy Pack #1 (Egress): Uses relative API paths only
 */
export function useNotifications(options: UseNotificationsOptions = {}): UseNotificationsReturn {
  const { enabled = true, maxNotifications = 50, workspaceId, useSSE = true } = options;

  // Local state for notifications-specific data
  const [notifications, setNotifications] = useState<Notification[]>([]);
  const [summary, setSummary] = useState<NotificationSummary | null>(null);

  // Store latest values in refs to avoid recreating callbacks
  const maxNotificationsRef = useRef(maxNotifications);
  const workspaceIdRef = useRef(workspaceId);

  useEffect(() => {
    maxNotificationsRef.current = maxNotifications;
    workspaceIdRef.current = workspaceId;
  }, [maxNotifications, workspaceId]);

  // ============================================================================
  // Polling Function
  // ============================================================================

  const fetchNotifications = useCallback(async (): Promise<NotificationData> => {
    // Fetch notifications and summary in parallel
    const [notificationsResponse, summaryResponse] = await Promise.all([
      apiClient.listNotifications({
        workspace_id: workspaceIdRef.current,
        unread_only: true,
        limit: maxNotificationsRef.current,
      }),
      apiClient.getNotificationSummary(workspaceIdRef.current),
    ]);

    // Use debug level for routine updates to avoid console spam
    // Only log at info level when there are unread notifications
    if (summaryResponse.unread_count > 0) {
      logger.debug('Notifications updated', {
        component: 'useNotifications',
        operation: 'fetchNotifications',
        notificationCount: notificationsResponse.length,
        unreadCount: summaryResponse.unread_count,
        workspaceId: workspaceIdRef.current,
      });
    }

    return {
      notifications: notificationsResponse,
      summary: summaryResponse,
    };
  }, []);

  // ============================================================================
  // SSE Transform Function
  // ============================================================================

  const transformSSE = useCallback((sseData: unknown): NotificationData => {
    // SSE data from endpoint has format:
    // { notifications: Notification[], count: number, timestamp: string }
    const rawData = sseData as {
      notifications?: Notification[];
      count?: number;
      timestamp?: string;
    };

    if (!rawData.notifications || !Array.isArray(rawData.notifications)) {
      // Return empty data if malformed
      return {
        notifications: [],
        summary: {
          total_count: 0,
          unread_count: 0,
          by_type: {},
        },
      };
    }

    return {
      notifications: rawData.notifications,
      summary: {
        total_count: rawData.notifications.length,
        unread_count: rawData.count ?? 0,
        by_type: {},
      },
    };
  }, []);

  // ============================================================================
  // Use Shared SSE + Polling Hook
  // ============================================================================

  const {
    data: notificationData,
    isLoading,
    error: hookError,
    refetch,
  } = useSSEWithPollingFallback<NotificationData>({
    sseEndpoint: '/v1/stream/notifications',
    sseEventType: 'message', // Default event type for notifications SSE
    pollingFn: fetchNotifications,
    pollingSpeed: 'normal',
    enabled,
    useSSE,
    transformSSE,
    onError: (err, source) => {
      logger.error(`Notifications error from ${source}`, {
        component: 'useNotifications',
        operation: 'realtime',
        source,
        workspaceId: workspaceIdRef.current,
      }, err);
    },
    operationName: 'useNotifications',
    baselinePollingIntervalMs: 30000, // Baseline polling every 30s
    sseInitialBackoffMs: 500,
    sseMaxReconnectAttempts: 5,
  });

  // Update local state when data changes
  useEffect(() => {
    if (notificationData) {
      setNotifications(notificationData.notifications);
      setSummary(notificationData.summary);
    }
  }, [notificationData]);

  // ============================================================================
  // Notification-Specific Actions
  // ============================================================================

  const markRead = useCallback(async (notificationId: string) => {
    try {
      await apiClient.markNotificationRead(notificationId);

      // Update local state optimistically
      setNotifications(prev =>
        prev.map(n => n.id === notificationId ? { ...n, read_at: new Date().toISOString() } : n)
      );

      // Update summary
      setSummary(prev => prev ? { ...prev, unread_count: Math.max(0, prev.unread_count - 1) } : null);

      // Dispatch storage event for cross-tab sync
      const payload = { refresh: true, timestamp: new Date().toISOString() };
      try {
        localStorage.setItem('aos_notifications', JSON.stringify(payload));
        window.dispatchEvent(new StorageEvent('storage', {
          key: 'aos_notifications',
          newValue: JSON.stringify(payload),
        }));
      } catch (storageErr) {
        logger.error('Failed to emit notification storage event', {
          component: 'useNotifications',
          operation: 'markRead',
          notificationId,
        }, toError(storageErr));
      }

      logger.info('Notification marked as read', {
        component: 'useNotifications',
        operation: 'markRead',
        notificationId,
      });
    } catch (err) {
      logger.error('Failed to mark notification as read', {
        component: 'useNotifications',
        operation: 'markRead',
        notificationId,
      }, toError(err));
      throw err;
    }
  }, []);

  const markAllRead = useCallback(async () => {
    try {
      const result = await apiClient.markAllNotificationsRead(workspaceId);

      // Update local state optimistically
      setNotifications(prev => prev.map(n => ({ ...n, read_at: new Date().toISOString() })));
      setSummary(prev => prev ? { ...prev, unread_count: 0 } : null);

      // Dispatch storage event for cross-tab sync
      const payload = { refresh: true, timestamp: new Date().toISOString() };
      try {
        localStorage.setItem('aos_notifications', JSON.stringify(payload));
        window.dispatchEvent(new StorageEvent('storage', {
          key: 'aos_notifications',
          newValue: JSON.stringify(payload),
        }));
      } catch (storageErr) {
        logger.error('Failed to emit notification storage event', {
          component: 'useNotifications',
          operation: 'markAllRead',
          workspaceId,
        }, toError(storageErr));
      }

      logger.info('All notifications marked as read', {
        component: 'useNotifications',
        operation: 'markAllRead',
        count: result.count,
        workspaceId,
      });
    } catch (err) {
      logger.error('Failed to mark all notifications as read', {
        component: 'useNotifications',
        operation: 'markAllRead',
        workspaceId,
      }, toError(err));
      throw err;
    }
  }, [workspaceId]);

  // ============================================================================
  // Cross-Tab Sync via Storage Events
  // ============================================================================

  useEffect(() => {
    const handleStorageChange = (e: StorageEvent) => {
      if (e.key === 'aos_notifications' && e.newValue) {
        try {
          // Trigger refresh when storage event is received from another tab
          refetch();
        } catch (err) {
          logger.error('Failed to handle notification storage event', {
            component: 'useNotifications',
            operation: 'storage_listener',
          }, toError(err));
        }
      }
    };

    window.addEventListener('storage', handleStorageChange);
    return () => {
      window.removeEventListener('storage', handleStorageChange);
    };
  }, [refetch]);

  return {
    notifications,
    summary,
    isLoading,
    error: hookError,
    refetch,
    markRead,
    markAllRead,
  };
}
