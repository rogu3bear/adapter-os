//! Infinite Scroll Hook for Paginated Data
//!
//! Provides infinite scroll pagination with React Query integration.
//! Supports both cursor-based and offset-based pagination.
//!
//! # Usage
//! ```tsx
//! const {
//!   data,
//!   isLoading,
//!   hasNextPage,
//!   fetchNextPage,
//!   sentinelRef
//! } = useInfiniteScroll({
//!   queryKey: ['adapters'],
//!   queryFn: ({ pageParam }) => api.getAdapters({ cursor: pageParam }),
//!   getNextPageParam: (lastPage) => lastPage.nextCursor,
//! });
//!
//! return (
//!   <div>
//!     {data.map(item => <Item key={item.id} {...item} />)}
//!     <div ref={sentinelRef} />
//!   </div>
//! );
//! ```

import { useRef, useCallback, useEffect, useMemo, useState } from 'react';
import {
  useInfiniteQuery,
  UseInfiniteQueryOptions,
  QueryKey,
  InfiniteData,
} from '@tanstack/react-query';
import { logger } from '@/utils/logger';

export interface PageParam {
  /** Cursor for cursor-based pagination */
  cursor?: string;
  /** Offset for offset-based pagination */
  offset?: number;
  /** Page number for page-based pagination */
  page?: number;
  /** Limit per page */
  limit?: number;
}

export interface InfiniteScrollPage<TItem> {
  /** Items in this page */
  items: TItem[];
  /** Next cursor for cursor-based pagination */
  nextCursor?: string | null;
  /** Total count of items (if known) */
  totalCount?: number;
  /** Whether there are more pages */
  hasMore?: boolean;
}

export interface UseInfiniteScrollOptions<TItem, TPageParam = PageParam> {
  /** React Query key */
  queryKey: QueryKey;
  /** Function to fetch a page of data */
  queryFn: (context: { pageParam: TPageParam }) => Promise<InfiniteScrollPage<TItem>>;
  /** Function to get the next page parameter */
  getNextPageParam: (lastPage: InfiniteScrollPage<TItem>, allPages: InfiniteScrollPage<TItem>[]) => TPageParam | undefined;
  /** Function to get the previous page parameter (for bidirectional scrolling) */
  getPreviousPageParam?: (firstPage: InfiniteScrollPage<TItem>, allPages: InfiniteScrollPage<TItem>[]) => TPageParam | undefined;
  /** Initial page parameter */
  initialPageParam?: TPageParam;
  /** Number of items per page */
  pageSize?: number;
  /** Whether to enable the query */
  enabled?: boolean;
  /** Stale time in milliseconds */
  staleTime?: number;
  /** Intersection observer options */
  observerOptions?: IntersectionObserverInit;
  /** Threshold for triggering fetch (0-1, distance from bottom) */
  threshold?: number;
  /**
   * Callback when fetch starts
   *
   * **IMPORTANT**: This callback is included in dependency arrays. Callers MUST memoize this
   * callback using `useCallback` to avoid infinite re-renders and unnecessary effect re-runs.
   */
  onFetchStart?: () => void;
  /**
   * Callback when fetch completes
   *
   * **IMPORTANT**: This callback is included in dependency arrays. Callers MUST memoize this
   * callback using `useCallback` to avoid infinite re-renders and unnecessary effect re-runs.
   */
  onFetchComplete?: (page: InfiniteScrollPage<TItem>) => void;
  /**
   * Callback on error
   *
   * **IMPORTANT**: This callback is included in dependency arrays. Callers MUST memoize this
   * callback using `useCallback` to avoid infinite re-renders and unnecessary effect re-runs.
   */
  onError?: (error: Error) => void;
  /** Component name for logging */
  componentName?: string;
  /** Enable automatic fetching when sentinel is visible */
  autoFetch?: boolean;
  /** Refetch interval in milliseconds (0 to disable) */
  refetchInterval?: number;
}

export interface UseInfiniteScrollReturn<TItem> {
  /** All loaded items flattened */
  data: TItem[];
  /** All pages of data */
  pages: InfiniteScrollPage<TItem>[];
  /** Whether initial data is loading */
  isLoading: boolean;
  /** Whether more data is being fetched */
  isFetchingNextPage: boolean;
  /** Whether previous page is being fetched */
  isFetchingPreviousPage: boolean;
  /** Whether any fetch is in progress */
  isFetching: boolean;
  /** Whether there is a next page */
  hasNextPage: boolean;
  /** Whether there is a previous page */
  hasPreviousPage: boolean;
  /** Total count of items (if known) */
  totalCount: number | undefined;
  /** Current loaded count */
  loadedCount: number;
  /** Error from the query */
  error: Error | null;
  /** Fetch the next page */
  fetchNextPage: () => Promise<void>;
  /** Fetch the previous page */
  fetchPreviousPage: () => Promise<void>;
  /** Refetch all pages */
  refetch: () => Promise<void>;
  /** Ref to attach to sentinel element for auto-fetching */
  sentinelRef: (node: HTMLElement | null) => void;
  /** Reset and start from the beginning */
  reset: () => void;
}

/**
 * Hook for infinite scroll pagination with React Query integration.
 * Provides automatic fetching when a sentinel element becomes visible.
 *
 * @param options - Configuration options
 * @returns Infinite scroll state and controls
 */
export function useInfiniteScroll<TItem, TPageParam = PageParam>(
  options: UseInfiniteScrollOptions<TItem, TPageParam>
): UseInfiniteScrollReturn<TItem> {
  const {
    queryKey,
    queryFn,
    getNextPageParam,
    getPreviousPageParam,
    initialPageParam = {} as TPageParam,
    pageSize = 20,
    enabled = true,
    staleTime = 5 * 60 * 1000, // 5 minutes
    observerOptions,
    threshold = 0,
    onFetchStart,
    onFetchComplete,
    onError,
    componentName = 'useInfiniteScroll',
    autoFetch = true,
    refetchInterval = 0,
  } = options;

  const observerRef = useRef<IntersectionObserver | null>(null);
  const sentinelNodeRef = useRef<HTMLElement | null>(null);
  const [isObserving, setIsObserving] = useState(false);

  // Wrap queryFn to include pageSize
  const wrappedQueryFn = useCallback(
    async ({ pageParam }: { pageParam: TPageParam }) => {
      if (onFetchStart) {
        onFetchStart();
      }

      const paramWithSize = {
        ...pageParam,
        limit: pageSize,
      } as TPageParam;

      const result = await queryFn({ pageParam: paramWithSize });

      if (onFetchComplete) {
        onFetchComplete(result);
      }

      return result;
    },
    [queryFn, pageSize, onFetchStart, onFetchComplete]
  );

  const queryOptions = useMemo(() => ({
    queryKey,
    queryFn: wrappedQueryFn,
    getNextPageParam: (lastPage: InfiniteScrollPage<TItem>, allPages: InfiniteScrollPage<TItem>[]) => {
      const param = getNextPageParam(lastPage, allPages);
      return param;
    },
    getPreviousPageParam: getPreviousPageParam
      ? (firstPage: InfiniteScrollPage<TItem>, allPages: InfiniteScrollPage<TItem>[]) => getPreviousPageParam(firstPage, allPages)
      : undefined,
    initialPageParam,
    enabled,
    staleTime,
    refetchInterval: refetchInterval > 0 ? refetchInterval : undefined,
  } as UseInfiniteQueryOptions<
    InfiniteScrollPage<TItem>,
    Error,
    InfiniteData<InfiniteScrollPage<TItem>>,
    QueryKey,
    TPageParam
  >), [
    queryKey,
    wrappedQueryFn,
    getNextPageParam,
    getPreviousPageParam,
    initialPageParam,
    enabled,
    staleTime,
    refetchInterval,
  ]);

  const {
    data,
    isLoading,
    isFetchingNextPage,
    isFetchingPreviousPage,
    isFetching,
    hasNextPage,
    hasPreviousPage,
    error,
    fetchNextPage: fetchNextPageRQ,
    fetchPreviousPage: fetchPreviousPageRQ,
    refetch: refetchRQ,
  } = useInfiniteQuery(queryOptions);

  // Flatten pages into a single array
  const flattenedData = useMemo(() => {
    if (!data?.pages) return [];
    return data.pages.flatMap(page => page.items);
  }, [data?.pages]);

  const pages = useMemo(() => data?.pages ?? [], [data?.pages]);

  const totalCount = useMemo(() => {
    const lastPage = pages[pages.length - 1];
    return lastPage?.totalCount;
  }, [pages]);

  const loadedCount = flattenedData.length;

  // Fetch handlers with error handling
  const fetchNextPage = useCallback(async () => {
    if (!hasNextPage || isFetchingNextPage) return;

    try {
      await fetchNextPageRQ();
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));
      logger.error('Failed to fetch next page', {
        component: componentName,
        operation: 'fetchNextPage',
      }, error);
      if (onError) {
        onError(error);
      }
    }
  }, [hasNextPage, isFetchingNextPage, fetchNextPageRQ, componentName, onError]);

  const fetchPreviousPage = useCallback(async () => {
    if (!hasPreviousPage || isFetchingPreviousPage) return;

    try {
      await fetchPreviousPageRQ();
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));
      logger.error('Failed to fetch previous page', {
        component: componentName,
        operation: 'fetchPreviousPage',
      }, error);
      if (onError) {
        onError(error);
      }
    }
  }, [hasPreviousPage, isFetchingPreviousPage, fetchPreviousPageRQ, componentName, onError]);

  const refetch = useCallback(async () => {
    try {
      await refetchRQ();
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));
      logger.error('Failed to refetch', {
        component: componentName,
        operation: 'refetch',
      }, error);
      if (onError) {
        onError(error);
      }
    }
  }, [refetchRQ, componentName, onError]);

  const reset = useCallback(() => {
    // React Query will handle resetting when the query key changes
    // For manual reset, we can refetch which will reset to first page
    refetch();
  }, [refetch]);

  // Intersection Observer for auto-fetching
  const handleIntersection = useCallback(
    (entries: IntersectionObserverEntry[]) => {
      const [entry] = entries;
      if (entry.isIntersecting && autoFetch && hasNextPage && !isFetchingNextPage) {
        logger.debug('Sentinel visible, fetching next page', {
          component: componentName,
          operation: 'autoFetch',
        });
        fetchNextPage();
      }
    },
    [autoFetch, hasNextPage, isFetchingNextPage, fetchNextPage, componentName]
  );

  // Set up the sentinel ref callback
  const sentinelRef = useCallback(
    (node: HTMLElement | null) => {
      // Disconnect existing observer
      if (observerRef.current) {
        observerRef.current.disconnect();
        observerRef.current = null;
        setIsObserving(false);
      }

      sentinelNodeRef.current = node;

      // Create new observer if node exists and autoFetch is enabled
      if (node && autoFetch) {
        const options: IntersectionObserverInit = {
          root: null,
          rootMargin: '100px',
          threshold,
          ...observerOptions,
        };

        observerRef.current = new IntersectionObserver(handleIntersection, options);
        observerRef.current.observe(node);
        setIsObserving(true);

        logger.debug('Infinite scroll observer attached', {
          component: componentName,
          operation: 'observerAttached',
        });
      }
    },
    [autoFetch, threshold, observerOptions, handleIntersection, componentName]
  );

  // Cleanup observer on unmount
  useEffect(() => {
    return () => {
      if (observerRef.current) {
        observerRef.current.disconnect();
        observerRef.current = null;
      }
    };
  }, []);

  // Log errors
  useEffect(() => {
    if (error) {
      logger.error('Infinite scroll query error', {
        component: componentName,
        operation: 'query',
      }, error);
    }
  }, [error, componentName]);

  return {
    data: flattenedData,
    pages,
    isLoading,
    isFetchingNextPage,
    isFetchingPreviousPage,
    isFetching,
    hasNextPage: hasNextPage ?? false,
    hasPreviousPage: hasPreviousPage ?? false,
    totalCount,
    loadedCount,
    error,
    fetchNextPage,
    fetchPreviousPage,
    refetch,
    sentinelRef,
    reset,
  };
}

/**
 * Convenience hook for offset-based pagination.
 *
 * @param options - Configuration options
 * @returns Infinite scroll state and controls
 */
export function useOffsetPagination<TItem>(
  options: Omit<
    UseInfiniteScrollOptions<TItem, { offset: number; limit: number }>,
    'getNextPageParam' | 'initialPageParam'
  > & {
    /** Total count of items (for calculating hasNextPage) */
    getTotalCount?: (page: InfiniteScrollPage<TItem>) => number;
  }
) {
  const { pageSize = 20, getTotalCount, ...restOptions } = options;

  return useInfiniteScroll<TItem, { offset: number; limit: number }>({
    ...restOptions,
    pageSize,
    initialPageParam: { offset: 0, limit: pageSize },
    getNextPageParam: (lastPage, allPages) => {
      const loadedCount = allPages.reduce((sum, page) => sum + page.items.length, 0);
      const totalCount = getTotalCount
        ? getTotalCount(lastPage)
        : lastPage.totalCount;

      if (totalCount !== undefined && loadedCount >= totalCount) {
        return undefined;
      }

      if (lastPage.hasMore === false) {
        return undefined;
      }

      if (lastPage.items.length < pageSize) {
        return undefined;
      }

      return { offset: loadedCount, limit: pageSize };
    },
  });
}

/**
 * Convenience hook for cursor-based pagination.
 *
 * @param options - Configuration options
 * @returns Infinite scroll state and controls
 */
export function useCursorPagination<TItem>(
  options: Omit<
    UseInfiniteScrollOptions<TItem, { cursor?: string; limit: number }>,
    'getNextPageParam' | 'initialPageParam'
  >
) {
  const { pageSize = 20, ...restOptions } = options;

  return useInfiniteScroll<TItem, { cursor?: string; limit: number }>({
    ...restOptions,
    pageSize,
    initialPageParam: { limit: pageSize },
    getNextPageParam: (lastPage) => {
      if (!lastPage.nextCursor) {
        return undefined;
      }
      return { cursor: lastPage.nextCursor, limit: pageSize };
    },
  });
}

/**
 * Hook for virtual scrolling with infinite data.
 * Provides row indices for virtualization libraries.
 *
 * @param totalCount - Total number of items
 * @param loadedCount - Number of currently loaded items
 * @param fetchMore - Function to fetch more items
 * @returns Virtualization helpers
 */
export function useVirtualInfinite(
  totalCount: number | undefined,
  loadedCount: number,
  fetchMore: () => void
) {
  const isItemLoaded = useCallback(
    (index: number) => index < loadedCount,
    [loadedCount]
  );

  const loadMoreItems = useCallback(
    (_startIndex: number, _stopIndex: number) => {
      fetchMore();
      return Promise.resolve();
    },
    [fetchMore]
  );

  return {
    itemCount: totalCount ?? loadedCount,
    isItemLoaded,
    loadMoreItems,
  };
}
