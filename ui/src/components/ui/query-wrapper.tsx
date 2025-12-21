import React from 'react';
import { UseQueryResult } from '@tanstack/react-query';
import { QueryErrorFallback } from '@/components/errors/QueryErrorFallback';
import { LoadingState } from '@/components/ui/loading-state';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { AlertCircle } from 'lucide-react';

interface QueryWrapperProps<TData> {
  query: UseQueryResult<TData>;
  loadingMessage?: string;
  errorTitle?: string;
  emptyMessage?: string;
  isEmpty?: (data: TData) => boolean;
  children: (data: TData) => React.ReactNode;
  showDetails?: boolean;
}

/**
 * QueryWrapper - Declarative wrapper for TanStack Query results
 *
 * Handles loading, error, and empty states automatically.
 *
 * Usage:
 * ```tsx
 * const adaptersQuery = useQuery({
 *   queryKey: ['adapters'],
 *   queryFn: fetchAdapters,
 * });
 *
 * return (
 *   <QueryWrapper
 *     query={adaptersQuery}
 *     loadingMessage="Loading adapters..."
 *     errorTitle="Failed to load adapters"
 *     emptyMessage="No adapters found"
 *     isEmpty={(data) => data.length === 0}
 *   >
 *     {(data) => (
 *       <AdaptersList adapters={data} />
 *     )}
 *   </QueryWrapper>
 * );
 * ```
 */
export function QueryWrapper<TData>({
  query,
  loadingMessage = 'Loading...',
  errorTitle = 'Failed to load data',
  emptyMessage,
  isEmpty,
  children,
  showDetails = false,
}: QueryWrapperProps<TData>) {
  const { data, error, isLoading, refetch } = query;

  // Loading state
  if (isLoading) {
    return <LoadingState message={loadingMessage} />;
  }

  // Error state
  if (error) {
    return (
      <QueryErrorFallback
        error={error}
        refetch={refetch}
        showDetails={showDetails}
      />
    );
  }

  // No data
  if (!data) {
    return (
      <Alert>
        <AlertCircle className="h-4 w-4" />
        <AlertTitle>No Data</AlertTitle>
        <AlertDescription>
          {emptyMessage || 'No data available'}
        </AlertDescription>
      </Alert>
    );
  }

  // Empty data check
  if (isEmpty && isEmpty(data)) {
    return (
      <Alert>
        <AlertCircle className="h-4 w-4" />
        <AlertTitle>Empty</AlertTitle>
        <AlertDescription>
          {emptyMessage || 'No items found'}
        </AlertDescription>
      </Alert>
    );
  }

  // Render children with data
  return <>{children(data)}</>;
}

interface MultiQueryWrapperProps {
  queries: Array<{
    query: UseQueryResult<unknown>;
    name: string;
  }>;
  loadingMessage?: string;
  children: () => React.ReactNode;
}

/**
 * MultiQueryWrapper - Wrapper for multiple TanStack Query results
 *
 * Handles loading and error states for multiple queries.
 *
 * Usage:
 * ```tsx
 * const adaptersQuery = useQuery(['adapters'], fetchAdapters);
 * const modelsQuery = useQuery(['models'], fetchModels);
 *
 * return (
 *   <MultiQueryWrapper
 *     queries={[
 *       { query: adaptersQuery, name: 'Adapters' },
 *       { query: modelsQuery, name: 'Models' },
 *     ]}
 *     loadingMessage="Loading data..."
 *   >
 *     {() => (
 *       <div>
 *         <AdaptersList adapters={adaptersQuery.data} />
 *         <ModelsList models={modelsQuery.data} />
 *       </div>
 *     )}
 *   </MultiQueryWrapper>
 * );
 * ```
 */
export function MultiQueryWrapper({
  queries,
  loadingMessage = 'Loading...',
  children,
}: MultiQueryWrapperProps) {
  // Check if any query is loading
  const isLoading = queries.some((q) => q.query.isLoading);
  if (isLoading) {
    return <LoadingState message={loadingMessage} />;
  }

  // Collect errors from all queries
  const errors = queries
    .filter((q) => q.query.error)
    .map((q) => ({ name: q.name, error: q.query.error }));

  // Show errors if any
  if (errors.length > 0) {
    return (
      <div className="space-y-3">
        {errors.map(({ name, error }, index) => (
          <QueryErrorFallback
            key={index}
            error={error as Error}
            refetch={queries.find((q) => q.name === name)?.query.refetch}
          />
        ))}
      </div>
    );
  }

  // All queries successful, render children
  return <>{children()}</>;
}
