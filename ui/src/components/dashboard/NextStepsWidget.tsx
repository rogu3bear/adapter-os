import React from 'react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { ArrowRight, AlertCircle, Loader2 } from 'lucide-react';
import { useSystemMetrics } from '@/hooks/system/useSystemMetrics';
import { useTrainingJobs } from '@/hooks/training';
import { useSystemState } from '@/hooks/system/useSystemState';
import { useNextSteps, getPriorityColor } from '@/hooks/tutorial/useNextSteps';
import { useQuery } from '@tanstack/react-query';
import apiClient from '@/api/client';
import type { Adapter } from '@/api/adapter-types';
import { DashboardWidgetFrame, type DashboardWidgetState } from './DashboardWidgetFrame';

interface NextStepsWidgetProps {
  /** Maximum number of actions to display (default: 5) */
  maxActions?: number;
  /** Whether to show priority badges */
  showPriorityBadges?: boolean;
}

export function NextStepsWidget({
  maxActions = 5,
  showPriorityBadges = false
}: NextStepsWidgetProps = {}) {
  // Fetch live system data
  const {
    metrics,
    isLoading: metricsLoading,
    error: metricsError,
    lastUpdated: metricsUpdatedAt,
    refetch: refetchMetrics,
  } = useSystemMetrics('normal', true);
  const {
    data: systemState,
    isLoading: stateLoading,
    error: stateError,
    lastUpdated: stateUpdatedAt,
    refetch: refetchSystemState,
  } = useSystemState({ enabled: true });
  const {
    data: trainingData,
    isLoading: trainingLoading,
    error: trainingError,
    refetch: refetchTrainingJobs,
    dataUpdatedAt: trainingUpdatedAt,
  } = useTrainingJobs({}, { enabled: true });
  const {
    data: adapters,
    isLoading: adaptersLoading,
    refetch: refetchAdapters,
    dataUpdatedAt: adaptersUpdatedAt,
  } = useQuery({
    queryKey: ['adapters'],
    queryFn: (): Promise<Adapter[]> => apiClient.listAdapters(),
    refetchInterval: 10000,
  });

  // Convert training data to format expected by useNextSteps
  const trainingJobs = trainingData?.jobs?.map(job => ({
    id: job.id,
    status: job.status,
    adapter_name: job.adapter_name,
    started_at: job.started_at,
    completed_at: job.completed_at,
    error_message: job.error_message
  })) || [];

  // Detect system errors from metrics
  const systemErrors = [];
  if (metricsError) {
    systemErrors.push({
      message: 'Failed to fetch system metrics',
      severity: 'error' as const,
      source: 'metrics'
    });
  }
  const errorRate = metrics?.error_rate || 0;
  if (errorRate > 0.05) {
    systemErrors.push({
      message: `High error rate detected (${(errorRate * 100).toFixed(1)}%)`,
      severity: 'warning' as const,
      source: 'metrics'
    });
  }
  const cpuUsage = metrics?.cpu_usage_percent || metrics?.cpu_usage || 0;
  const memoryUsage = metrics?.memory_usage_percent || metrics?.memory_usage_pct || 0;
  if (cpuUsage > 90 || memoryUsage > 90) {
    systemErrors.push({
      message: `High resource usage (CPU: ${cpuUsage.toFixed(0)}%, Memory: ${memoryUsage.toFixed(0)}%)`,
      severity: 'warning' as const,
      source: 'system'
    });
  }

  // Generate dynamic recommendations using the hook
  const { actions, counts, hasCritical } = useNextSteps({
    systemState,
    adapters: adapters || [],
    trainingJobs,
    systemErrors
  });

  // Show loading state
  const isLoading = metricsLoading || stateLoading || trainingLoading || adaptersLoading;
  const hasError = Boolean(metricsError || stateError || trainingError);

  // Limit displayed actions
  const displayedActions = actions.slice(0, maxActions);

  const lastUpdatedCandidates = [
    metricsUpdatedAt ?? null,
    stateUpdatedAt ?? null,
    trainingUpdatedAt ? new Date(trainingUpdatedAt) : null,
    adaptersUpdatedAt ? new Date(adaptersUpdatedAt) : null,
  ].filter((d): d is Date => Boolean(d));
  const lastUpdated = lastUpdatedCandidates.sort((a, b) => b.getTime() - a.getTime())[0] || null;

  const handleRefresh = async (): Promise<void> => {
    await Promise.all([
      refetchMetrics(),
      refetchSystemState(),
      refetchTrainingJobs(),
      refetchAdapters(),
    ]);
  };

  const state: DashboardWidgetState = hasError
    ? 'error'
    : isLoading
      ? 'loading'
      : displayedActions.length === 0
        ? 'empty'
        : 'ready';

  return (
    <DashboardWidgetFrame
      title="Next Steps"
      subtitle="Recommended follow-ups based on system signals"
      state={state}
      onRefresh={handleRefresh}
      onRetry={handleRefresh}
      lastUpdated={lastUpdated}
      errorMessage={
        hasError
          ? (metricsError || stateError || trainingError)?.toString() || 'Failed to load signals'
          : undefined
      }
      emptyMessage="All systems operational. No immediate actions needed."
      headerRight={
        hasCritical ? (
          <Badge variant="destructive" className="text-xs">
            {counts.critical} Critical
          </Badge>
        ) : null
      }
      loadingContent={
        <div className="flex items-center gap-2 p-4 text-sm text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin" />
          <span>Analyzing system state...</span>
        </div>
      }
    >
      <div className="space-y-3">
        {displayedActions.map((action) => {
          const Icon = action.icon;
          return (
            <div
              key={action.id}
              className={`flex items-start gap-3 p-3 border-l-4 rounded-r-lg bg-muted/50 hover:bg-muted transition-colors ${getPriorityColor(action.priority)}`}
            >
              <Icon className="h-5 w-5 text-muted-foreground mt-0.5 flex-shrink-0" />
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-0.5">
                  <h4 className="font-medium text-sm">{action.title}</h4>
                  {showPriorityBadges && (
                    <Badge
                      variant={action.priority === 'critical' ? 'destructive' : 'secondary'}
                      className="text-xs capitalize"
                    >
                      {action.priority}
                    </Badge>
                  )}
                </div>
                <p className="text-xs text-muted-foreground">{action.description}</p>
              </div>
              <Button
                size="sm"
                variant="ghost"
                onClick={action.action}
                className="flex-shrink-0"
                title={`Go to ${action.title}`}
              >
                <ArrowRight className="h-4 w-4" />
              </Button>
            </div>
          );
        })}
      </div>
      {actions.length > maxActions && (
        <div className="mt-3 text-center">
          <p className="text-xs text-muted-foreground">
            {actions.length - maxActions} more recommendation{actions.length - maxActions > 1 ? 's' : ''} available
          </p>
        </div>
      )}
    </DashboardWidgetFrame>
  );
}
