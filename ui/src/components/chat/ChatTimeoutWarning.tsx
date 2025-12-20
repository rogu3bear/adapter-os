/**
 * ChatTimeoutWarning - Warning card shown when initial load takes too long
 *
 * Displays a "taking longer than expected" message with a retry button.
 *
 * 【2025-01-20†ui-never-spins-forever】
 */

import { Clock, RefreshCw } from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

export interface ChatTimeoutWarningProps {
  /** Called when user clicks "Retry All" */
  onRetry: () => void;
  /** Whether retry is in progress */
  isRetrying?: boolean;
  /** Additional class names */
  className?: string;
}

export function ChatTimeoutWarning({
  onRetry,
  isRetrying = false,
  className,
}: ChatTimeoutWarningProps) {
  return (
    <div
      className={cn(
        'flex min-h-[400px] items-center justify-center p-4',
        className
      )}
    >
      <Card className="mx-auto w-full max-w-lg" role="alert" aria-live="polite">
        <CardHeader>
          <div className="flex items-start gap-3">
            <div className="mt-0.5 flex h-9 w-9 items-center justify-center rounded-full bg-amber-100">
              <Clock className="h-5 w-5 text-amber-600" aria-hidden="true" />
            </div>
            <div className="space-y-1">
              <CardTitle className="text-lg">Taking Longer Than Expected</CardTitle>
              <CardDescription>
                Initial data is still loading. This may indicate backend issues.
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-sm text-muted-foreground">
            The chat interface requires data from the server to initialize. If this persists,
            check that the AdapterOS backend is running and accessible.
          </p>

          <div className="flex flex-wrap gap-2">
            <Button
              onClick={onRetry}
              disabled={isRetrying}
              className="gap-2"
            >
              <RefreshCw
                className={cn('h-4 w-4', isRetrying && 'animate-spin')}
                aria-hidden="true"
              />
              {isRetrying ? 'Retrying...' : 'Retry All'}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

export default ChatTimeoutWarning;
