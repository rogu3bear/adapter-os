// 【ui/src/components/UserReportsPage.tsx§38-80】 - Replace manual polling with standardized hook
import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import apiClient from '../api/client';
import { logger, toError } from '../utils/logger';
import { usePolling } from '../hooks/usePolling';
import { useTenant } from '@/layout/LayoutProvider';
import { LastUpdated } from './ui/last-updated';
import {
  Activity,
  Clock,
  TrendingUp,
  CheckCircle,
  XCircle,
  Zap,
  FileText,
  Download,
  BarChart3,
  Calendar
} from 'lucide-react';
import type { 
  SystemMetrics, 
  TrainingJob,
  InferenceSession,
  TelemetryEvent,
  Adapter
} from '@/api/types';
import { KpiGrid } from './ui/grid';

interface UserReportsPageProps {
  tenantId?: string;
}

export function UserReportsPage({ tenantId }: UserReportsPageProps) {
  const { selectedTenant } = useTenant();
  const effectiveTenant = tenantId || selectedTenant;
  const [recentActivity, setRecentActivity] = useState<TelemetryEvent[]>([]);

  // 【ui/src/hooks/usePolling.ts】 - Standardized polling hook
  const fetchReportData = useCallback(async () => {
    const [metricsRes, trainingRes, adaptersRes, activityRes] = await Promise.all([
      apiClient.getSystemMetrics().catch(() => null),
      apiClient.listTrainingJobs().catch(() => []),
      apiClient.listAdapters().catch(() => []),
      // Fetch recent telemetry events from API
      effectiveTenant
        ? apiClient.getTelemetryEvents({
            limit: 20, // Show last 20 events in reports page
            tenantId: effectiveTenant,
            startTime: new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString(), // Last 24 hours
          }).catch(() => []) // Graceful degradation: return empty array on error
        : Promise.resolve([]), // Return empty array if no tenant selected
    ]);

    return {
      metrics: metricsRes,
      recentTraining: trainingRes.slice(0, 5),
      adapters: adaptersRes,
      activity: activityRes // Real telemetry events from API
    };
  }, [effectiveTenant]);

  const { 
    data, 
    isLoading: loading, 
    lastUpdated,
    error: pollingError 
  } = usePolling(
    fetchReportData,
    'slow', // Background updates (reports)
    {
      enabled: !!effectiveTenant, // Only poll when tenant is available
      showLoadingIndicator: true,
      onError: (err) => {
        logger.error('Failed to fetch user report data', { 
          component: 'UserReportsPage', 
          operation: 'fetchReportData',
          tenantId: effectiveTenant,
        }, err);
      }
    }
  );

  const metrics = data?.metrics || null;
  const recentTraining = data?.recentTraining || [];
  const adapters = data?.adapters || [];

  useEffect(() => {
    if (data?.activity) {
      setRecentActivity(data.activity);
    }
  }, [data]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-96">
        <div className="text-center">
          <Activity className="w-8 h-8 animate-spin text-blue-500 mx-auto mb-2" />
          <p className="text-sm text-muted-foreground">Loading reports...</p>
        </div>
      </div>
    );
  }

  const completedTraining = recentTraining.filter(j => j.status === 'completed').length;
  const failedTraining = recentTraining.filter(j => j.status === 'failed').length;
  const activeAdapters = adapters.filter(a => a.active).length;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-3xl font-bold">Reports & Activity</h1>
        <p className="text-muted-foreground">
          Overview of your recent activity and system usage
        </p>
      </div>

      {/* Key Metrics */}
      <KpiGrid>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Active Adapters</CardTitle>
            <Zap className="w-4 h-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{activeAdapters}</div>
            <p className="text-xs text-muted-foreground mt-1">
              {adapters.length} total registered
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Training Jobs</CardTitle>
            <TrendingUp className="w-4 h-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{recentTraining.length}</div>
            <p className="text-xs text-muted-foreground mt-1">
              {completedTraining} completed, {failedTraining} failed
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Avg. Latency</CardTitle>
            <Clock className="w-4 h-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {metrics?.latency_p95_ms?.toFixed(0) || '0'}ms
            </div>
            <p className="text-xs text-muted-foreground mt-1">
              95th percentile
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Throughput</CardTitle>
            <Activity className="w-4 h-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {metrics?.tokens_per_sec?.toFixed(1) || '0'}
            </div>
            <p className="text-xs text-muted-foreground mt-1">
              tokens/second
            </p>
          </CardContent>
        </Card>
      </KpiGrid>

      {/* Recent Training Jobs */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <BarChart3 className="w-5 h-5" />
            Recent Training Jobs
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            {recentTraining.length > 0 ? (
              recentTraining.map(job => (
                <div
                  key={job.id}
                  className="flex items-center justify-between p-4 border rounded-lg hover:bg-accent transition-colors"
                >
                  <div className="flex items-center gap-4 flex-1">
                    <div className="flex-shrink-0">
                      {job.status === 'completed' ? (
                        <CheckCircle className="w-5 h-5 text-green-500" />
                      ) : job.status === 'failed' ? (
                        <XCircle className="w-5 h-5 text-red-500" />
                      ) : job.status === 'running' ? (
                        <Activity className="w-5 h-5 text-blue-500 animate-pulse" />
                      ) : (
                        <Clock className="w-5 h-5 text-gray-400" />
                      )}
                    </div>
                    <div className="flex-1 min-w-0">
                      <p className="font-medium truncate">{job.adapter_name}</p>
                      <p className="text-sm text-muted-foreground">
                        {new Date(job.created_at).toLocaleDateString()} at{' '}
                        {new Date(job.created_at).toLocaleTimeString()}
                      </p>
                    </div>
                    {job.progress_pct !== undefined && job.status === 'running' && (
                      <div className="w-32">
                        <div className="text-xs text-right mb-1">{job.progress_pct}%</div>
                        <div className="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
                          <div
                            className="bg-blue-600 h-2 rounded-full transition-all"
                            style={{ width: `${job.progress_pct}%` }}
                          />
                        </div>
                      </div>
                    )}
                  </div>
                  <div className="ml-4">
                    <Badge
                      variant={
                        job.status === 'completed'
                          ? 'default'
                          : job.status === 'failed'
                          ? 'destructive'
                          : 'secondary'
                      }
                    >
                      {job.status}
                    </Badge>
                  </div>
                </div>
              ))
            ) : (
              <div className="text-center py-8">
                <FileText className="w-12 h-12 text-muted-foreground mx-auto mb-3" />
                <p className="text-sm text-muted-foreground">No training jobs yet</p>
                <Button variant="link" className="mt-2">
                  Start Training
                </Button>
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Recent Activity */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Calendar className="w-5 h-5" />
            Recent Activity
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            {recentActivity.length > 0 ? (
              recentActivity.map(event => (
                <div
                  key={event.id}
                  className="flex items-start gap-3 p-3 border rounded-lg"
                >
                  <div className="flex-shrink-0 mt-1">
                    {event.level === 'error' ? (
                      <XCircle className="w-4 h-4 text-red-500" />
                    ) : event.level === 'warning' ? (
                      <Activity className="w-4 h-4 text-yellow-500" />
                    ) : (
                      <CheckCircle className="w-4 h-4 text-green-500" />
                    )}
                  </div>
                  <div className="flex-1 min-w-0">
                    <p className="text-sm font-medium">{event.message}</p>
                    <div className="flex items-center gap-2 mt-1">
                      <Badge variant="outline" className="text-xs">
                        {event.component}
                      </Badge>
                      <span className="text-xs text-muted-foreground">
                        {new Date(event.timestamp).toLocaleString()}
                      </span>
                    </div>
                  </div>
                </div>
              ))
            ) : (
              <p className="text-sm text-muted-foreground text-center py-4">
                No recent activity
              </p>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Export Options */}
      <Card>
        <CardHeader>
          <CardTitle>Export Reports</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex flex-wrap gap-3">
            <Button variant="outline">
              <Download className="w-4 h-4 mr-2" />
              Export Training History
            </Button>
            <Button variant="outline">
              <Download className="w-4 h-4 mr-2" />
              Export Activity Log
            </Button>
            <Button variant="outline">
              <Download className="w-4 h-4 mr-2" />
              Export Metrics Summary
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

