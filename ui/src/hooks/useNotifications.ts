//! Real-time notifications hook
//!
//! Provides live notification updates with SSE and polling fallback.
//! Follows same pattern as useActivityFeed for consistency.
//!
//! Citation: ui/src/hooks/useActivityFeed.ts L61-L290
//! - SSE connection with EventSource and reconnect logic
//! - Fallback polling with 30s baseline, 500ms when disconnected
//! - Exponential backoff for reconnect attempts
//! - Structured logging via logger utility

import { useState, useEffect, useRef, useCallback } from 'react';
import { logger, toError } from '@/utils/logger';
import apiClient from '@/api/client';
import { Notification, NotificationSummary } from '@/api/types';

export interface UseNotificationsOptions {
  enabled?: boolean;
  maxNotifications?: number;
  workspaceId?: string;
  useSSE?: boolean;
}

export interface UseNotificationsReturn {
  notifications: Notification[];
  summary: NotificationSummary | null;
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  markRead: (notificationId: string) => Promise<void>;
  markAllRead: () => Promise<void>;
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

  const [notifications, setNotifications] = useState<Notification[]>([]);
  const [summary, setSummary] = useState<NotificationSummary | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const sseRef = useRef<(() => void) | null>(null);
  const fallbackIntervalRef = useRef<NodeJS.Timeout | null>(null);
  const baselineIntervalRef = useRef<NodeJS.Timeout | null>(null);
  const isMountedRef = useRef(true);
  
  // Store latest values in refs to avoid recreating callbacks
  const enabledRef = useRef(enabled);
  const maxNotificationsRef = useRef(maxNotifications);
  const workspaceIdRef = useRef(workspaceId);
  
  useEffect(() => {
    enabledRef.current = enabled;
    maxNotificationsRef.current = maxNotifications;
    workspaceIdRef.current = workspaceId;
  }, [enabled, maxNotifications, workspaceId]);

  const fetchNotifications = useCallback(async () => {
    if (!enabledRef.current || !isMountedRef.current) return;

    setLoading(true);
    setError(null);

    try {
      // Fetch notifications and summary in parallel
      const [notificationsResponse, summaryResponse] = await Promise.all([
        apiClient.listNotifications({
          workspace_id: workspaceIdRef.current,
          unread_only: true,
          limit: maxNotificationsRef.current,
        }),
        apiClient.getNotificationSummary(workspaceIdRef.current),
      ]);

      if (!isMountedRef.current) return;

      setNotifications(notificationsResponse);
      setSummary(summaryResponse);

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
    } catch (err) {
      if (!isMountedRef.current) return;
      
      const errorMessage = err instanceof Error ? err.message : 'Failed to fetch notifications';
      setError(errorMessage);

      logger.error('Failed to fetch notifications', {
        component: 'useNotifications',
        operation: 'fetchNotifications',
        workspaceId: workspaceIdRef.current,
      }, err instanceof Error ? err : new Error(String(err)));
    } finally {
      if (isMountedRef.current) {
        setLoading(false);
      }
    }
  }, []); // Empty deps - use refs for values

  const markRead = useCallback(async (notificationId: string) => {
    try {
      await apiClient.markNotificationRead(notificationId);

      // Update local state
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

      // Update local state
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

  // Listen for storage events from other tabs
  useEffect(() => {
    const handleStorageChange = (e: StorageEvent) => {
      if (e.key === 'aos_notifications' && e.newValue) {
        try {
          // Trigger refresh when storage event is received
          fetchNotifications();
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
  }, [fetchNotifications]);

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
      if (sseRef.current) {
        sseRef.current();
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
    if (sseRef.current) {
      sseRef.current();
      sseRef.current = null;
    }

    fetchNotifications();

    // Baseline polling every 30s
    baselineIntervalRef.current = setInterval(() => {
      if (isMountedRef.current && enabledRef.current) {
        fetchNotifications();
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
          // Properly handle async errors instead of fire-and-forget
          fetchNotifications().catch((error) => {
            logger.warn('Fallback polling failed', {
              component: 'useNotifications',
              error: error instanceof Error ? error.message : String(error),
            });
          });
        }
      }, 500);
    }

    function stopSSE() {
      if (sseRef.current) {
        try {
          sseRef.current();
        } catch (e) {
          // Ignore cleanup errors
        }
        sseRef.current = null;
      }
    }

    function connectSSE() {
      // Always cleanup previous connection first
      if (sseRef.current) {
        try {
          sseRef.current();
        } catch {}
        sseRef.current = null;
      }

      if (!useSSE || !isMountedRef.current) return;

      try {
        const unsubscribe = apiClient.subscribeToNotifications((data) => {
          if (!isMountedRef.current) {
            // Component unmounted, cleanup
            if (unsubscribe) unsubscribe();
            return;
          }
          
          if (data) {
            setNotifications(data.notifications);
            setSummary({
              total_count: data.notifications.length,
              unread_count: data.count,
              by_type: {},
            });
            setError(null);
            reconnectAttempts = 0;
            clearFallback();
          } else {
            startFallbackPolling();
          }
        });

        sseRef.current = unsubscribe;
      } catch (err) {
        logger.error('Failed to initialize notifications SSE', {
          component: 'useNotifications',
          operation: 'sse_init',
          workspaceId: workspaceIdRef.current,
        }, err instanceof Error ? err : new Error(String(err)));
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
      stopSSE();
    };
  }, [enabled, workspaceId, useSSE, fetchNotifications]);

  return {
    notifications,
    summary,
    loading,
    error,
    refresh: fetchNotifications,
    markRead,
    markAllRead,
  };
}
