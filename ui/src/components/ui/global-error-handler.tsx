import React from 'react';
import { ErrorBoundary } from 'react-error-boundary';
import { AlertTriangle, RefreshCw, Home } from 'lucide-react';
import { Button } from './button';
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from './card';
import { logUIError, type UIErrorSeverity } from '@/lib/logUIError';
import { useNavigate } from 'react-router-dom';

interface GlobalErrorFallbackProps {
  error: Error;
  resetErrorBoundary: () => void;
  componentStack?: string;
}

/**
 * GlobalErrorFallback - Top-level error fallback for catastrophic failures
 *
 * Provides recovery options:
 * - Retry rendering
 * - Navigate to dashboard
 * - Show error details in development
 */
export function GlobalErrorFallback({ error, resetErrorBoundary }: GlobalErrorFallbackProps) {
  const navigate = useNavigate();
  const isDev = import.meta.env.DEV;

  const handleGoHome = () => {
    navigate('/dashboard');
    resetErrorBoundary();
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-background p-4">
      <Card className="w-full max-w-2xl border-destructive/50">
        <CardHeader>
          <div className="flex items-start gap-3">
            <div className="mt-0.5 flex h-10 w-10 items-center justify-center rounded-full bg-destructive/10">
              <AlertTriangle className="h-6 w-6 text-destructive" aria-hidden="true" />
            </div>
            <div className="space-y-1">
              <CardTitle className="text-xl">Application Error</CardTitle>
              <CardDescription>
                Something went wrong and the application couldn't recover. You can try reloading or go back to the dashboard.
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="rounded-md bg-muted/50 p-4 space-y-2">
            <p className="text-sm font-medium text-foreground">Error Details:</p>
            <p className="text-sm font-mono text-muted-foreground break-all">
              {error.message || 'An unexpected error occurred'}
            </p>
          </div>

          {isDev && error.stack && (
            <details className="rounded-md bg-muted/30 p-3">
              <summary className="cursor-pointer text-sm font-medium text-muted-foreground hover:text-foreground">
                Stack Trace (Development Only)
              </summary>
              <pre className="mt-2 overflow-auto text-xs text-muted-foreground whitespace-pre-wrap">
                {error.stack}
              </pre>
            </details>
          )}
        </CardContent>
        <CardFooter className="flex flex-col sm:flex-row gap-2">
          <Button onClick={resetErrorBoundary} className="gap-2 flex-1">
            <RefreshCw className="h-4 w-4" />
            Try Again
          </Button>
          <Button onClick={handleGoHome} variant="outline" className="gap-2 flex-1">
            <Home className="h-4 w-4" />
            Go to Dashboard
          </Button>
        </CardFooter>
      </Card>
    </div>
  );
}

interface GlobalErrorHandlerProps {
  children: React.ReactNode;
  onError?: (error: Error, errorInfo: React.ErrorInfo) => void;
}

/**
 * GlobalErrorHandler - Wraps the entire application with error recovery
 *
 * Usage:
 * ```tsx
 * <GlobalErrorHandler>
 *   <App />
 * </GlobalErrorHandler>
 * ```
 */
export function GlobalErrorHandler({ children, onError }: GlobalErrorHandlerProps) {
  const handleError = (error: Error, errorInfo: React.ErrorInfo) => {
    // Log to error tracking service
    logUIError(error, {
      scope: 'global',
      component: 'GlobalErrorHandler',
      severity: 'critical',
      errorInfo: errorInfo.componentStack || undefined,
    });

    // Call custom error handler if provided
    onError?.(error, errorInfo);
  };

  return (
    <ErrorBoundary
      FallbackComponent={GlobalErrorFallback}
      onError={handleError}
      onReset={() => {
        // Clear any cached query data that might be causing issues
        window.location.reload();
      }}
    >
      {children}
    </ErrorBoundary>
  );
}
