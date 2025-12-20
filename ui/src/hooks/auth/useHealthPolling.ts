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

  // Initial fetch and polling
  useEffect(() => {
    if (!hasFetchedRef.current) {
      hasFetchedRef.current = true;
      fetchHealth();
    }

    const interval = setInterval(
      fetchHealth,
      backendStatus === 'ready'
        ? AUTH_DEFAULTS.HEALTH_POLL_INTERVAL_READY
        : AUTH_DEFAULTS.HEALTH_POLL_INTERVAL_DEGRADED
    );

    return () => {
      clearInterval(interval);
      abortControllerRef.current?.abort();
    };
  }, [fetchHealth, backendStatus]);

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
