// 【ui/src/hooks/useActivityFeed.ts§172-294】 - SSE + polling pattern
// 【ui/src/components/RealtimeMetrics.tsx§138-182】 - Metrics polling pattern
// 【ui/src/utils/logger.ts】 - Error handling pattern
import { useState, useEffect, useCallback, useRef } from 'react';

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

  const fetchData = useCallback(async () => {
    if (!mountedRef.current) return;
    
    try {
      if (showLoadingIndicator) setIsLoading(true);
      const result = await fetchFn();
      
      if (!mountedRef.current) return;
      
      setData(result);
      setLastUpdated(new Date());
      setError(null);
      onSuccess?.(result);
    } catch (err) {
      if (!mountedRef.current) return;
      
      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);
      onError?.(error);
    } finally {
      if (mountedRef.current && showLoadingIndicator) {
        setIsLoading(false);
      }
    }
  }, [fetchFn, showLoadingIndicator, onError, onSuccess]);

  const refetch = useCallback(async () => {
    await fetchData();
  }, [fetchData]);

  useEffect(() => {
    mountedRef.current = true;
    
    if (!enabled) {
      setIsLoading(false);
      return;
    }

    // Initial fetch
    fetchData();

    // Set up polling interval
    intervalRef.current = setInterval(fetchData, intervalMs);

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

