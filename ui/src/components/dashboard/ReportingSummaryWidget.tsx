// 【ui/src/components/dashboard/ReportingSummaryWidget.tsx§23-48】 - Replace manual polling with standardized hook
import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { FileText, TrendingUp, Activity, AlertCircle } from 'lucide-react';
import { Link } from 'react-router-dom';
import { Button } from '../ui/button';
import apiClient from '../../api/client';
import { logger, toError } from '../../utils/logger';
import { usePolling } from '../../hooks/usePolling';
import { LastUpdated } from '../ui/last-updated';
import { ErrorRecovery, errorRecoveryTemplates } from '../ui/error-recovery';

interface ReportingSummaryWidgetProps {
  selectedTenant?: string;
}

export function ReportingSummaryWidget({ selectedTenant }: ReportingSummaryWidgetProps) {
  const [metrics, setMetrics] = React.useState({
    totalInferences: 0,
    totalTrainingJobs: 0,
    activeAdapters: 0,
    systemHealth: 0,
  });
  const [error, setError] = React.useState<Error | null>(null);

  // 【ui/src/hooks/usePolling.ts】 - Standardized polling hook for reporting metrics
  const fetchReportingData = async () => {
    const [systemMetrics, adapters, trainingJobsResponse] = await Promise.all([
      apiClient.getSystemMetrics().catch(() => null),
      apiClient.listAdapters().catch(() => []),
      apiClient.listTrainingJobs().catch(() => ({ jobs: [], total: 0, page: 1, page_size: 0 })),
    ]);

    const trainingJobs = trainingJobsResponse?.jobs || [];
    return {
      totalInferences: systemMetrics?.active_sessions || 0,
      totalTrainingJobs: trainingJobs.length,
      activeAdapters: adapters.filter((a: any) => ['hot', 'warm'].includes(a.current_state)).length,
      systemHealth: systemMetrics?.memory_usage_pct || 0,
    };
  };

  const {
    data: reportingData,
    isLoading: loading,
    lastUpdated,
    error: pollingError,
    refetch: refreshMetrics
  } = usePolling(
    fetchReportingData,
    'slow', // Background updates for reporting
    {
      operationName: 'ReportingSummaryWidget.fetchReportingData',
      showLoadingIndicator: false,
      onError: (err) => {
        const error = err instanceof Error ? err : new Error('Failed to fetch reporting metrics');
        setError(error);
        logger.error('Failed to fetch reporting metrics', { component: 'ReportingSummaryWidget' }, err);
      }
    }
  );

  // Update metrics when polling data arrives
  React.useEffect(() => {
    if (reportingData) {
      setMetrics(reportingData);
      setError(null);
    }
  }, [reportingData]);

  if (error) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <FileText className="h-5 w-5" />
            Reporting Summary
          </CardTitle>
        </CardHeader>
        <CardContent>
          <ErrorRecovery
            error={error.message}
            onRetry={() => refreshMetrics()}
          />
        </CardContent>
      </Card>
    );
  }

  if (loading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Reporting Summary</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="h-32 animate-pulse bg-muted rounded" />
        </CardContent>
      </Card>
    );
  }

  const healthColor = metrics.systemHealth < 70 ? 'text-green-600' : metrics.systemHealth < 90 ? 'text-yellow-600' : 'text-red-600';
  const healthStatus = metrics.systemHealth < 70 ? 'Healthy' : metrics.systemHealth < 90 ? 'Warning' : 'Critical';

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <FileText className="h-5 w-5" />
          Reporting Summary
        </CardTitle>
        {lastUpdated && <LastUpdated timestamp={lastUpdated} className="mt-1" />}
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Quick Stats */}
        <div className="grid grid-cols-2 gap-4">
          <div className="p-3 bg-muted rounded-lg">
            <div className="flex items-center gap-2 mb-1">
              <Activity className="h-4 w-4 text-muted-foreground" />
              <span className="text-xs text-muted-foreground">Inferences</span>
            </div>
            <p className="text-2xl font-bold">{metrics.totalInferences.toLocaleString()}</p>
          </div>
          <div className="p-3 bg-muted rounded-lg">
            <div className="flex items-center gap-2 mb-1">
              <TrendingUp className="h-4 w-4 text-muted-foreground" />
              <span className="text-xs text-muted-foreground">Training Jobs</span>
            </div>
            <p className="text-2xl font-bold">{metrics.totalTrainingJobs}</p>
          </div>
          <div className="p-3 bg-muted rounded-lg">
            <div className="flex items-center gap-2 mb-1">
              <Activity className="h-4 w-4 text-muted-foreground" />
              <span className="text-xs text-muted-foreground">Active Adapters</span>
            </div>
            <p className="text-2xl font-bold">{metrics.activeAdapters}</p>
          </div>
          <div className="p-3 bg-muted rounded-lg">
            <div className="flex items-center gap-2 mb-1">
              <AlertCircle className={`h-4 w-4 ${healthColor}`} />
              <span className="text-xs text-muted-foreground">System Health</span>
            </div>
            <div className="flex items-center gap-2">
              <p className="text-2xl font-bold">{metrics.systemHealth}%</p>
              <Badge variant={metrics.systemHealth < 70 ? 'default' : metrics.systemHealth < 90 ? 'secondary' : 'destructive'}>
                {healthStatus}
              </Badge>
            </div>
          </div>
        </div>

        {/* Quick Actions */}
        <div className="flex flex-col gap-2 pt-2 border-t">
          <Link to="/monitoring">
            <Button variant="outline" className="w-full justify-start">
              <FileText className="h-4 w-4 mr-2" />
              View Detailed Reports
            </Button>
          </Link>
          <Link to="/telemetry">
            <Button variant="outline" className="w-full justify-start">
              <Activity className="h-4 w-4 mr-2" />
              System Telemetry
            </Button>
          </Link>
        </div>
      </CardContent>
    </Card>
  );
}

