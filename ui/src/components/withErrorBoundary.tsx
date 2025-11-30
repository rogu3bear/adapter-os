import { ErrorBoundary } from './shared/Feedback';
import { ComponentType } from 'react';

export function withErrorBoundary<P extends object>(
  Component: ComponentType<P>,
  fallbackMessage?: string
) {
  return function WithErrorBoundary(props: P) {
    return (
      <ErrorBoundary fallback={fallbackMessage}>
        <Component {...props} />
      </ErrorBoundary>
    );
  };
}
