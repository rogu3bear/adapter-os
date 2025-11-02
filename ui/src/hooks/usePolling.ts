// 【ui/src/hooks/useActivityFeed.ts§172-294】 - SSE + polling pattern
// 【ui/src/components/RealtimeMetrics.tsx§138-182】 - Metrics polling pattern
// 【ui/src/utils/logger.ts】 - Error handling pattern
import { useState, useEffect, useCallback, useRef } from 'react';
import { logger, toError } from '../utils/logger';

export interface PollingConfig {
  intervalMs?: number; // Override default interval
  enabled?: boolean;
  showLoadingIndicator?: boolean;
  onError?: (error: Error) => void;
  onSuccess?: (data: unknown) => void;
}

export type PollingSpeed = 'fast' | 'normal' | 'slow';

const POLLING_INTERVALS: Record<PollingSpeed, number> = {
  fast: 2000,    // Real-time updates (alerts, training progress)
  normal: 5000,  // Standard updates (metrics, dashboard)
  slow: 30000    // Background updates (system health, admin)
};

export interface UsePollingReturn<T> {
  data: T | null;
  isLoading: boolean;
  lastUpdated: Date | null;
  error: Error | null;
  refetch: () => Promise<void>;
}

export function usePolling<T>(
  fetchFn: () => Promise<T>,
  speed: PollingSpeed = 'normal',
  config?: PollingConfig
): UsePollingReturn<T> {
  const { 
    intervalMs = POLLING_INTERVALS[speed], 
    enabled = true, 
    showLoadingIndicator = false, 
    onError,
    onSuccess
  } = config || {};
  
  const [data, setData] = useState<T | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const intervalRef = useRef<NodeJS.Timeout | null>(null);
  const mountedRef = useRef(true);
  
  // Store latest values in refs to avoid recreating callbacks
  const fetchFnRef = useRef(fetchFn);
  const showLoadingIndicatorRef = useRef(showLoadingIndicator);
  const onErrorRef = useRef(onError);
  const onSuccessRef = useRef(onSuccess);
  
  useEffect(() => {
    fetchFnRef.current = fetchFn;
    showLoadingIndicatorRef.current = showLoadingIndicator;
    onErrorRef.current = onError;
    onSuccessRef.current = onSuccess;
  }, [fetchFn, showLoadingIndicator, onError, onSuccess]);

  const fetchData = useCallback(async () => {
    if (!mountedRef.current) return;
    
    try {
      if (showLoadingIndicatorRef.current) setIsLoading(true);
      const result = await fetchFnRef.current();
      
      if (!mountedRef.current) return;
      
      setData(result);
      setLastUpdated(new Date());
      setError(null);
      onSuccessRef.current?.(result);
    } catch (err) {
      if (!mountedRef.current) return;
      
      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);
      
      // Log error before calling onError callback
      logger.error('Polling operation failed', {
        component: 'usePolling',
        operation: 'fetchData',
        intervalMs: intervalMs,
        speed,
      }, toError(err));
      
      onErrorRef.current?.(error);
    } finally {
      if (mountedRef.current && showLoadingIndicatorRef.current) {
        setIsLoading(false);
      }
    }
  }, []); // Empty deps - use refs for values

  const refetch = useCallback(async () => {
    await fetchData();
  }, [fetchData]);

  useEffect(() => {
    mountedRef.current = true;
    
    if (!enabled) {
      setIsLoading(false);
      // Clean up any existing interval
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
      return;
    }

    // Clean up any existing interval first
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
      intervalRef.current = null;
    }

    // Initial fetch
    fetchData();

    // Set up polling interval
    intervalRef.current = setInterval(() => {
      if (mountedRef.current && enabled) {
        fetchData();
      }
    }, intervalMs);

    return () => {
      mountedRef.current = false;
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [fetchData, intervalMs, enabled]);

  return { data, isLoading, lastUpdated, error, refetch };
}

