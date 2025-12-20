/**
 * System Health Panel
 *
 * Displays backend health status on the login page.
 * Shows component-level health details with expandable view.
 * Uses skeleton placeholders during loading to prevent layout collapse.
 */

import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Loader2 } from 'lucide-react';
import { Skeleton } from '@/components/ui/skeleton';
import type { UseHealthPollingReturn } from '@/hooks/auth/useHealthPolling';

interface SystemHealthPanelProps {
  health: UseHealthPollingReturn;
}

/** Get color classes for status badges */
function getStatusTone(status: string): string {
  switch (status) {
    case 'healthy':
      return 'bg-emerald-500/10 text-emerald-700 border-emerald-300';
    case 'degraded':
      return 'bg-amber-500/10 text-amber-700 border-amber-300';
    case 'unhealthy':
    case 'issue':
      return 'bg-red-500/10 text-red-700 border-red-300';
    default:
      return 'bg-muted text-muted-foreground border-border';
  }
}

/** Skeleton row for loading state */
function SkeletonComponentRow() {
  return (
    <div className="flex items-center justify-between text-xs">
      <Skeleton className="h-3 w-24" />
      <Skeleton className="h-5 w-16 rounded" />
    </div>
  );
}

/** Default component names shown as skeletons during loading */
const SKELETON_COMPONENTS = ['database', 'worker', 'storage', 'model'];

export function SystemHealthPanel({ health }: SystemHealthPanelProps) {
  const [showDetails, setShowDetails] = useState(false);

  const {
    backendStatus,
    healthError,
    isReady,
    issueComponents,
    allComponents,
    lastUpdated,
    refresh,
  } = health;

  const systemStatus = health.health?.status || health.systemHealth?.status || 'unknown';

  // Build backend updates list for display
  const backendUpdates = [
    {
      title: 'Overall health',
      status: systemStatus,
      message: isReady
        ? 'All critical services are healthy.'
        : 'Waiting for services to report healthy.',
    },
    ...Object.entries(allComponents)
      .slice(0, 4)
      .map(([name, comp]) => ({
        title: name,
        status: comp?.status ?? 'unknown',
        message: comp?.message || 'No detail reported yet.',
      })),
  ];

  return (
    <section className="rounded-lg border bg-card/95 backdrop-blur-sm p-4 space-y-3 text-left shadow-sm min-h-[140px]">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="space-y-0.5">
          <h3 className="text-sm font-semibold">System status</h3>
          <p className="text-xs text-muted-foreground">
            {isReady
              ? 'All systems operational'
              : 'Waiting for services...'}
          </p>
        </div>
        <span
          className={`rounded-full border px-2.5 py-1 text-xs font-medium capitalize ${getStatusTone(systemStatus)}`}
        >
          {systemStatus}
        </span>
      </div>

      {/* Health error message */}
      {healthError && (
        <p className="text-sm text-destructive">{healthError}</p>
      )}

      {/* Issue components list (when not ready) */}
      {backendStatus !== 'ready' && (
        <div className="space-y-1.5">
          {issueComponents.length > 0 ? (
            issueComponents.slice(0, 2).map((item) => (
              <div key={item.name} className="flex items-center justify-between text-xs">
                <span className="text-muted-foreground">{item.name}</span>
                <span className={`capitalize px-1.5 py-0.5 rounded text-xs ${getStatusTone(item.status)}`}>
                  {item.status}
                </span>
              </div>
            ))
          ) : (
            /* Skeleton placeholders when no component data yet */
            SKELETON_COMPONENTS.slice(0, 2).map((name) => (
              <SkeletonComponentRow key={name} />
            ))
          )}
        </div>
      )}

      {/* Action buttons */}
      <div className="flex items-center gap-2 pt-1">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-7 px-2 text-xs"
          onClick={() => setShowDetails((prev) => !prev)}
        >
          {showDetails ? 'Hide' : 'Details'}
        </Button>
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-7 px-2 text-xs"
          onClick={refresh}
          disabled={backendStatus === 'checking'}
        >
          {backendStatus === 'checking' ? (
            <>
              <Loader2 className="mr-1.5 h-3 w-3 animate-spin" />
              Checking
            </>
          ) : (
            'Refresh'
          )}
        </Button>
      </div>

      {/* Expanded details view */}
      {showDetails && (
        <div className="rounded-md border bg-muted/30 p-3 space-y-2 text-left min-h-[120px]">
          {Object.keys(allComponents).length > 0 ? (
            <div className="space-y-2">
              {Object.entries(allComponents).map(([name, comp]) => {
                const compStatus = comp?.status ?? 'unknown';
                return (
                  <div
                    key={name}
                    className="flex items-center justify-between text-xs border-b border-border/40 pb-1.5 last:border-b-0 last:pb-0"
                  >
                    <span className="font-medium">{name}</span>
                    <span className={`capitalize px-1.5 py-0.5 rounded text-xs ${getStatusTone(compStatus)}`}>
                      {compStatus}
                    </span>
                  </div>
                );
              })}
            </div>
          ) : (
            /* Skeleton placeholders to maintain layout during loading */
            <div className="space-y-2">
              <p className="text-xs text-muted-foreground mb-3">
                Discovering services...
              </p>
              {SKELETON_COMPONENTS.map((name) => (
                <div
                  key={name}
                  className="flex items-center justify-between text-xs border-b border-border/40 pb-1.5 last:border-b-0 last:pb-0"
                >
                  <Skeleton className="h-3 w-20" />
                  <Skeleton className="h-5 w-14 rounded" />
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
}
