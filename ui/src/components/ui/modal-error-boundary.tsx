import React from 'react';
import { ErrorBoundary as ReactErrorBoundary } from 'react-error-boundary';
import { AlertCircle, RefreshCw, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { logUIError, UIErrorContext } from '@/lib/logUIError';

interface ModalErrorBoundaryProps {
  children: React.ReactNode;
  context?: Pick<UIErrorContext, 'route' | 'pageKey' | 'component'>;
  onClose?: () => void;
}

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
        <div className="text-sm text-muted-foreground break-words">{error.message || 'Unknown error'}</div>
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

export function ModalErrorBoundary({ children, context, onClose }: ModalErrorBoundaryProps) {
  return (
    <ReactErrorBoundary
      FallbackComponent={({ error, resetErrorBoundary }) => (
        <ModalErrorFallback error={error} resetErrorBoundary={resetErrorBoundary} onClose={onClose} />
      )}
      onError={(error) =>
        logUIError(error, {
          scope: 'modal',
          component: context?.component ?? 'Modal',
          route: context?.route,
          pageKey: context?.pageKey,
        })
      }
    >
      {children}
    </ReactErrorBoundary>
  );
}

export default ModalErrorBoundary;

