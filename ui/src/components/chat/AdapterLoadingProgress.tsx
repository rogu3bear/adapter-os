/**
 * AdapterLoadingProgress - Inline progress display during adapter loading
 *
 * Shows loading progress for adapters being warmed up before chat.
 */

import * as React from 'react';
import { Loader2, CheckCircle, XCircle, AlertTriangle } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Progress } from '@/components/ui/progress';
import { Button } from '@/components/ui/button';
import { useLiveData } from '@/hooks/useLiveData';
import type { AdapterStreamEvent, AdapterStateTransitionEvent } from '@/api/streaming-types';

// ============================================================================
// Types
// ============================================================================

export type LoadingStatus = 'pending' | 'loading' | 'ready' | 'failed';

export interface AdapterLoadingItem {
  id: string;
  name: string;
  status: LoadingStatus;
  progress?: number;
  estimatedTimeRemaining?: number;
  error?: string;
}

export interface AdapterLoadingProgressProps {
  /** List of adapters being loaded */
  adapters: AdapterLoadingItem[];

  /** Called when all adapters are ready */
  onComplete?: () => void;

  /** Called when user cancels loading */
  onCancel?: () => void;

  /** Called when user retries failed adapters */
  onRetry?: (failedIds: string[]) => void;

  /** Additional class names */
  className?: string;
}

// ============================================================================
// Component
// ============================================================================

export function AdapterLoadingProgress({
  adapters,
  onComplete,
  onCancel,
  onRetry,
  className,
}: AdapterLoadingProgressProps) {
  const [loadingItems, setLoadingItems] = React.useState<Map<string, AdapterLoadingItem>>(
    new Map(adapters.map((a) => [a.id, a]))
  );

  // Update when adapters prop changes
  React.useEffect(() => {
    setLoadingItems(new Map(adapters.map((a) => [a.id, a])));
  }, [adapters]);

  // Subscribe to adapter state transitions
  useLiveData({
    sseEndpoint: '/v1/stream/adapters',
    sseEventType: 'adapters',
    fetchFn: async () => {
      // No polling fallback for adapter state transitions - SSE only
      return null;
    },
    enabled: true,
    pollingSpeed: 'fast',
    onSSEMessage: (event) => {
      const adapterEvent = event as AdapterStreamEvent;
      if (adapterEvent && 'current_state' in adapterEvent) {
        const transition = adapterEvent as AdapterStateTransitionEvent;
        setLoadingItems((prev) => {
          const updated = new Map(prev);
          const existing = updated.get(transition.adapter_id);
          if (existing) {
            const isReady = transition.current_state === 'hot' || transition.current_state === 'warm' || transition.current_state === 'resident';
            updated.set(transition.adapter_id, {
              ...existing,
              status: isReady ? 'ready' : existing.status,
              progress: isReady ? 100 : existing.progress,
            });
          }
          return updated;
        });
      }
    },
  });

  // Calculate overall progress
  const items = Array.from(loadingItems.values());
  const readyCount = items.filter((a) => a.status === 'ready').length;
  const failedCount = items.filter((a) => a.status === 'failed').length;
  const totalCount = items.length;
  const overallProgress = totalCount > 0 ? Math.round((readyCount / totalCount) * 100) : 0;
  const allComplete = readyCount === totalCount;
  const hasFailed = failedCount > 0;

  // Notify when complete
  React.useEffect(() => {
    if (allComplete && onComplete) {
      onComplete();
    }
  }, [allComplete, onComplete]);

  // Calculate total estimated time remaining
  const totalEta = items
    .filter((a) => a.status === 'loading' && a.estimatedTimeRemaining)
    .reduce((acc, a) => acc + (a.estimatedTimeRemaining || 0), 0);

  const failedIds = items.filter((a) => a.status === 'failed').map((a) => a.id);

  return (
    <div
      className={cn(
        'rounded-lg border bg-slate-50 p-4 space-y-3',
        allComplete && 'bg-green-50 border-green-200',
        hasFailed && !allComplete && 'bg-amber-50 border-amber-200',
        className
      )}
    >
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          {allComplete ? (
            <>
              <CheckCircle className="h-5 w-5 text-green-600" />
              <span className="font-medium text-green-700">All adapters ready!</span>
            </>
          ) : hasFailed ? (
            <>
              <AlertTriangle className="h-5 w-5 text-amber-600" />
              <span className="font-medium text-amber-700">
                {failedCount} adapter{failedCount > 1 ? 's' : ''} failed to load
              </span>
            </>
          ) : (
            <>
              <Loader2 className="h-5 w-5 animate-spin text-blue-600" />
              <span className="font-medium text-slate-700">Loading adapters...</span>
            </>
          )}
        </div>

        {!allComplete && onCancel && (
          <Button variant="ghost" size="sm" onClick={onCancel}>
            Cancel
          </Button>
        )}
      </div>

      {/* Overall progress bar */}
      {!allComplete && (
        <div className="space-y-1">
          <Progress value={overallProgress} className="h-2" />
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>{overallProgress}% complete</span>
            {totalEta > 0 && <span>~{totalEta}s remaining</span>}
          </div>
        </div>
      )}

      {/* Individual adapter status */}
      <div className="space-y-2">
        {items.map((item) => (
          <div
            key={item.id}
            className={cn(
              'flex items-center justify-between px-3 py-2 rounded-md border',
              item.status === 'ready' && 'bg-green-100 border-green-200',
              item.status === 'loading' && 'bg-blue-50 border-blue-200',
              item.status === 'failed' && 'bg-red-50 border-red-200',
              item.status === 'pending' && 'bg-gray-50 border-gray-200'
            )}
          >
            <div className="flex items-center gap-2">
              {item.status === 'ready' && <CheckCircle className="h-4 w-4 text-green-600" />}
              {item.status === 'loading' && (
                <Loader2 className="h-4 w-4 animate-spin text-blue-600" />
              )}
              {item.status === 'failed' && <XCircle className="h-4 w-4 text-red-600" />}
              {item.status === 'pending' && (
                <div className="h-4 w-4 rounded-full border-2 border-gray-300" />
              )}
              <span className="text-sm font-medium">{item.name}</span>
            </div>

            <div className="flex items-center gap-2">
              {item.status === 'loading' && item.progress !== undefined && (
                <span className="text-xs text-blue-600">{item.progress}%</span>
              )}
              {item.status === 'loading' && item.estimatedTimeRemaining !== undefined && (
                <span className="text-xs text-muted-foreground">
                  {item.estimatedTimeRemaining}s
                </span>
              )}
              {item.status === 'ready' && (
                <span className="text-xs text-green-600">Ready</span>
              )}
              {item.status === 'failed' && (
                <span className="text-xs text-red-600">{item.error || 'Failed'}</span>
              )}
              {item.status === 'pending' && (
                <span className="text-xs text-muted-foreground">Pending</span>
              )}
            </div>
          </div>
        ))}
      </div>

      {/* Retry button for failed adapters */}
      {hasFailed && onRetry && (
        <div className="flex justify-end">
          <Button variant="outline" size="sm" onClick={() => onRetry(failedIds)}>
            Retry Failed
          </Button>
        </div>
      )}
    </div>
  );
}

export default AdapterLoadingProgress;
