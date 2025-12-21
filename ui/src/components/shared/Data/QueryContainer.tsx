/**
 * QueryContainer
 *
 * A declarative wrapper for TanStack Query results that standardizes
 * loading, error, and empty state handling across the application.
 *
 * Reduces the repeated pattern of:
 *   if (isLoading) return <LoadingState />;
 *   if (error) return <ErrorState />;
 *   if (!data?.length) return <EmptyState />;
 *   return <DataComponent data={data} />;
 *
 * @example
 * ```tsx
 * const query = useQuery({ queryKey: ['adapters'], queryFn: fetchAdapters });
 *
 * <QueryContainer
 *   query={query}
 *   isEmpty={(data) => !data?.length}
 *   emptyComponent={<EmptyState title="No adapters" />}
 * >
 *   {(data) => <AdapterList adapters={data} />}
 * </QueryContainer>
 * ```
 */

import * as React from 'react';
import type { UseQueryResult } from '@tanstack/react-query';
import { AlertCircle, RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { cn } from '@/lib/utils';

export interface QueryContainerProps<TData> {
  /** The TanStack Query result object */
  query: UseQueryResult<TData, Error>;
  /** Render function that receives the data when available */
  children: (data: TData) => React.ReactNode;
  /** Component to show while loading (defaults to skeleton) */
  loadingComponent?: React.ReactNode;
  /** Component to show on error (defaults to error card with retry) */
  errorComponent?: React.ReactNode | ((error: Error, refetch: () => void) => React.ReactNode);
  /** Component to show when data is empty */
  emptyComponent?: React.ReactNode;
  /** Function to determine if data should be considered empty */
  isEmpty?: (data: TData) => boolean;
  /** Additional className for the container */
  className?: string;
  /** Skeleton rows to show during initial load */
  skeletonRows?: number;
  /** Compact error display (inline instead of card) */
  compactError?: boolean;
}

/**
 * Default loading skeleton for QueryContainer
 */
function DefaultLoadingSkeleton({ rows = 3 }: { rows?: number }) {
  return (
    <div className="space-y-3">
      {Array.from({ length: rows }).map((_, i) => (
        <Skeleton key={i} className="h-12 w-full" />
      ))}
    </div>
  );
}

/**
 * Default error component for QueryContainer
 */
function DefaultErrorComponent({
  error,
  refetch,
  compact = false,
}: {
  error: Error;
  refetch: () => void;
  compact?: boolean;
}) {
  if (compact) {
    return (
      <div className="flex items-center gap-2 text-sm text-destructive">
        <AlertCircle className="h-4 w-4 shrink-0" />
        <span className="flex-1">{error.message || 'Failed to load data'}</span>
        <Button variant="ghost" size="sm" onClick={() => refetch()} className="h-7 px-2">
          <RefreshCw className="h-3 w-3 mr-1" />
          Retry
        </Button>
      </div>
    );
  }

  return (
    <Card className="border-destructive/50 bg-destructive/5">
      <CardContent className="flex flex-col items-center justify-center p-6 text-center">
        <AlertCircle className="h-8 w-8 mb-3 text-destructive" />
        <h3 className="font-semibold mb-1 text-destructive">Failed to load data</h3>
        <p className="text-sm text-muted-foreground mb-4 max-w-md">
          {error.message || 'An unexpected error occurred'}
        </p>
        <Button variant="outline" size="sm" onClick={() => refetch()} className="gap-2">
          <RefreshCw className="h-4 w-4" />
          Try Again
        </Button>
      </CardContent>
    </Card>
  );
}

/**
 * Default empty component for QueryContainer
 */
function DefaultEmptyComponent() {
  return (
    <div className="flex flex-col items-center justify-center py-12 text-center text-muted-foreground">
      <p className="text-sm">No data available</p>
    </div>
  );
}

/**
 * QueryContainer - Declarative wrapper for TanStack Query results
 */
export function QueryContainer<TData>({
  query,
  children,
  loadingComponent,
  errorComponent,
  emptyComponent,
  isEmpty,
  className,
  skeletonRows = 3,
  compactError = false,
}: QueryContainerProps<TData>): React.ReactElement | null {
  const { data, isLoading, isPending, error, refetch, isFetching, isRefetching } = query;

  // Loading state (initial load, not background refetch)
  if (isLoading || isPending) {
    return (
      <div className={cn('relative', className)}>
        {loadingComponent ?? <DefaultLoadingSkeleton rows={skeletonRows} />}
      </div>
    );
  }

  // Error state
  if (error) {
    const errorContent =
      typeof errorComponent === 'function'
        ? errorComponent(error, refetch)
        : errorComponent ?? <DefaultErrorComponent error={error} refetch={refetch} compact={compactError} />;

    return <div className={cn('relative', className)}>{errorContent}</div>;
  }

  // Empty state (data exists but is empty)
  if (data !== undefined && isEmpty?.(data)) {
    return (
      <div className={cn('relative', className)}>
        {emptyComponent ?? <DefaultEmptyComponent />}
      </div>
    );
  }

  // Data available - render children
  if (data !== undefined) {
    return (
      <div className={cn('relative', className)}>
        {/* Show subtle loading indicator during background refetch */}
        {(isFetching || isRefetching) && (
          <div className="absolute top-0 right-0 p-1">
            <RefreshCw className="h-3 w-3 animate-spin text-muted-foreground" />
          </div>
        )}
        {children(data)}
      </div>
    );
  }

  // Fallback (shouldn't typically reach here)
  return null;
}

QueryContainer.displayName = 'QueryContainer';

export default QueryContainer;
