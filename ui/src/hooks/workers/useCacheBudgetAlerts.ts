/**
 * Hook for monitoring and displaying cache budget exceeded alerts.
 *
 * Provides toast notifications when model cache budget is exceeded on workers.
 */

import { useCallback, useEffect, useRef } from 'react';
import { toast } from 'sonner';
import type { CacheBudgetAlert } from '@/api/types';
import { logger } from '@/utils/logger';

export interface UseCacheBudgetAlertsOptions {
  /** Whether to show toast notifications (default: true) */
  showToasts?: boolean;
  /** Maximum alerts to keep in memory (default: 50) */
  maxAlerts?: number;
  /** Dedupe window in ms - alerts for same worker within this time are merged (default: 5000) */
  dedupeWindowMs?: number;
}

/**
 * Hook to handle cache budget exceeded alerts.
 *
 * Provides methods to:
 * - Show toast notifications for budget exceeded events
 * - Track recent alerts for display in UI
 * - Deduplicate alerts from the same worker
 *
 * @param options - Configuration options
 * @returns Object with alert handling functions and recent alerts
 */
export function useCacheBudgetAlerts(options: UseCacheBudgetAlertsOptions = {}) {
  const {
    showToasts = true,
    maxAlerts = 50,
    dedupeWindowMs = 5000,
  } = options;

  // Track recent alerts with timestamps for deduplication
  const recentAlertsRef = useRef<Map<string, number>>(new Map());
  const alertsRef = useRef<CacheBudgetAlert[]>([]);

  // Cleanup old dedupe entries periodically
  useEffect(() => {
    const interval = setInterval(() => {
      const now = Date.now();
      const newMap = new Map<string, number>();
      for (const [key, timestamp] of recentAlertsRef.current) {
        if (now - timestamp < dedupeWindowMs * 2) {
          newMap.set(key, timestamp);
        }
      }
      recentAlertsRef.current = newMap;
    }, dedupeWindowMs);

    return () => clearInterval(interval);
  }, [dedupeWindowMs]);

  /**
   * Handle a cache budget exceeded alert.
   * Will deduplicate and optionally show a toast.
   */
  const handleAlert = useCallback((alert: CacheBudgetAlert) => {
    const now = Date.now();
    const dedupeKey = `${alert.worker_id}:${alert.tenant_id}`;

    // Check for duplicate within window
    const lastAlertTime = recentAlertsRef.current.get(dedupeKey);
    if (lastAlertTime && now - lastAlertTime < dedupeWindowMs) {
      logger.debug('Deduped cache budget alert', {
        component: 'useCacheBudgetAlerts',
        workerId: alert.worker_id,
      });
      return;
    }

    // Update dedupe tracker
    recentAlertsRef.current.set(dedupeKey, now);

    // Add to alerts list
    alertsRef.current = [alert, ...alertsRef.current].slice(0, maxAlerts);

    // Log the alert
    logger.warn('Cache budget exceeded', {
      component: 'useCacheBudgetAlerts',
      workerId: alert.worker_id,
      tenantId: alert.tenant_id,
      neededMb: alert.needed_mb,
      maxMb: alert.max_mb,
      pinnedEntries: alert.pinned_entries,
      activeEntries: alert.active_entries,
    });

    // Show toast if enabled
    if (showToasts) {
      const shortfall = alert.needed_mb - alert.freed_mb;
      toast.error('Model Cache Budget Exceeded', {
        description: `Worker ${alert.worker_id.slice(0, 8)}... needs ${alert.needed_mb}MB but max is ${alert.max_mb}MB (shortfall: ${shortfall}MB). ${alert.pinned_entries} pinned, ${alert.active_entries} active entries.`,
        duration: 10000,
        action: {
          label: 'View Details',
          onClick: () => {
            // Navigate to worker details - can be customized
            window.location.href = `/workers/${alert.worker_id}`;
          },
        },
      });
    }
  }, [showToasts, maxAlerts, dedupeWindowMs]);

  /**
   * Create an alert from inference error details.
   * Useful when receiving structured errors from API responses.
   */
  const createAlertFromError = useCallback((
    workerId: string,
    tenantId: string,
    error: {
      needed_mb: number;
      freed_mb: number;
      max_mb: number;
      pinned_count: number;
      active_count: number;
      model_key?: string;
    }
  ) => {
    const alert: CacheBudgetAlert = {
      worker_id: workerId,
      tenant_id: tenantId,
      severity: 'critical',
      needed_mb: error.needed_mb,
      freed_mb: error.freed_mb,
      max_mb: error.max_mb,
      pinned_entries: error.pinned_count,
      active_entries: error.active_count,
      timestamp: new Date().toISOString(),
      model_key: error.model_key,
    };
    handleAlert(alert);
    return alert;
  }, [handleAlert]);

  /**
   * Get recent alerts for display in UI.
   */
  const getRecentAlerts = useCallback(() => {
    return [...alertsRef.current];
  }, []);

  /**
   * Clear all stored alerts.
   */
  const clearAlerts = useCallback(() => {
    alertsRef.current = [];
    recentAlertsRef.current.clear();
  }, []);

  return {
    handleAlert,
    createAlertFromError,
    getRecentAlerts,
    clearAlerts,
  };
}
