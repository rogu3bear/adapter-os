/**
 * Health Polling Hook
 *
 * Manages health check polling for the login page.
 * Extracted from LoginForm for better separation of concerns.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { apiClient } from '@/api/services';
import type { HealthResponse, SystemHealthResponse, ComponentHealth } from '@/api/api-types';
import { AUTH_DEFAULTS } from '@/auth/constants';

export type BackendStatus = 'checking' | 'ready' | 'issue';

export interface HealthState {
  /** Current backend status */
  backendStatus: BackendStatus;
  /** Basic health response */
  health: HealthResponse | null;
  /** Detailed system health response */
  systemHealth: SystemHealthResponse | null;
  /** Error message if health check failed */
  healthError: string | null;
  /** Whether the system is fully ready (healthy status) */
  isReady: boolean;
  /** Components with issues (status !== 'healthy') */
  issueComponents: Array<{
    name: string;
    status: string;
    message?: string;
  }>;
  /** All components for detailed display */
  allComponents: Record<string, ComponentHealth>;
  /** Last update timestamp */
  lastUpdated: string | null;
}

export interface UseHealthPollingReturn extends HealthState {
  /** Manually trigger a health check */
  refresh: () => Promise<void>;
}

/**
 * Hook for polling backend health status.
 *
 * Automatically polls at different intervals based on status:
 * - Ready: 10s interval
 * - Degraded/Issue: 2.5s interval
 *
 * @returns Health state and refresh function
 */
export function useHealthPolling(): UseHealthPollingReturn {
  const [backendStatus, setBackendStatus] = useState<BackendStatus>('checking');
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [systemHealth, setSystemHealth] = useState<SystemHealthResponse | null>(null);
  const [healthError, setHealthError] = useState<string | null>(null);

  const isMountedRef = useRef(true);
  const abortControllerRef = useRef<AbortController | null>(null);
  const hasFetchedRef = useRef(false);

  // Exponential backoff state for error handling
  const errorCountRef = useRef(0);
  const MAX_ERROR_BACKOFF_MULTIPLIER = 4; // Max 4x the degraded interval

  const fetchHealth = useCallback(async () => {
    // Abort any in-flight request
    abortControllerRef.current?.abort();
    const controller = new AbortController();
    abortControllerRef.current = controller;

    const timeoutId = setTimeout(
      () => controller.abort(),
      AUTH_DEFAULTS.HEALTH_CHECK_TIMEOUT
    );

    try {
      // Fetch basic health
      const healthRes = await apiClient.request<HealthResponse>(
        '/healthz',
        { method: 'GET' },
        false, // skipRetry
        controller.signal
      );

      if (controller.signal.aborted || !isMountedRef.current) {
        return;
      }

      setHealth(healthRes);
      setHealthError(null);

      // Reset error count on successful fetch
      errorCountRef.current = 0;

      // Try to fetch detailed system health (may not be available)
      try {
        const systemRes = await apiClient.request<SystemHealthResponse>(
          '/healthz/all',
          { method: 'GET' },
          false,
          controller.signal
        );
        if (!controller.signal.aborted && isMountedRef.current) {
          setSystemHealth(systemRes);
        }
      } catch {
        // System details may not be available yet; keep previous value
      }

      const status = healthRes.status === 'healthy' ? 'ready' : 'issue';
      setBackendStatus(status);
    } catch (err) {
      if (controller.signal.aborted || !isMountedRef.current) {
        return;
      }

      // Increment error count for backoff (cap at 3 to limit max backoff)
      errorCountRef.current = Math.min(errorCountRef.current + 1, 3);

      setBackendStatus('issue');
      if (err instanceof Error && err.name === 'AbortError') {
        setHealthError('Health check timed out.');
      } else {
        setHealthError('Unable to reach system health.');
      }
    } finally {
      clearTimeout(timeoutId);
      if (abortControllerRef.current === controller) {
        abortControllerRef.current = null;
      }
    }
  }, []);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      isMountedRef.current = false;
      abortControllerRef.current?.abort();
    };
  }, []);

  // Calculate polling interval with exponential backoff on errors
  const getPollingInterval = useCallback((): number => {
    if (backendStatus === 'ready') {
      return AUTH_DEFAULTS.HEALTH_POLL_INTERVAL_READY;
    }

    // Apply exponential backoff when there are errors
    const baseInterval = AUTH_DEFAULTS.HEALTH_POLL_INTERVAL_DEGRADED;
    const backoffMultiplier = Math.pow(2, errorCountRef.current);
    return Math.min(
      baseInterval * backoffMultiplier,
      baseInterval * MAX_ERROR_BACKOFF_MULTIPLIER
    );
  }, [backendStatus]);

  // Initial fetch and polling with dynamic intervals
  useEffect(() => {
    if (!hasFetchedRef.current) {
      hasFetchedRef.current = true;
      fetchHealth();
    }

    let timeoutId: NodeJS.Timeout | null = null;

    const scheduleNextPoll = () => {
      const interval = getPollingInterval();
      timeoutId = setTimeout(async () => {
        await fetchHealth();
        scheduleNextPoll();
      }, interval);
    };

    scheduleNextPoll();

    return () => {
      if (timeoutId) {
        clearTimeout(timeoutId);
      }
      abortControllerRef.current?.abort();
    };
  }, [fetchHealth, getPollingInterval]);

  // Compute derived state
  // Convert SystemHealthResponse.components (array) to Record for easier access
  const componentsArray: ComponentHealth[] = (systemHealth?.components as ComponentHealth[] | undefined) ?? [];
  const allComponents: Record<string, ComponentHealth> = componentsArray.reduce(
    (acc: Record<string, ComponentHealth>, comp: ComponentHealth) => {
      acc[comp.component] = comp;
      return acc;
    },
    {}
  );

  const issueComponents = componentsArray
    .map((comp: ComponentHealth) => ({
      name: comp.component,
      status: comp.status ?? 'unknown',
      message: comp.message,
    }))
    .filter((item: { name: string; status: string; message: string }) => item.status !== 'healthy');

  const systemStatus = health?.status || systemHealth?.status || 'unknown';
  const isReady = backendStatus === 'ready' && systemStatus === 'healthy';

  const lastUpdated = systemHealth?.timestamp
    ? new Date(systemHealth.timestamp).toLocaleTimeString()
    : null;

  return {
    backendStatus,
    health,
    systemHealth,
    healthError,
    isReady,
    issueComponents,
    allComponents,
    lastUpdated,
    refresh: fetchHealth,
  };
}
