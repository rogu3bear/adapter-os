/**
 * OperatorChatLayout - Chat-first layout for operator dashboard
 *
 * Combines ModelStatusBar at top with ChatInterface filling the main viewport.
 * Shows loading state while model is being auto-loaded.
 * Handles error states with retry functionality.
 */

import React from 'react';
import { Loader2, MessageSquare, AlertTriangle, RefreshCw, WifiOff, ServerOff, Clock, HardDrive } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { ModelStatusBar } from './ModelStatusBar';
import { ChatInterface } from '@/components/ChatInterface';
import { useModelStatus } from '@/hooks/useModelStatus';
import { useAutoLoadModel, type AutoLoadError } from '@/hooks/useAutoLoadModel';
import { useGetDefaultStack } from '@/hooks/useAdmin';

interface OperatorChatLayoutProps {
  tenantId: string;
}

// Error display component
function ErrorDisplay({
  error,
  onRetry,
  onDismiss,
  isRetrying
}: {
  error: AutoLoadError;
  onRetry: () => void;
  onDismiss: () => void;
  isRetrying: boolean;
}) {
  const getErrorIcon = () => {
    switch (error.code) {
      case 'NETWORK_ERROR':
        return <WifiOff className="h-5 w-5" />;
      case 'NO_MODELS':
        return <ServerOff className="h-5 w-5" />;
      case 'TIMEOUT':
        return <Clock className="h-5 w-5" />;
      case 'OUT_OF_MEMORY':
        return <HardDrive className="h-5 w-5" />;
      case 'ALREADY_LOADING':
        return <Loader2 className="h-5 w-5 animate-spin" />;
      default:
        return <AlertTriangle className="h-5 w-5" />;
    }
  };

  const getErrorTitle = () => {
    switch (error.code) {
      case 'NETWORK_ERROR':
        return 'Network Error';
      case 'NO_MODELS':
        return 'No Models Available';
      case 'LOAD_FAILED':
        return 'Model Load Failed';
      case 'TIMEOUT':
        return 'Loading Timed Out';
      case 'OUT_OF_MEMORY':
        return 'Insufficient Memory';
      case 'ALREADY_LOADING':
        return 'Model Loading in Progress';
      default:
        return 'Error';
    }
  };

  const getErrorDescription = () => {
    switch (error.code) {
      case 'NETWORK_ERROR':
        return 'Unable to connect to the server. Please check your connection.';
      case 'NO_MODELS':
        return 'No models are available. Please import a model or contact your administrator.';
      case 'TIMEOUT':
        return 'The model is taking too long to load. The server may be busy or the model may be too large.';
      case 'OUT_OF_MEMORY':
        return 'Not enough memory to load this model. Try closing other applications or unloading other models.';
      case 'ALREADY_LOADING':
        return 'A model is already being loaded. Please wait for it to complete.';
      default:
        return error.message;
    }
  };

  // For ALREADY_LOADING, show different variant (info instead of destructive)
  const alertVariant = error.code === 'ALREADY_LOADING' ? 'default' : 'destructive';

  return (
    <div className="flex flex-col items-center justify-center h-full gap-6 p-8">
      <Alert variant={alertVariant} className="max-w-md">
        <div className="flex items-start gap-3">
          {getErrorIcon()}
          <div className="flex-1">
            <AlertTitle>{getErrorTitle()}</AlertTitle>
            <AlertDescription className="mt-2">
              {getErrorDescription()}
            </AlertDescription>
            {error.retryCount > 0 && (
              <p className="text-xs mt-2 opacity-70">
                Attempt {error.retryCount} of 3
              </p>
            )}
          </div>
        </div>
      </Alert>

      <div className="flex gap-3">
        {error.canRetry && (
          <Button onClick={onRetry} disabled={isRetrying} className="gap-2">
            {isRetrying ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <RefreshCw className="h-4 w-4" />
            )}
            {isRetrying ? 'Retrying...' : 'Retry'}
          </Button>
        )}
        <Button variant="outline" onClick={onDismiss}>
          Dismiss
        </Button>
      </div>
    </div>
  );
}

export function OperatorChatLayout({ tenantId }: OperatorChatLayoutProps) {
  const { status } = useModelStatus(tenantId);
  const { isAutoLoading, error, isError, retry, clearError } = useAutoLoadModel(tenantId, true);
  const { data: defaultStack } = useGetDefaultStack(tenantId);

  // Show loading state while auto-loading model
  if (isAutoLoading && !isError) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-4 text-muted-foreground">
        <Loader2 className="h-8 w-8 animate-spin" />
        <div className="text-center">
          <p className="text-lg font-medium">Preparing your workspace...</p>
          <p className="text-sm mt-1">Loading model for inference</p>
        </div>
      </div>
    );
  }

  // Show error state with retry option
  if (isError && error) {
    return (
      <div className="flex flex-col h-full">
        <ModelStatusBar tenantId={tenantId} />
        <ErrorDisplay
          error={error}
          onRetry={retry}
          onDismiss={clearError}
          isRetrying={isAutoLoading}
        />
      </div>
    );
  }

  // Show prompt when no model and not loading
  if (status === 'no-model') {
    return (
      <div className="flex flex-col h-full">
        <ModelStatusBar tenantId={tenantId} />
        <div className="flex-1 flex flex-col items-center justify-center gap-4 text-muted-foreground p-8">
          <MessageSquare className="h-12 w-12 opacity-50" />
          <div className="text-center max-w-md">
            <p className="text-lg font-medium">No model loaded</p>
            <p className="text-sm mt-2">
              Click "Load Model" above to load a model and start chatting.
              If no models are available, contact your administrator.
            </p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <ModelStatusBar tenantId={tenantId} />
      <div className="flex-1 overflow-hidden">
        <ChatInterface
          selectedTenant={tenantId}
          initialStackId={defaultStack?.id}
        />
      </div>
    </div>
  );
}

export default OperatorChatLayout;
