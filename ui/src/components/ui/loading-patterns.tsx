import React from 'react';
import { Skeleton } from './skeleton';
import { cn } from './utils';
import type { UseQueryResult } from '@tanstack/react-query';

const DEFAULT_LOADING_TEST_ID = 'loading-state';
const DEFAULT_REFRESHING_TEST_ID = 'refreshing-indicator';

/**
 * Standardized loading pattern for tables
 * Shows skeleton rows to indicate table data is loading
 */
export function TableLoadingState({ rows = 5 }: { rows?: number }) {
  return (
    <div className="space-y-2" role="status" aria-label="Loading table data" data-testid={DEFAULT_LOADING_TEST_ID}>
      {Array.from({ length: rows }).map((_, i) => (
        <Skeleton key={i} className="h-12 w-full" />
      ))}
    </div>
  );
}

/**
 * Standardized loading pattern for card grids
 * Shows skeleton cards in responsive grid layout
 */
export function CardGridLoadingState({ cards = 6 }: { cards?: number }) {
  return (
    <div
      className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4"
      role="status"
      aria-label="Loading card grid"
      data-testid={DEFAULT_LOADING_TEST_ID}
    >
      {Array.from({ length: cards }).map((_, i) => (
        <Skeleton key={i} className="h-32 w-full rounded-lg" />
      ))}
    </div>
  );
}

/**
 * Standardized loading pattern for detail pages
 * Shows skeleton for title, description, stats, and main content
 */
export function DetailPageLoadingState() {
  return (
    <div className="space-y-6" role="status" aria-label="Loading page details" data-testid={DEFAULT_LOADING_TEST_ID}>
      <Skeleton className="h-8 w-1/3" /> {/* Title */}
      <Skeleton className="h-4 w-2/3" /> {/* Description */}
      <div className="grid grid-cols-2 gap-4">
        <Skeleton className="h-24" />
        <Skeleton className="h-24" />
      </div>
      <Skeleton className="h-64" /> {/* Main content */}
    </div>
  );
}

/**
 * Standardized loading pattern for inline/small areas
 * Inline skeleton with configurable width
 */
export function InlineLoadingState({ width = 'w-24' }: { width?: string }) {
  return <Skeleton className={cn('h-4 inline-block', width)} />;
}

/**
 * Standardized loading pattern for a compact card footer
 * Shows a single skeleton block for summary data.
 */
export function CardFooterLoadingState({ label }: { label: string }) {
  return (
    <div className="flex w-full items-center gap-3" role="status" aria-label={label} data-testid={DEFAULT_LOADING_TEST_ID}>
      <Skeleton className="h-16 w-full" />
      <span className="sr-only">{label}</span>
    </div>
  );
}

/**
 * Standardized loading pattern for background refresh/polling
 * Shows subtle indicator during background updates
 */
export function RefreshingIndicator({ className }: { className?: string }) {
  return (
    <div
      className={cn('flex items-center gap-2 text-xs text-muted-foreground', className)}
      role="status"
      aria-label="Refreshing data"
      data-testid={DEFAULT_REFRESHING_TEST_ID}
    >
      <Skeleton className="h-3 w-3 rounded-full" />
      <span>Refreshing...</span>
    </div>
  );
}

/**
 * Hook for consistent loading state handling across queries
 * Wraps useQuery result with isEmpty detection and loading state logic
 */
export function useLoadingState<T>(
  query: UseQueryResult<T>,
  options?: {
    showOnRefetch?: boolean;
  }
) {
  const { data, isLoading, isFetching, isError, error } = query;
  const showLoading = isLoading || (options?.showOnRefetch && isFetching);

  return {
    isLoading: showLoading,
    isRefreshing: !isLoading && isFetching,
    isEmpty: !isLoading && (!data || (Array.isArray(data) && data.length === 0)),
    isError,
    error,
    data,
  };
}
