/**
 * SystemNotReadyBanner - Alert banner for 503 System Not Ready errors
 *
 * Displays a "system is starting up" message with auto-retry countdown.
 *
 * 【2025-01-20†ui-never-spins-forever】
 */

import { AlertTriangle, RefreshCw, Loader2 } from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

export interface SystemNotReadyBannerProps {
  /** Called when user clicks "Check Now" */
  onRetry: () => void;
  /** Whether auto-retry is active */
  isAutoRetrying?: boolean;
  /** Seconds until next retry (for countdown display) */
  nextRetryInSeconds?: number | null;
  /** Whether manual retry is in progress */
  isRetrying?: boolean;
  /** Additional class names */
  className?: string;
}

export function SystemNotReadyBanner({
  onRetry,
  isAutoRetrying = false,
  nextRetryInSeconds,
  isRetrying = false,
  className,
}: SystemNotReadyBannerProps) {
  return (
    <div
      className={cn(
        'flex min-h-[400px] items-center justify-center p-4',
        className
      )}
    >
      <Card className="mx-auto w-full max-w-lg border-amber-200 bg-amber-50/50" role="alert" aria-live="polite">
        <CardHeader>
          <div className="flex items-start gap-3">
            <div className="mt-0.5 flex h-9 w-9 items-center justify-center rounded-full bg-amber-100">
              {isAutoRetrying ? (
                <Loader2 className="h-5 w-5 text-amber-600 animate-spin" aria-hidden="true" />
              ) : (
                <AlertTriangle className="h-5 w-5 text-amber-600" aria-hidden="true" />
              )}
            </div>
            <div className="space-y-1">
              <CardTitle className="text-lg text-amber-900">System Not Ready</CardTitle>
              <CardDescription className="text-amber-700">
                The system is starting up. Some services are not yet available.
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-sm text-amber-800">
            This typically resolves within 30-60 seconds after startup.
            The system will automatically check again when ready.
          </p>

          {isAutoRetrying && nextRetryInSeconds != null && nextRetryInSeconds > 0 && (
            <div className="flex items-center gap-2 text-sm text-amber-700">
              <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
              <span>
                Retrying in {nextRetryInSeconds} second{nextRetryInSeconds !== 1 ? 's' : ''}...
              </span>
            </div>
          )}

          <div className="flex flex-wrap gap-2 pt-2">
            <Button
              variant="outline"
              onClick={onRetry}
              disabled={isRetrying}
              className="gap-2 border-amber-300 hover:bg-amber-100"
            >
              <RefreshCw
                className={cn('h-4 w-4', isRetrying && 'animate-spin')}
                aria-hidden="true"
              />
              {isRetrying ? 'Checking...' : 'Check Now'}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

export default SystemNotReadyBanner;
