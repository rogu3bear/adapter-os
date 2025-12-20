/**
 * NoWorkersPanel - Guidance panel when no workers are available
 *
 * Displays instructions on how to start a worker with actionable command.
 *
 * 【2025-01-20†ui-never-spins-forever】
 */

import { AlertTriangle, RefreshCw, Terminal } from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

export interface NoWorkersPanelProps {
  /** Called when user clicks "Check Again" */
  onRetry: () => void;
  /** Whether retry is in progress */
  isRetrying?: boolean;
  /** Additional class names */
  className?: string;
}

export function NoWorkersPanel({
  onRetry,
  isRetrying = false,
  className,
}: NoWorkersPanelProps) {
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
              <AlertTriangle className="h-5 w-5 text-amber-600" aria-hidden="true" />
            </div>
            <div className="space-y-1">
              <CardTitle className="text-lg">No Workers Available</CardTitle>
              <CardDescription>
                The inference service has no active workers.
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-sm text-muted-foreground">
            Chat requires at least one running worker to handle inference requests.
            Start a worker using the command below.
          </p>

          <div className="space-y-2">
            <div className="flex items-center gap-2 text-sm font-medium text-foreground">
              <Terminal className="h-4 w-4" aria-hidden="true" />
              <span>To start a worker:</span>
            </div>
            <code className="block rounded-md bg-muted px-3 py-2 font-mono text-sm">
              ./start --worker
            </code>
          </div>

          <p className="text-xs text-muted-foreground">
            Alternatively, use <code className="rounded bg-muted px-1">make dev</code> to start
            both the control plane and a worker.
          </p>

          <div className="flex flex-wrap gap-2 pt-2">
            <Button
              variant="outline"
              onClick={onRetry}
              disabled={isRetrying}
              className="gap-2"
            >
              <RefreshCw
                className={cn('h-4 w-4', isRetrying && 'animate-spin')}
                aria-hidden="true"
              />
              {isRetrying ? 'Checking...' : 'Check Again'}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

export default NoWorkersPanel;
