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
}

function SectionErrorFallback({ error, resetErrorBoundary, sectionName }: SectionErrorFallbackProps) {
  return (
    <Card
      className="border-destructive/50 bg-destructive/5"
      role="alert"
      aria-live="assertive"
    >
      <CardContent className="flex flex-col items-center justify-center p-6 text-center">
        <AlertTriangle
          className="h-8 w-8 text-destructive mb-3"
          aria-hidden="true"
        />
        <h3 className="font-semibold text-destructive mb-1">
          {sectionName ? `${sectionName} failed to load` : 'Something went wrong'}
        </h3>
        <p className="text-sm text-muted-foreground mb-4 max-w-md">
          {error.message || 'An unexpected error occurred'}
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
}

export function SectionErrorBoundary({
  children,
  sectionName,
  onReset,
  fallback
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
          />
        )
      }
      onError={(error, errorInfo) => {
        logUIError(error, {
          scope: 'section',
          component: 'SectionErrorBoundary',
          route: undefined,
          pageKey: sectionName,
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
