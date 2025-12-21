/**
 * Dashboard widget showing aggregate model cache health across workers.
 *
 * Displays:
 * - Summary badges (N healthy, N warning, N critical)
 * - Per-worker utilization bars
 * - Remediation tips when warnings are present
 */

import React from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { AlertTriangle, AlertCircle, CheckCircle, HardDrive, ExternalLink } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useWorkers } from '@/hooks/system/useSystemMetrics';
import { useWorkerCacheHealth, type CacheHealthSummary } from '@/hooks/workers/useWorkerCacheHealth';
import { DashboardWidgetFrame, type DashboardWidgetState } from './DashboardWidgetFrame';
import type { WorkerCacheHealth } from '@/api/types';

export function CacheHealthWidget() {
  const navigate = useNavigate();
  const { workers, isLoading, error, refetch, lastUpdated } = useWorkers(undefined, undefined, 'normal');
  const { workerHealth, summary } = useWorkerCacheHealth(workers);

  const widgetState: DashboardWidgetState = error
    ? 'error'
    : isLoading
    ? 'loading'
    : workers && workers.length > 0
    ? 'ready'
    : 'empty';

  const getStatusIcon = (status: WorkerCacheHealth['status']) => {
    switch (status) {
      case 'critical':
        return <AlertTriangle className="h-4 w-4 text-destructive" />;
      case 'warning':
        return <AlertCircle className="h-4 w-4 text-amber-500" />;
      default:
        return <CheckCircle className="h-4 w-4 text-emerald-500" />;
    }
  };

  const getProgressColor = (status: WorkerCacheHealth['status']) => {
    switch (status) {
      case 'critical':
        return 'bg-destructive';
      case 'warning':
        return 'bg-amber-500';
      default:
        return 'bg-emerald-500';
    }
  };

  const hasWarnings = summary.warning > 0 || summary.critical > 0;

  return (
    <DashboardWidgetFrame
      title={
        <span className="flex items-center gap-2">
          <HardDrive className="h-5 w-5" />
          Model Cache Health
        </span>
      }
      subtitle="Worker memory cache utilization"
      state={widgetState}
      lastUpdated={lastUpdated}
      onRefresh={refetch}
      headerRight={
        <Button
          variant="outline"
          size="sm"
          onClick={() => navigate('/system/workers')}
        >
          <ExternalLink className="h-4 w-4 mr-1" />
          View Workers
        </Button>
      }
      emptyMessage="No workers running"
    >
      <div className="space-y-4">
        {/* Summary badges */}
        <div className="flex flex-wrap gap-2">
          {summary.healthy > 0 && (
            <Badge variant="outline" className="bg-emerald-50 text-emerald-700 border-emerald-200">
              <CheckCircle className="h-3 w-3 mr-1" />
              {summary.healthy} Healthy
            </Badge>
          )}
          {summary.warning > 0 && (
            <Badge variant="outline" className="bg-amber-50 text-amber-700 border-amber-200">
              <AlertCircle className="h-3 w-3 mr-1" />
              {summary.warning} Warning
            </Badge>
          )}
          {summary.critical > 0 && (
            <Badge variant="destructive">
              <AlertTriangle className="h-3 w-3 mr-1" />
              {summary.critical} Critical
            </Badge>
          )}
          {summary.unknown > 0 && (
            <Badge variant="secondary">
              {summary.unknown} Unknown
            </Badge>
          )}
        </div>

        {/* Per-worker utilization bars (show top 5 worst) */}
        {workerHealth.length > 0 && (
          <div className="space-y-3">
            <div className="text-sm font-medium text-muted-foreground">
              Worker Utilization
            </div>
            {workerHealth
              .sort((a, b) => b.utilization_pct - a.utilization_pct)
              .slice(0, 5)
              .map((health) => (
                <div key={health.worker_id} className="space-y-1">
                  <div className="flex items-center justify-between text-sm">
                    <span className="flex items-center gap-2">
                      {getStatusIcon(health.status)}
                      <span className="font-mono text-xs truncate max-w-[120px]" title={health.worker_id}>
                        {health.worker_id.slice(0, 8)}...
                      </span>
                    </span>
                    <span className="text-muted-foreground">
                      {health.cache_used_mb}/{health.cache_max_mb} MB ({health.utilization_pct}%)
                    </span>
                  </div>
                  <div className="h-2 bg-muted rounded-full overflow-hidden">
                    <div
                      className={`h-full transition-all ${getProgressColor(health.status)}`}
                      style={{ width: `${health.utilization_pct}%` }}
                    />
                  </div>
                </div>
              ))}
          </div>
        )}

        {/* Remediation tips */}
        {hasWarnings && (
          <div className="rounded-md bg-amber-50 dark:bg-amber-950/30 p-3 text-sm">
            <div className="font-medium text-amber-800 dark:text-amber-200 mb-1">
              Cache Pressure Detected
            </div>
            <ul className="text-amber-700 dark:text-amber-300 list-disc list-inside space-y-1">
              <li>Consider increasing <code className="bg-amber-100 dark:bg-amber-900 px-1 rounded">AOS_MODEL_CACHE_MAX_MB</code></li>
              <li>Review pinned adapters that block eviction</li>
              <li>Reduce concurrent inference requests</li>
            </ul>
          </div>
        )}
      </div>
    </DashboardWidgetFrame>
  );
}
