import React from 'react';
import { ErrorBoundary as ReactErrorBoundary } from 'react-error-boundary';
import { AlertCircle, AlertTriangle, RefreshCw, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { logUIError, UIErrorContext } from '@/lib/logUIError';

// Error boundary scope determines UI and logging behavior
export type ErrorBoundaryScope = 'page' | 'section' | 'modal';

interface UnifiedErrorBoundaryProps {
  children: React.ReactNode;
  scope: ErrorBoundaryScope;
  /** Section/component name for display and logging */
  name?: string;
  /** Optional custom fallback UI */
  fallback?: React.ReactNode;
  /** Callback when reset button is clicked */
  onReset?: () => void;
  /** For modal scope: callback to close the modal */
  onClose?: () => void;
  /** Severity level for section scope (warning shows amber, error shows red) */
  severity?: 'warning' | 'error';
  /** Additional context for logging */
  context?: Pick<UIErrorContext, 'route' | 'pageKey' | 'component'>;
}

// Page-scope fallback: Alert-based, full width
function PageErrorFallback({
  error,
  resetErrorBoundary,
}: {
  error: Error;
  resetErrorBoundary: () => void;
}) {
  return (
    <Alert variant="destructive" className="m-4">
      <AlertCircle className="h-4 w-4" />
      <AlertTitle>Something went wrong</AlertTitle>
      <AlertDescription className="space-y-2">
        <p>{error.message || 'An unexpected error occurred'}</p>
        <Button
          variant="outline"
          size="sm"
          onClick={resetErrorBoundary}
        >
          <RefreshCw className="h-3 w-3 mr-1" />
          Try again
        </Button>
      </AlertDescription>
    </Alert>
  );
}

// Section-scope fallback: Card-based, supports warning/error severity
function SectionErrorFallback({
  error,
  resetErrorBoundary,
  sectionName,
  severity = 'error',
}: {
  error: Error;
  resetErrorBoundary: () => void;
  sectionName?: string;
  severity?: 'warning' | 'error';
}) {
  const isWarning = severity === 'warning';
  const toneClasses = isWarning
    ? 'border-amber-300 bg-amber-50'
    : 'border-destructive/50 bg-destructive/5';
  const iconClasses = isWarning ? 'text-amber-600' : 'text-destructive';
  const heading = sectionName
    ? isWarning
      ? `${sectionName} had a hiccup`
      : `${sectionName} failed to load`
    : isWarning
      ? 'Check this section'
      : 'Something went wrong';
  const body =
    error.message ||
    (isWarning
      ? 'This section may not have loaded fully. You can retry safely.'
      : 'An unexpected error occurred');

  return (
    <Card
      className={toneClasses}
      role="alert"
      aria-live={isWarning ? 'polite' : 'assertive'}
    >
      <CardContent className="flex flex-col items-center justify-center p-6 text-center">
        <AlertTriangle
          className={`h-8 w-8 mb-3 ${iconClasses}`}
          aria-hidden="true"
        />
        <h3 className={`font-semibold mb-1 ${isWarning ? 'text-amber-800' : 'text-destructive'}`}>
          {heading}
        </h3>
        <p className="text-sm text-muted-foreground mb-4 max-w-md">{body}</p>
        <Button
          variant="outline"
          size="sm"
          onClick={resetErrorBoundary}
          className="gap-2"
          aria-label="Retry loading this section"
        >
          <RefreshCw className="h-4 w-4" />
          Try Again
        </Button>
      </CardContent>
    </Card>
  );
}

// Modal-scope fallback: Card-based with close button
function ModalErrorFallback({
  error,
  resetErrorBoundary,
  onClose,
}: {
  error: Error;
  resetErrorBoundary: () => void;
  onClose?: () => void;
}) {
  return (
    <Card className="border-destructive/40 bg-destructive/5">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-destructive">
          <AlertCircle className="h-4 w-4" />
          Something went wrong
        </CardTitle>
        <CardDescription>We hit an error while rendering this modal.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="text-sm text-muted-foreground break-words">
          {error.message || 'Unknown error'}
        </div>
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" size="sm" onClick={resetErrorBoundary} className="gap-1">
            <RefreshCw className="h-3 w-3" />
            Try again
          </Button>
          {onClose && (
            <Button variant="secondary" size="sm" onClick={onClose} className="gap-1">
              <X className="h-3 w-3" />
              Close
            </Button>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

/**
 * Unified error boundary with scope-aware UI and logging.
 *
 * Consolidates page, section, and modal error boundaries into a single
 * component with a `scope` prop that determines:
 * - Fallback UI style
 * - Log severity
 * - Available recovery options
 *
 * @example
 * // Page-level error boundary
 * <ErrorBoundary scope="page">
 *   <MyPage />
 * </ErrorBoundary>
 *
 * @example
 * // Section-level with warning severity
 * <ErrorBoundary scope="section" name="User Settings" severity="warning">
 *   <SettingsSection />
 * </ErrorBoundary>
 *
 * @example
 * // Modal with close handler
 * <ErrorBoundary scope="modal" onClose={handleClose}>
 *   <ModalContent />
 * </ErrorBoundary>
 */
export function ErrorBoundary({
  children,
  scope,
  name,
  fallback,
  onReset,
  onClose,
  severity = 'error',
  context,
}: UnifiedErrorBoundaryProps) {
  const renderFallback = ({
    error,
    resetErrorBoundary,
  }: {
    error: Error;
    resetErrorBoundary: () => void;
  }) => {
    if (fallback) {
      return <>{fallback}</>;
    }

    switch (scope) {
      case 'page':
        return <PageErrorFallback error={error} resetErrorBoundary={resetErrorBoundary} />;
      case 'section':
        return (
          <SectionErrorFallback
            error={error}
            resetErrorBoundary={resetErrorBoundary}
            sectionName={name}
            severity={severity}
          />
        );
      case 'modal':
        return (
          <ModalErrorFallback
            error={error}
            resetErrorBoundary={resetErrorBoundary}
            onClose={onClose}
          />
        );
      default:
        return <PageErrorFallback error={error} resetErrorBoundary={resetErrorBoundary} />;
    }
  };

  return (
    <ReactErrorBoundary
      FallbackComponent={renderFallback}
      onError={(error) => {
        logUIError(error, {
          scope,
          component: context?.component ?? name ?? `${scope}ErrorBoundary`,
          route: context?.route,
          pageKey: context?.pageKey ?? name,
          severity,
        });
      }}
      onReset={() => {
        onReset?.();
      }}
    >
      {children}
    </ReactErrorBoundary>
  );
}

// Higher-order component for wrapping components
export function withErrorBoundary<P extends object>(
  Component: React.ComponentType<P>,
  options: {
    scope: ErrorBoundaryScope;
    name?: string;
    severity?: 'warning' | 'error';
  }
) {
  return function WrappedComponent(props: P) {
    return (
      <ErrorBoundary scope={options.scope} name={options.name} severity={options.severity}>
        <Component {...props} />
      </ErrorBoundary>
    );
  };
}

// Re-export legacy names for backward compatibility
export { ErrorBoundary as UnifiedErrorBoundary };

// Export individual fallback components for tests and advanced usage
export { PageErrorFallback, SectionErrorFallback, ModalErrorFallback };
