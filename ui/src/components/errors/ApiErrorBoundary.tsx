import { Component, ReactNode } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { AlertTriangle } from 'lucide-react';

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

/**
 * ApiErrorBoundary - React error boundary for API data issues
 *
 * Catches rendering errors caused by malformed or unexpected API responses,
 * displays user-friendly error message, and provides retry functionality.
 *
 * Usage:
 * ```tsx
 * <ApiErrorBoundary>
 *   <ComponentThatMightFail />
 * </ApiErrorBoundary>
 * ```
 *
 * Or with custom fallback:
 * ```tsx
 * <ApiErrorBoundary fallback={<CustomErrorUI />}>
 *   <ComponentThatMightFail />
 * </ApiErrorBoundary>
 * ```
 */
export class ApiErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, error: null };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    // Log error details for debugging
    console.error('ApiErrorBoundary caught error:', {
      error,
      errorInfo: info,
      componentStack: info.componentStack,
      timestamp: new Date().toISOString(),
    });
  }

  handleRetry = () => {
    // Reset error state to retry rendering
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (this.state.hasError) {
      // Use custom fallback if provided, otherwise show default error UI
      return this.props.fallback || (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <AlertTriangle className="h-5 w-5 text-destructive" />
              Something went wrong
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-muted-foreground mb-4">
              {this.state.error?.message || 'An unexpected error occurred while rendering this content'}
            </p>
            <Button onClick={this.handleRetry} variant="outline">
              Try again
            </Button>
          </CardContent>
        </Card>
      );
    }

    return this.props.children;
  }
}
