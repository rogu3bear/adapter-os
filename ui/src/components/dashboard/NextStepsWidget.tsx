import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { ArrowRight, AlertCircle, Loader2 } from 'lucide-react';
import { useSystemMetrics } from '@/hooks/useSystemMetrics';
import { useTrainingJobs } from '@/hooks/useTraining';
import { useSystemState } from '@/hooks/useSystemState';
import { useNextSteps, getPriorityColor } from '@/hooks/useNextSteps';
import { useQuery } from '@tanstack/react-query';
import apiClient from '@/api/client';
import type { Adapter } from '@/api/adapter-types';

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
  const { metrics, isLoading: metricsLoading, error: metricsError } = useSystemMetrics('normal', true);
  const { data: systemState, isLoading: stateLoading } = useSystemState({ enabled: true });
  const { data: trainingData, isLoading: trainingLoading } = useTrainingJobs({}, { enabled: true });
  const { data: adapters, isLoading: adaptersLoading } = useQuery<Adapter[]>({
    queryKey: ['adapters'],
    queryFn: () => apiClient.listAdapters(),
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

  // Limit displayed actions
  const displayedActions = actions.slice(0, maxActions);

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
        <CardTitle className="text-base font-medium">Next Steps</CardTitle>
        {hasCritical && (
          <Badge variant="destructive" className="text-xs">
            {counts.critical} Critical
          </Badge>
        )}
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <div className="flex items-center gap-2 p-4 text-sm text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            <span>Analyzing system state...</span>
          </div>
        ) : displayedActions.length === 0 ? (
          <div className="flex items-center gap-2 p-4 text-sm text-muted-foreground">
            <AlertCircle className="h-4 w-4" />
            <span>All systems operational. No immediate actions needed.</span>
          </div>
        ) : (
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
        )}
        {actions.length > maxActions && (
          <div className="mt-3 text-center">
            <p className="text-xs text-muted-foreground">
              {actions.length - maxActions} more recommendation{actions.length - maxActions > 1 ? 's' : ''} available
            </p>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
