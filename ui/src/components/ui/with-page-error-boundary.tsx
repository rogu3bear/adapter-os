import React from 'react';
import { ErrorBoundary } from 'react-error-boundary';
import { AlertTriangle, RefreshCw, Home } from 'lucide-react';
import { Button } from './button';
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from './card';
import { logUIError } from '@/lib/logUIError';
import { useNavigate } from 'react-router-dom';

interface PageErrorFallbackProps {
  error: Error;
  resetErrorBoundary: () => void;
  pageName?: string;
}

function PageErrorFallback({ error, resetErrorBoundary, pageName }: PageErrorFallbackProps) {
  const navigate = useNavigate();
  const isDev = import.meta.env.DEV;

  const handleGoBack = () => {
    navigate(-1);
    resetErrorBoundary();
  };

  const handleGoHome = () => {
    navigate('/dashboard');
    resetErrorBoundary();
  };

  return (
    <div className="flex items-center justify-center p-6">
      <Card className="w-full max-w-2xl border-destructive/50">
        <CardHeader>
          <div className="flex items-start gap-3">
            <div className="mt-0.5 flex h-9 w-9 items-center justify-center rounded-full bg-destructive/10">
              <AlertTriangle className="h-5 w-5 text-destructive" aria-hidden="true" />
            </div>
            <div className="space-y-1">
              <CardTitle>Page Error</CardTitle>
              <CardDescription>
                {pageName ? `The ${pageName} page encountered an error.` : 'This page encountered an error.'} You can try again or navigate away.
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="rounded-md bg-muted/50 p-3 space-y-1">
            <p className="text-sm font-medium text-foreground">Error:</p>
            <p className="text-sm text-muted-foreground">
              {error.message || 'An unexpected error occurred'}
            </p>
          </div>

          {isDev && error.stack && (
            <details className="rounded-md bg-muted/30 p-2">
              <summary className="cursor-pointer text-xs font-medium text-muted-foreground hover:text-foreground">
                Stack Trace
              </summary>
              <pre className="mt-2 overflow-auto text-xs text-muted-foreground whitespace-pre-wrap max-h-60">
                {error.stack}
              </pre>
            </details>
          )}
        </CardContent>
        <CardFooter className="flex flex-wrap gap-2">
          <Button onClick={resetErrorBoundary} size="sm" className="gap-2">
            <RefreshCw className="h-4 w-4" />
            Try Again
          </Button>
          <Button onClick={handleGoBack} variant="outline" size="sm" className="gap-2">
            Go Back
          </Button>
          <Button onClick={handleGoHome} variant="ghost" size="sm" className="gap-2">
            <Home className="h-4 w-4" />
            Dashboard
          </Button>
        </CardFooter>
      </Card>
    </div>
  );
}

interface WithPageErrorBoundaryOptions {
  pageName?: string;
  onError?: (error: Error, errorInfo: React.ErrorInfo) => void;
}

/**
 * withPageErrorBoundary - HOC that wraps a page component with error boundary
 *
 * Usage:
 * ```tsx
 * export default withPageErrorBoundary(InferencePage, { pageName: 'Inference' });
 * ```
 */
export function withPageErrorBoundary<P extends object>(
  Component: React.ComponentType<P>,
  options: WithPageErrorBoundaryOptions = {}
) {
  const { pageName, onError } = options;

  return function PageWithErrorBoundary(props: P) {
    const handleError = (error: Error, errorInfo: React.ErrorInfo) => {
      logUIError(error, {
        scope: 'page',
        component: Component.displayName || Component.name || 'Page',
        pageKey: pageName,
        severity: 'error',
        errorInfo: errorInfo.componentStack || undefined,
      });

      onError?.(error, errorInfo);
    };

    return (
      <ErrorBoundary
        FallbackComponent={({ error, resetErrorBoundary }) => (
          <PageErrorFallback
            error={error}
            resetErrorBoundary={resetErrorBoundary}
            pageName={pageName}
          />
        )}
        onError={handleError}
        onReset={() => {
          // Optional: Clear any page-specific state
        }}
      >
        <Component {...props} />
      </ErrorBoundary>
    );
  };
}
