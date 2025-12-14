// 【ui/src/hooks/useActivityFeed.ts§172-294】 - SSE + polling pattern
// 【ui/src/components/RealtimeMetrics.tsx§138-182】 - Metrics polling pattern
// 【ui/src/utils/logger.ts】 - Error handling pattern
import { useState, useEffect, useCallback, useRef } from 'react';
import { logger, toError } from '@/utils/logger';

export interface PollingConfig {
  intervalMs?: number; // Override default interval
  enabled?: boolean;
  showLoadingIndicator?: boolean;
  onError?: (error: Error) => void;
  onSuccess?: (data: unknown) => void;
  operationName?: string; // Name of the operation for error logging
  enableCircuitBreaker?: boolean; // Enable circuit breaker pattern (default: true)
  circuitBreakerThreshold?: number; // Number of consecutive failures before opening circuit (default: 5)
  circuitBreakerResetMs?: number; // Time to wait before retrying after circuit opens (default: 30000)
  maxBackoffMultiplier?: number; // Maximum backoff multiplier (default: 5)
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
  isFetching: boolean;
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
    onSuccess,
    operationName = 'unknown',
    enableCircuitBreaker = true,
    circuitBreakerThreshold = 5,
    circuitBreakerResetMs = 30000,
    maxBackoffMultiplier = 5
  } = config || {};
  
  const [data, setData] = useState<T | null>(null);
  // Only start in loading state if we intend to show a loading indicator
  const [isLoading, setIsLoading] = useState(showLoadingIndicator);
  const [isFetching, setIsFetching] = useState(false);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [error, setError] = useState<Error | null>(null);
  
  // Circuit breaker state
  const failureCountRef = useRef(0);
  const lastSuccessRef = useRef<Date | null>(null);
  const circuitOpenTimeRef = useRef<Date | null>(null);
  const currentIntervalRef = useRef(intervalMs);
  const backoffMultiplierRef = useRef(1);
  
  const intervalRef = useRef<NodeJS.Timeout | null>(null);
  const circuitBreakerTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const mountedRef = useRef(true);
  
  // Store latest values in refs to avoid recreating callbacks
  const fetchFnRef = useRef(fetchFn);
  const showLoadingIndicatorRef = useRef(showLoadingIndicator);
  const onErrorRef = useRef(onError);
  const onSuccessRef = useRef(onSuccess);
  const configRef = useRef(config);
  
  useEffect(() => {
    fetchFnRef.current = fetchFn;
    showLoadingIndicatorRef.current = showLoadingIndicator;
    onErrorRef.current = onError;
    onSuccessRef.current = onSuccess;
    configRef.current = config;
  }, [fetchFn, showLoadingIndicator, onError, onSuccess, config]);

  // Schedule next poll based on current interval (with backoff)
  // Use refs to avoid circular dependency with fetchData
  const scheduleNextPollRef = useRef<(() => void) | null>(null);

  const fetchData = useCallback(async () => {
    if (!mountedRef.current) return;
    
    // Check circuit breaker
    if (enableCircuitBreaker && circuitOpenTimeRef.current) {
      const timeSinceOpen = Date.now() - circuitOpenTimeRef.current.getTime();
      if (timeSinceOpen < circuitBreakerResetMs) {
        // Circuit is still open, skip this poll
        return;
      } else {
        // Circuit breaker reset period elapsed, try again
        circuitOpenTimeRef.current = null;
        failureCountRef.current = 0;
        backoffMultiplierRef.current = 1;
        currentIntervalRef.current = intervalMs;
        logger.info('Circuit breaker reset - resuming polling', {
          component: 'usePolling',
          operation: operationName,
        });
      }
    }
    
    try {
      if (showLoadingIndicatorRef.current) setIsLoading(true);
      setIsFetching(true);
      const result = await fetchFnRef.current();
      
      if (!mountedRef.current) return;
      
      // Success - reset failure tracking
      setData(result);
      setLastUpdated(new Date());
      setError(null);
      lastSuccessRef.current = new Date();
      failureCountRef.current = 0;
      backoffMultiplierRef.current = 1;
      currentIntervalRef.current = intervalMs;
      circuitOpenTimeRef.current = null;
      
      // Clear any circuit breaker timeout
      if (circuitBreakerTimeoutRef.current) {
        clearTimeout(circuitBreakerTimeoutRef.current);
        circuitBreakerTimeoutRef.current = null;
      }
      
      onSuccessRef.current?.(result);
      
      // Schedule next poll with reset interval
      scheduleNextPollRef.current?.();
    } catch (err) {
      if (!mountedRef.current) return;
      
      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);
      
      // Increment failure count
      failureCountRef.current += 1;
      
      // Apply exponential backoff
      const newMultiplier = Math.min(backoffMultiplierRef.current * 2, maxBackoffMultiplier);
      backoffMultiplierRef.current = newMultiplier;
      currentIntervalRef.current = intervalMs * newMultiplier;
      
      // Extract endpoint information if available
      const endpoint = (error.message || '').includes('/') 
        ? error.message.match(/\/[\/a-zA-Z0-9_\-?=&]+/)?.[0] || 'unknown'
        : 'unknown';
      
      // Log error with enhanced context
      logger.error('Polling operation failed', {
        component: 'usePolling',
        operation: operationName,
        endpoint: endpoint,
        failureCount: failureCountRef.current,
        backoffMultiplier: backoffMultiplierRef.current,
        currentIntervalMs: currentIntervalRef.current,
        lastSuccess: lastSuccessRef.current?.toISOString() || null,
        circuitBreakerEnabled: enableCircuitBreaker,
        circuitBreakerThreshold: circuitBreakerThreshold,
      }, toError(err));
      
      // Check if circuit breaker should open
      if (enableCircuitBreaker && failureCountRef.current >= circuitBreakerThreshold) {
        circuitOpenTimeRef.current = new Date();
        logger.warn('Circuit breaker opened - pausing polling', {
          component: 'usePolling',
          operation: operationName,
          failureCount: failureCountRef.current,
          resetAfterMs: circuitBreakerResetMs,
        });
        
        // Clear current polling interval
        if (intervalRef.current) {
          clearTimeout(intervalRef.current);
          intervalRef.current = null;
        }
        
        // Schedule circuit breaker reset
        circuitBreakerTimeoutRef.current = setTimeout(() => {
          if (mountedRef.current) {
            circuitOpenTimeRef.current = null;
            failureCountRef.current = 0;
            backoffMultiplierRef.current = 1;
            currentIntervalRef.current = intervalMs;
            logger.info('Circuit breaker reset - resuming polling', {
              component: 'usePolling',
              operation: operationName,
            });
            // Restart polling - check mountedRef again before scheduling
            if (mountedRef.current) {
              scheduleNextPollRef.current?.();
            }
          }
        }, circuitBreakerResetMs);
      } else {
        // Schedule next poll with backoff interval
        scheduleNextPollRef.current?.();
      }
      
      onErrorRef.current?.(error);
    } finally {
      if (mountedRef.current) {
        if (showLoadingIndicatorRef.current) {
          setIsLoading(false);
        }
        setIsFetching(false);
      }
    }
  }, [operationName, intervalMs, enableCircuitBreaker, circuitBreakerThreshold, circuitBreakerResetMs, maxBackoffMultiplier]); // scheduleNextPoll accessed via ref to avoid circular deps

  const refetch = useCallback(async () => {
    await fetchData();
  }, [fetchData]);

  useEffect(() => {
    mountedRef.current = true;
    
    // Reset circuit breaker state when enabled changes
    if (enabled) {
      failureCountRef.current = 0;
      backoffMultiplierRef.current = 1;
      currentIntervalRef.current = intervalMs;
      circuitOpenTimeRef.current = null;
      if (circuitBreakerTimeoutRef.current) {
        clearTimeout(circuitBreakerTimeoutRef.current);
        circuitBreakerTimeoutRef.current = null;
      }
    }
    
    if (!enabled) {
      setIsLoading(false);
      // Clean up any existing interval
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
      if (circuitBreakerTimeoutRef.current) {
        clearTimeout(circuitBreakerTimeoutRef.current);
        circuitBreakerTimeoutRef.current = null;
      }
      return;
    }

    // Clean up any existing interval first
    if (intervalRef.current) {
      clearTimeout(intervalRef.current);
      intervalRef.current = null;
    }

    // Schedule next poll function (uses refs to avoid circular dependencies)
    const scheduleNextPoll = () => {
      if (!mountedRef.current || !enabled) return;
      
      // Check circuit breaker before scheduling
      if (enableCircuitBreaker && circuitOpenTimeRef.current) {
        const timeSinceOpen = Date.now() - circuitOpenTimeRef.current.getTime();
        if (timeSinceOpen < circuitBreakerResetMs) {
          // Circuit is still open, don't schedule
          return;
        }
      }
      
      // Clear any existing timeout
      if (intervalRef.current) {
        clearTimeout(intervalRef.current);
        intervalRef.current = null;
      }
      
      // Schedule next poll with current interval (includes backoff)
      intervalRef.current = setTimeout(() => {
        if (mountedRef.current && enabled) {
          fetchData();
        }
      }, currentIntervalRef.current);
    };

    // Store scheduleNextPoll in ref so fetchData can call it
    scheduleNextPollRef.current = scheduleNextPoll;

    // Initial fetch - this will schedule the next poll
    fetchData();

    return () => {
      mountedRef.current = false;
      if (intervalRef.current) {
        clearTimeout(intervalRef.current);
        intervalRef.current = null;
      }
      if (circuitBreakerTimeoutRef.current) {
        clearTimeout(circuitBreakerTimeoutRef.current);
        circuitBreakerTimeoutRef.current = null;
      }
    };
  }, [fetchData, intervalMs, enabled, enableCircuitBreaker, circuitBreakerResetMs]);

  return { data, isLoading, isFetching, lastUpdated, error, refetch };
}

