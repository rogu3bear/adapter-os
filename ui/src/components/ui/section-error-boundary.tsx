import React from 'react';
import { ErrorBoundary as ReactErrorBoundary } from 'react-error-boundary';
import { AlertTriangle, RefreshCw } from 'lucide-react';
import { Button } from './button';
import { Card, CardContent } from './card';
import { logUIError } from '@/lib/logUIError';

interface SectionErrorFallbackProps {
  error: Error;
  resetErrorBoundary: () => void;
  sectionName?: string;
  severity?: 'warning' | 'error';
}

export function SectionErrorFallback({ error, resetErrorBoundary, sectionName, severity = 'error' }: SectionErrorFallbackProps) {
  const isWarning = severity === 'warning';
  const toneClasses = isWarning ? 'border-amber-300 bg-amber-50' : 'border-destructive/50 bg-destructive/5';
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
      className={`${toneClasses}`}
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
        <p className="text-sm text-muted-foreground mb-4 max-w-md">
          {body}
        </p>
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

interface SectionErrorBoundaryProps {
  children: React.ReactNode;
  sectionName?: string;
  onReset?: () => void;
  fallback?: React.ReactNode;
  severity?: 'warning' | 'error';
}

export function SectionErrorBoundary({
  children,
  sectionName,
  onReset,
  fallback,
  severity = 'error',
}: SectionErrorBoundaryProps) {
  return (
    <ReactErrorBoundary
      FallbackComponent={({ error, resetErrorBoundary }) =>
        fallback ? (
          <>{fallback}</>
        ) : (
          <SectionErrorFallback
            error={error}
            resetErrorBoundary={resetErrorBoundary}
            sectionName={sectionName}
            severity={severity}
          />
        )
      }
      onError={(error, errorInfo) => {
        logUIError(error, {
          scope: 'section',
          component: 'SectionErrorBoundary',
          route: undefined,
          pageKey: sectionName,
          severity,
        });
      }}
      onReset={() => {
        if (onReset) {
          onReset();
        }
      }}
    >
      {children}
    </ReactErrorBoundary>
  );
}

// Higher-order component for wrapping sections
export function withSectionErrorBoundary<P extends object>(
  Component: React.ComponentType<P>,
  sectionName: string
) {
  return function WrappedComponent(props: P) {
    return (
      <SectionErrorBoundary sectionName={sectionName}>
        <Component {...props} />
      </SectionErrorBoundary>
    );
  };
}
