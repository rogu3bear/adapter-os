import { useState, useEffect, useCallback } from 'react';
import { logger, toError } from '@/utils/logger';

export interface FeatureDegradationConfig {
  /** Feature identifier */
  featureId: string;
  /** Health check function that returns true if feature is available */
  healthCheck: () => Promise<boolean> | boolean;
  /** Interval in ms to re-check feature availability (default: 30000) */
  checkInterval?: number;
  /** Initial degraded state (default: false) */
  initialDegraded?: boolean;
}

export interface FeatureDegradationState {
  /** Whether feature is currently degraded */
  isDegraded: boolean;
  /** Last health check timestamp */
  lastChecked: Date | null;
  /** Number of consecutive failures */
  failureCount: number;
  /** Manually force degraded state */
  forceDegraded: (degraded: boolean) => void;
  /** Manually trigger health check */
  checkHealth: () => Promise<void>;
}

/**
 * Hook for detecting feature availability and graceful degradation
 * 
 * Monitors feature health and provides degradation state for components
 * to adapt functionality when features are unavailable.
 * 
 * @example
 * ```tsx
 * const { isDegraded, checkHealth } = useFeatureDegradation({
 *   featureId: 'sse-updates',
 *   healthCheck: async () => {
 *     const response = await fetch('/api/health/sse');
 *     return response.ok;
 *   },
 *   checkInterval: 60000
 * });
 * 
 * if (isDegraded) {
 *   return <FallbackComponent />;
 * }
 * ```
 */
export function useFeatureDegradation(
  config: FeatureDegradationConfig
): FeatureDegradationState {
  const {
    featureId,
    healthCheck,
    checkInterval = 30000,
    initialDegraded = false,
  } = config;

  const [isDegraded, setIsDegraded] = useState(initialDegraded);
  const [lastChecked, setLastChecked] = useState<Date | null>(null);
  const [failureCount, setFailureCount] = useState(0);
  const [forcedDegraded, setForcedDegraded] = useState<boolean | null>(null);

  const performHealthCheck = useCallback(async () => {
    try {
      const result = await Promise.resolve(healthCheck());
      
      if (result) {
        setIsDegraded(false);
        setFailureCount(0);
      } else {
        setIsDegraded(true);
        setFailureCount((prev) => prev + 1);
      }
      
      setLastChecked(new Date());
    } catch (err) {
      logger.warn(
        `Feature health check failed: ${featureId}`,
        { component: 'useFeatureDegradation', operation: 'healthCheck', featureId }
      );
      setIsDegraded(true);
      setFailureCount((prev) => prev + 1);
      setLastChecked(new Date());
    }
  }, [healthCheck, featureId]);

  const checkHealth = useCallback(async () => {
    if (forcedDegraded === null) {
      await performHealthCheck();
    }
  }, [forcedDegraded, performHealthCheck]);

  const forceDegraded = useCallback((degraded: boolean) => {
    setForcedDegraded(degraded);
    setIsDegraded(degraded);
  }, []);

  // Initial health check
  useEffect(() => {
    if (forcedDegraded === null) {
      performHealthCheck();
    }
  }, [forcedDegraded, performHealthCheck]);

  // Periodic health checks
  useEffect(() => {
    if (forcedDegraded !== null) {
      return; // Don't auto-check if manually forced
    }

    const interval = setInterval(() => {
      performHealthCheck();
    }, checkInterval);

    return () => clearInterval(interval);
  }, [checkInterval, forcedDegraded, performHealthCheck]);

  return {
    isDegraded,
    lastChecked,
    failureCount,
    forceDegraded,
    checkHealth,
  };
}

/**
 * Helper to create multiple feature degradation monitors
 * 
 * @example
 * ```tsx
 * const features = useFeatureDegradations({
 *   sse: { featureId: 'sse', healthCheck: checkSSE },
 *   websocket: { featureId: 'websocket', healthCheck: checkWS },
 * });
 * 
 * if (features.sse.isDegraded) {
 *   // Fallback to polling
 * }
 * ```
 */
export function useFeatureDegradations<T extends Record<string, Omit<FeatureDegradationConfig, 'featureId'>>>(
  configs: T
): Record<keyof T, FeatureDegradationState> {
  const entries = Object.entries(configs).map(([key, config]) => [
    key,
    useFeatureDegradation({
      ...config,
      featureId: key,
    }),
  ]);

  return Object.fromEntries(entries) as Record<keyof T, FeatureDegradationState>;
}

