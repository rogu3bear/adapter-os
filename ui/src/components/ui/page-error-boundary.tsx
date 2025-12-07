import React, { Component, createContext, useContext, useState, useCallback, useEffect } from 'react';
import { AlertCircle, X, RefreshCw } from 'lucide-react';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { logUIError } from '@/lib/logUIError';

// Types
export interface PageError {
  id: string;
  message: string;
  timestamp: Date;
  dismissible?: boolean;
  recoveryAction?: () => void;
  recoveryLabel?: string;
}

interface PageErrorsContextValue {
  errors: PageError[];
  addError: (id: string, message: string, recoveryAction?: () => void, options?: { dismissible?: boolean; recoveryLabel?: string }) => void;
  clearError: (id: string) => void;
  clearAll: () => void;
}

// Context
const PageErrorsContext = createContext<PageErrorsContextValue | null>(null);

// Hook
export function usePageErrors(): PageErrorsContextValue {
  const context = useContext(PageErrorsContext);
  if (!context) {
    throw new Error('usePageErrors must be used within a PageErrorsProvider');
  }
  return context;
}

// Provider
export function PageErrorsProvider({ children }: { children: React.ReactNode }) {
  const [errors, setErrors] = useState<PageError[]>([]);

  const addError = useCallback((
    id: string,
    message: string,
    recoveryAction?: () => void,
    options?: { dismissible?: boolean; recoveryLabel?: string }
  ) => {
    const newError: PageError = {
      id,
      message,
      timestamp: new Date(),
      dismissible: options?.dismissible ?? true,
      recoveryAction,
      recoveryLabel: options?.recoveryLabel ?? 'Retry',
    };

    // Log error
    console.error(`[PageError] ${id}: ${message}`);

    setErrors(prev => {
      // Replace existing error with same id or add new
      const filtered = prev.filter(e => e.id !== id);
      return [...filtered, newError];
    });
  }, []);

  const clearError = useCallback((id: string) => {
    setErrors(prev => prev.filter(e => e.id !== id));
  }, []);

  const clearAll = useCallback(() => {
    setErrors([]);
  }, []);

  return (
    <PageErrorsContext.Provider value={{ errors, addError, clearError, clearAll }}>
      {children}
    </PageErrorsContext.Provider>
  );
}

// Display component
export function PageErrors({ errors }: { errors: PageError[] }) {
  const { clearError } = usePageErrors();

  if (errors.length === 0) return null;

  return (
    <div className="space-y-2 mb-4">
      {errors.map(error => (
        <Alert key={error.id} variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle className="flex items-center justify-between">
            <span>Error</span>
            <div className="flex items-center gap-2">
              {error.recoveryAction && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={error.recoveryAction}
                  className="h-6 px-2 text-xs"
                >
                  <RefreshCw className="h-3 w-3 mr-1" />
                  {error.recoveryLabel}
                </Button>
              )}
              {error.dismissible && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => clearError(error.id)}
                  className="h-6 w-6 p-0"
                >
                  <X className="h-3 w-3" />
                </Button>
              )}
            </div>
          </AlertTitle>
          <AlertDescription>{error.message}</AlertDescription>
        </Alert>
      ))}
    </div>
  );
}

// Error Boundary
interface ErrorBoundaryProps {
  children: React.ReactNode;
  fallback?: React.ReactNode;
  onError?: (error: Error, errorInfo: React.ErrorInfo) => void;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

export class PageErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error('[PageErrorBoundary] Caught error:', error, errorInfo);
    this.props.onError?.(error, errorInfo);
    logUIError(error, { scope: 'page', component: 'PageErrorBoundary' });
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback;
      }

      return (
        <Alert variant="destructive" className="m-4">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>Something went wrong</AlertTitle>
          <AlertDescription className="space-y-2">
            <p>{this.state.error?.message || 'An unexpected error occurred'}</p>
            <Button
              variant="outline"
              size="sm"
              onClick={() => this.setState({ hasError: false, error: null })}
            >
              <RefreshCw className="h-3 w-3 mr-1" />
              Try again
            </Button>
          </AlertDescription>
        </Alert>
      );
    }

    return this.props.children;
  }
}

// Convenience wrapper that includes provider
export function withPageErrors<P extends object>(
  WrappedComponent: React.ComponentType<P>
): React.FC<P> {
  return function WithPageErrorsWrapper(props: P) {
    return (
      <PageErrorsProvider>
        <PageErrorBoundary>
          <WrappedComponent {...props} />
        </PageErrorBoundary>
      </PageErrorsProvider>
    );
  };
}
