import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { AlertCircle, RefreshCw } from 'lucide-react';

interface QueryErrorFallbackProps {
  error: Error | unknown;
  refetch?: () => void;
  resetErrorBoundary?: () => void;
  showDetails?: boolean;
}

/**
 * QueryErrorFallback - Fallback component for TanStack Query errors
 *
 * Displays formatted error messages from failed API queries with retry functionality.
 * Can be used with useQuery's onError or as a standalone error display component.
 *
 * Usage with TanStack Query:
 * ```tsx
 * const { data, error, refetch } = useQuery({
 *   queryKey: ['key'],
 *   queryFn: fetchData,
 * });
 *
 * if (error) {
 *   return <QueryErrorFallback error={error} refetch={refetch} />;
 * }
 * ```
 *
 * Usage as wrapper with error boundary:
 * ```tsx
 * <ErrorBoundary FallbackComponent={QueryErrorFallback}>
 *   <ComponentThatUsesQuery />
 * </ErrorBoundary>
 * ```
 */
export function QueryErrorFallback({
  error,
  refetch,
  resetErrorBoundary,
  showDetails = false,
}: QueryErrorFallbackProps) {
  // Extract error message
  const errorMessage = error instanceof Error
    ? error.message
    : 'An unexpected error occurred while fetching data';

  // Determine if we can retry
  const canRetry = refetch || resetErrorBoundary;
  const handleRetry = () => {
    if (refetch) {
      refetch();
    } else if (resetErrorBoundary) {
      resetErrorBoundary();
    }
  };

  // Log error for debugging
  console.error('QueryErrorFallback rendering error:', {
    error,
    timestamp: new Date().toISOString(),
  });

  return (
    <Card className="border-destructive/50">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-destructive">
          <AlertCircle className="h-5 w-5" />
          Failed to load data
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2">
          <p className="text-sm text-muted-foreground">
            {errorMessage}
          </p>

          {showDetails && error instanceof Error && error.stack && (
            <details className="mt-2">
              <summary className="cursor-pointer text-xs text-muted-foreground hover:text-foreground">
                Error details
              </summary>
              <pre className="mt-2 overflow-auto rounded-md bg-muted p-2 text-xs">
                {error.stack}
              </pre>
            </details>
          )}
        </div>

        {canRetry && (
          <Button
            onClick={handleRetry}
            variant="outline"
            className="gap-2"
          >
            <RefreshCw className="h-4 w-4" />
            Retry
          </Button>
        )}
      </CardContent>
    </Card>
  );
}
