/**
 * SSEErrorBanner - Global banner for SSE connection errors
 *
 * Displays a dismissible banner when any SSE stream has connection issues.
 * Uses the global LiveDataStatus context to aggregate errors from all streams.
 */

import * as React from 'react';
import { AlertTriangle, RefreshCw, X, WifiOff } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { useLiveDataStatus, type OverallConnectionStatus } from '@/hooks/realtime/useLiveDataStatus';

export interface SSEErrorBannerProps {
  /** Additional class names */
  className?: string;
}

export function SSEErrorBanner({ className }: SSEErrorBannerProps) {
  const { overall, streams, totalStreams, reconnectAll } = useLiveDataStatus();
  const [dismissed, setDismissed] = React.useState(false);
  const [isReconnecting, setIsReconnecting] = React.useState(false);

  // Reset dismissed state when status changes to live
  React.useEffect(() => {
    if (overall === 'live') {
      setDismissed(false);
    }
  }, [overall]);

  // Get the first error message from any stream
  const errorMessage = React.useMemo(() => {
    for (const [, status] of Object.entries(streams)) {
      if (status.error) {
        return status.error.message;
      }
    }
    return null;
  }, [streams]);

  // Count disconnected streams
  const disconnectedCount = React.useMemo(() => {
    return Object.values(streams).filter((s) => !s.connected).length;
  }, [streams]);

  // Don't show if:
  // - No streams registered
  // - All streams connected
  // - Banner was dismissed
  // - Status is live or polling (polling is a fallback, not an error)
  const shouldShow =
    totalStreams > 0 &&
    (overall === 'offline' || overall === 'partial') &&
    !dismissed &&
    disconnectedCount > 0;

  if (!shouldShow) {
    return null;
  }

  const handleReconnect = async () => {
    setIsReconnecting(true);
    try {
      reconnectAll();
      // Give it a moment to reconnect before removing the loading state
      await new Promise((resolve) => setTimeout(resolve, 1000));
    } finally {
      setIsReconnecting(false);
    }
  };

  const getStatusConfig = (status: OverallConnectionStatus) => {
    switch (status) {
      case 'offline':
        return {
          icon: WifiOff,
          bg: 'bg-red-50 dark:bg-red-900/20',
          border: 'border-red-200 dark:border-red-800',
          text: 'text-red-800 dark:text-red-200',
          iconColor: 'text-red-600 dark:text-red-400',
          title: 'Connection Lost',
        };
      case 'partial':
        return {
          icon: AlertTriangle,
          bg: 'bg-amber-50 dark:bg-amber-900/20',
          border: 'border-amber-200 dark:border-amber-800',
          text: 'text-amber-800 dark:text-amber-200',
          iconColor: 'text-amber-600 dark:text-amber-400',
          title: 'Partial Connection',
        };
      default:
        return {
          icon: AlertTriangle,
          bg: 'bg-gray-50 dark:bg-gray-900/20',
          border: 'border-gray-200 dark:border-gray-800',
          text: 'text-gray-800 dark:text-gray-200',
          iconColor: 'text-gray-600 dark:text-gray-400',
          title: 'Connection Issue',
        };
    }
  };

  const config = getStatusConfig(overall);
  const Icon = config.icon;

  return (
    <div
      role="alert"
      className={cn(
        'flex items-center justify-between gap-4 px-4 py-2 border-b',
        config.bg,
        config.border,
        className
      )}
    >
      <div className="flex items-center gap-3">
        <Icon className={cn('h-4 w-4 shrink-0', config.iconColor)} />
        <div className={cn('text-sm', config.text)}>
          <span className="font-medium">{config.title}:</span>{' '}
          <span className="opacity-90">
            {errorMessage || `${disconnectedCount} of ${totalStreams} stream${totalStreams > 1 ? 's' : ''} disconnected`}
          </span>
        </div>
      </div>

      <div className="flex items-center gap-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={handleReconnect}
          disabled={isReconnecting}
          className={cn('h-7 px-2 text-xs', config.text)}
        >
          <RefreshCw className={cn('h-3 w-3 mr-1', isReconnecting && 'animate-spin')} />
          {isReconnecting ? 'Reconnecting...' : 'Reconnect'}
        </Button>
        <Button
          variant="ghost"
          size="icon"
          onClick={() => setDismissed(true)}
          className={cn('h-7 w-7', config.text)}
          aria-label="Dismiss"
        >
          <X className="h-3 w-3" />
        </Button>
      </div>
    </div>
  );
}

export default SSEErrorBanner;
