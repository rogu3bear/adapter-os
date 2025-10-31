// 【ui/src/components/ui/error-recovery.tsx§236-252】 - Generic error template
import React from 'react';
import { ErrorBoundary as ReactErrorBoundary } from 'react-error-boundary';
import { logger } from '../utils/logger';
import { ErrorRecoveryTemplates } from './ui/error-recovery';

interface ErrorFallbackProps {
  error: Error;
  resetErrorBoundary: () => void;
}

function ErrorFallback({ error, resetErrorBoundary }: ErrorFallbackProps) {
  return (
    <div className="flex items-center justify-center min-h-screen p-6">
      {ErrorRecoveryTemplates.genericError(
        error,
        async () => {
          resetErrorBoundary();
        }
      )}
    </div>
  );
}

interface ErrorBoundaryProps {
  children: React.ReactNode;
  fallback?: React.ReactNode;
}

export function ErrorBoundary({ children, fallback }: ErrorBoundaryProps) {
  return (
    <ReactErrorBoundary 
      FallbackComponent={fallback ? () => <>{fallback}</> : ErrorFallback}
      onError={(error, errorInfo) => {
        logger.error('Error caught by boundary', {
          component: 'ErrorBoundary',
          componentStack: errorInfo.componentStack,
        }, error);
      }}
      onReset={() => {
        window.location.reload();
      }}
    >
      {children}
    </ReactErrorBoundary>
  );
}
