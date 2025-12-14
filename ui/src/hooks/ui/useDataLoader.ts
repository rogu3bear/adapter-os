/**
 * useDataLoader - Unified hook for simple data loading with optional polling/SSE
 *
 * This hook consolidates the common data loading pattern found across 43+ components,
 * providing a simple interface that replaces manual state management with a single hook call.
 *
 * Features:
 * - Automatic initial fetch on mount
 * - Loading, error, and data state management
 * - Manual refetch capability
 * - Optional polling support (delegates to useLiveData)
 * - Optional SSE support (delegates to useLiveData)
 * - Stale-while-revalidate pattern
 * - Focus-based refetching
 * - Connection status reporting
 * - Comprehensive error handling and logging
 *
 * @example Simple data loading
 * ```tsx
 * const { data, isLoading, error, refetch } = useDataLoader({
 *   fetchFn: async () => {
 *     const response = await apiClient.get('/v1/adapters');
 *     return response.data;
 *   },
 *   operationName: 'fetchAdapters',
 * });
 *
 * if (isLoading) return <Spinner />;
 * if (error) return <ErrorMessage error={error} />;
 * return <AdapterList adapters={data} onRefresh={refetch} />;
 * ```
 *
 * @example With polling
 * ```tsx
 * const { data, isLoading, connectionStatus } = useDataLoader({
 *   fetchFn: () => apiClient.get('/v1/metrics').then(r => r.data),
 *   pollingSpeed: 'fast', // 2s interval
 *   operationName: 'fetchMetrics',
 * });
 * ```
 *
 * @example With SSE
 * ```tsx
 * const { data, connectionStatus, lastUpdated } = useDataLoader({
 *   fetchFn: () => apiClient.get('/v1/adapters').then(r => r.data),
 *   sseEndpoint: '/v1/stream/adapters',
 *   sseEventType: 'adapters',
 *   operationName: 'streamAdapters',
 * });
 * ```
 *
 * @example With initial data and refetch on focus
 * ```tsx
 * const { data, refetch } = useDataLoader({
 *   fetchFn: fetchUserProfile,
 *   initialData: cachedProfile,
 *   refetchOnFocus: true,
 *   operationName: 'fetchUserProfile',
 * });
 * ```
 *
 * @example Conditional fetching
 * ```tsx
 * const { data, isLoading } = useDataLoader({
 *   fetchFn: () => apiClient.get(`/v1/adapters/${id}`).then(r => r.data),
 *   enabled: !!id, // Only fetch when ID is available
 *   operationName: 'fetchAdapter',
 * });
 * ```
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { useLiveData, PollingSpeed, ConnectionStatus } from '@/hooks/realtime/useLiveData';
import { logger, toError } from '@/utils/logger';

// ============================================================================
// Types
// ============================================================================

export interface UseDataLoaderOptions<T> {
  /** Function to fetch data */
  fetchFn: () => Promise<T>;

  /** Enable/disable fetching (default: true) */
  enabled?: boolean;

  /** Initial data to display before first fetch */
  initialData?: T;

  /** Refetch when window regains focus (default: false) */
  refetchOnFocus?: boolean;

  /** Polling speed (if provided, enables polling via useLiveData) */
  pollingSpeed?: PollingSpeed;

  /** SSE endpoint (if provided, enables SSE via useLiveData) */
  sseEndpoint?: string;

  /** SSE event type to listen for */
  sseEventType?: string;

  /** Operation name for logging and debugging */
  operationName?: string;

  /** Transform SSE data before setting state (only used with sseEndpoint) */
  transformSSE?: (sseData: unknown) => Partial<T>;

  /** Merge strategy for SSE data (default: 'replace') */
  mergeStrategy?: 'replace' | 'merge';

  /** Callback on error */
  onError?: (error: Error) => void;

  /** Callback on successful fetch */
  onSuccess?: (data: T) => void;

  /** Stale time in milliseconds - data older than this is considered stale (default: 60000) */
  staleTime?: number;
}

export interface UseDataLoaderReturn<T> {
  /** Current data */
  data: T | null;

  /** True during initial load */
  isInitialLoading: boolean;

  /** True during any loading operation */
  isLoading: boolean;

  /** True during refetch while data exists */
  isRefreshing: boolean;

  /** Last error that occurred */
  error: Error | null;

  /** Manually trigger a data refetch */
  refetch: () => Promise<void>;

  /** Connection status (relevant when using polling/SSE) */
  connectionStatus: ConnectionStatus;

  /** Timestamp of last successful update */
  lastUpdated: Date | null;

  /** Whether the data is stale based on staleTime */
  isStale: boolean;
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * Unified data loading hook that simplifies the common fetch-on-mount pattern.
 *
 * Automatically handles:
 * - Initial data fetching
 * - Loading/error/data state management
 * - Optional polling (via useLiveData)
 * - Optional SSE (via useLiveData)
 * - Refetch on window focus
 * - Stale-while-revalidate pattern
 */
export function useDataLoader<T>(options: UseDataLoaderOptions<T>): UseDataLoaderReturn<T> {
  const {
    fetchFn,
    enabled = true,
    initialData,
    refetchOnFocus = false,
    pollingSpeed,
    sseEndpoint,
    sseEventType,
    operationName = 'useDataLoader',
    transformSSE,
    mergeStrategy = 'replace',
    onError,
    onSuccess,
    staleTime = 60000, // 60 seconds default
  } = options;

  // ============================================================================
  // Delegation to useLiveData
  // ============================================================================

  // If SSE or polling is requested, delegate to useLiveData
  const shouldUseLiveData = !!sseEndpoint || !!pollingSpeed;

  const liveDataResult = useLiveData<T>({
    fetchFn,
    enabled: enabled && shouldUseLiveData,
    pollingSpeed: pollingSpeed || 'normal',
    sseEndpoint,
    sseEventType,
    transformSSE,
    mergeStrategy,
    operationName,
    onError: onError ? (err) => onError(err) : undefined,
  });

  // ============================================================================
  // Simple State Management (when not using useLiveData)
  // ============================================================================

  const [simpleData, setSimpleData] = useState<T | null>(initialData ?? null);
  const [simpleIsLoading, setSimpleIsLoading] = useState(!initialData);
  const [simpleIsRefreshing, setSimpleIsRefreshing] = useState(false);
  const [simpleError, setSimpleError] = useState<Error | null>(null);
  const [simpleLastUpdated, setSimpleLastUpdated] = useState<Date | null>(
    initialData ? new Date() : null
  );

  const mountedRef = useRef(true);
  const fetchFnRef = useRef(fetchFn);
  const onErrorRef = useRef(onError);
  const onSuccessRef = useRef(onSuccess);
  const initialFetchDoneRef = useRef(false);

  // Update refs on change
  useEffect(() => {
    fetchFnRef.current = fetchFn;
    onErrorRef.current = onError;
    onSuccessRef.current = onSuccess;
  }, [fetchFn, onError, onSuccess]);

  // Simple fetch function
  const simpleFetch = useCallback(
    async (isRefresh = false) => {
      if (!mountedRef.current || !enabled) return;

      try {
        if (!isRefresh) {
          setSimpleIsLoading(true);
        } else {
          setSimpleIsRefreshing(true);
        }
        setSimpleError(null);

        const result = await fetchFnRef.current();

        if (!mountedRef.current) return;

        setSimpleData(result);
        setSimpleLastUpdated(new Date());
        setSimpleError(null);

        onSuccessRef.current?.(result);

        logger.debug('Data fetch successful', {
          component: 'useDataLoader',
          operation: operationName,
          isRefresh,
        });
      } catch (err) {
        if (!mountedRef.current) return;

        const error = toError(err);
        setSimpleError(error);

        logger.error('Data fetch failed', {
          component: 'useDataLoader',
          operation: operationName,
          isRefresh,
        }, error);

        onErrorRef.current?.(error);
      } finally {
        if (mountedRef.current) {
          setSimpleIsLoading(false);
          setSimpleIsRefreshing(false);
        }
      }
    },
    [enabled, operationName]
  );

  // Initial fetch (simple mode only)
  useEffect(() => {
    if (!shouldUseLiveData && enabled && !initialFetchDoneRef.current) {
      initialFetchDoneRef.current = true;
      simpleFetch(false);
    }
  }, [shouldUseLiveData, enabled, simpleFetch]);

  // ============================================================================
  // Focus Refetching
  // ============================================================================

  useEffect(() => {
    if (!refetchOnFocus || !enabled) return;

    const handleFocus = () => {
      if (shouldUseLiveData) {
        liveDataResult.refetch();
      } else {
        simpleFetch(true);
      }
    };

    window.addEventListener('focus', handleFocus);
    return () => window.removeEventListener('focus', handleFocus);
  }, [refetchOnFocus, enabled, shouldUseLiveData, liveDataResult, simpleFetch]);

  // ============================================================================
  // Cleanup
  // ============================================================================

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  // ============================================================================
  // Return Interface
  // ============================================================================

  // If using live data, map its interface to our interface
  if (shouldUseLiveData) {
    const isStale = liveDataResult.lastUpdated
      ? Date.now() - liveDataResult.lastUpdated.getTime() > staleTime
      : true;

    return {
      data: liveDataResult.data,
      isInitialLoading: liveDataResult.isLoading && liveDataResult.data === null,
      isLoading: liveDataResult.isLoading,
      isRefreshing: liveDataResult.isLoading && liveDataResult.data !== null,
      error: liveDataResult.error,
      refetch: liveDataResult.refetch,
      connectionStatus: liveDataResult.connectionStatus,
      lastUpdated: liveDataResult.lastUpdated,
      isStale,
    };
  }

  // Simple mode
  const isStale = simpleLastUpdated
    ? Date.now() - simpleLastUpdated.getTime() > staleTime
    : true;

  return {
    data: simpleData,
    isInitialLoading: simpleIsLoading && simpleData === null,
    isLoading: simpleIsLoading,
    isRefreshing: simpleIsRefreshing,
    error: simpleError,
    refetch: async () => {
      await simpleFetch(true);
    },
    connectionStatus: 'disconnected' as ConnectionStatus,
    lastUpdated: simpleLastUpdated,
    isStale,
  };
}

export default useDataLoader;
