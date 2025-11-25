import React from 'react';
import { Link } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Skeleton } from '@/components/ui/skeleton';
import { ContentGrid, KpiGrid } from '@/components/ui/grid';
import { PageHeader } from '@/components/ui/page-header';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { useTraining } from '@/hooks/useTraining';
import { useAdapters } from '@/pages/Adapters/useAdapters';
import {
  Upload,
  Play,
  List,
  Settings,
  TrendingUp,
  Database,
  Cpu,
  Activity,
  CheckCircle,
  Clock,
  XCircle,
  AlertCircle,
} from 'lucide-react';

interface OperatorDashboardProps {
  selectedTenant?: string;
}

export default function OperatorDashboard({
  selectedTenant = 'default',
}: OperatorDashboardProps) {
  // Fetch training jobs
  const {
    data: trainingJobsData,
    isLoading: trainingJobsLoading,
    error: trainingJobsError,
    refetch: refetchTrainingJobs,
  } = useTraining.useTrainingJobs(undefined, {
    refetchInterval: 10000,
    staleTime: 5000,
  });

  // Fetch datasets
  const {
    data: datasetsData,
    isLoading: datasetsLoading,
    error: datasetsError,
    refetch: refetchDatasets,
  } = useTraining.useDatasets(undefined, { staleTime: 30000 });

  // Fetch adapters
  const {
    data: adaptersData,
    isLoading: adaptersLoading,
    error: adaptersError,
    refetch: refetchAdapters,
  } = useAdapters();

  // Derived data
  const trainingJobs = trainingJobsData?.jobs ?? [];
  const datasets = datasetsData?.datasets ?? [];
  const adapters = adaptersData?.adapters ?? [];

  // Training job statistics
  const activeJobs = trainingJobs.filter(
    (job) => job.status === 'running' || job.status === 'pending'
  ).length;
  const completedJobs = trainingJobs.filter((job) => job.status === 'completed').length;
  const failedJobs = trainingJobs.filter((job) => job.status === 'failed').length;
  const recentJobs = trainingJobs.slice(0, 5);

  // Dataset statistics
  const validDatasets = datasets.filter((d) => d.validation_status === 'valid').length;
  const totalDatasets = datasets.length;

  // Adapter lifecycle statistics
  const loadedAdapters = adapters.filter(
    (a) => a.current_state === 'cold' || a.current_state === 'warm' || a.current_state === 'hot' || a.current_state === 'resident'
  ).length;
  const totalAdapters = adapters.length;

  // Recent activity (last 5 training jobs)
  const recentActivity = recentJobs.map((job) => ({
    id: job.id,
    action: `Training job: ${job.adapter_name || job.id}`,
    status: job.status,
    time: job.updated_at || job.created_at,
    progress: job.progress_pct || 0,
  }));

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'completed':
        return <CheckCircle className="h-4 w-4 text-green-600" />;
      case 'running':
        return <Activity className="h-4 w-4 text-blue-600 animate-pulse" />;
      case 'pending':
        return <Clock className="h-4 w-4 text-yellow-600" />;
      case 'failed':
        return <XCircle className="h-4 w-4 text-red-600" />;
      case 'cancelled':
        return <AlertCircle className="h-4 w-4 text-gray-600" />;
      default:
        return <Clock className="h-4 w-4 text-gray-600" />;
    }
  };

  const getStatusVariant = (status: string): 'default' | 'secondary' | 'destructive' | 'outline' => {
    switch (status) {
      case 'completed':
        return 'default';
      case 'running':
        return 'secondary';
      case 'failed':
        return 'destructive';
      default:
        return 'outline';
    }
  };

  const formatTimeAgo = (timestamp: string): string => {
    const now = new Date();
    const time = new Date(timestamp);
    const diffMs = now.getTime() - time.getTime();
    const diffMins = Math.floor(diffMs / (1000 * 60));
    const diffHours = Math.floor(diffMins / 60);
    const diffDays = Math.floor(diffHours / 24);

    if (diffMins < 1) return 'just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    return `${diffDays}d ago`;
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <PageHeader
        title="Operator Dashboard"
        description={`Training & runtime operations for tenant: ${selectedTenant}`}
        badges={[
          { label: `Tenant: ${selectedTenant}`, variant: 'outline' },
          { label: 'Operator', variant: 'secondary' },
        ]}
      />

      {/* Quick Actions */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base sm:text-lg">Quick Actions</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
            <Button asChild variant="default" className="w-full h-auto p-3 sm:p-4">
              <Link to="/training/datasets" state={{ openUpload: true }}>
                <Upload className="h-4 w-4 mr-2" />
                <span className="text-sm">Upload Dataset</span>
              </Link>
            </Button>
            <Button asChild variant="default" className="w-full h-auto p-3 sm:p-4">
              <Link to="/training" state={{ openTrainingWizard: true }}>
                <Play className="h-4 w-4 mr-2" />
                <span className="text-sm">Start Training</span>
              </Link>
            </Button>
            <Button asChild variant="outline" className="w-full h-auto p-3 sm:p-4">
              <Link to="/training/jobs">
                <List className="h-4 w-4 mr-2" />
                <span className="text-sm hidden sm:inline">View Training Jobs</span>
                <span className="text-sm sm:hidden">Jobs</span>
              </Link>
            </Button>
            <Button asChild variant="outline" className="w-full h-auto p-3 sm:p-4">
              <Link to="/adapters">
                <Settings className="h-4 w-4 mr-2" />
                <span className="text-sm hidden sm:inline">Manage Adapters</span>
                <span className="text-sm sm:hidden">Adapters</span>
              </Link>
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* KPI Cards */}
      <KpiGrid>
        {/* Training Progress */}
        <SectionErrorBoundary sectionName="Training Progress">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between pb-2">
              <CardTitle className="text-sm font-medium">Training Progress</CardTitle>
              <TrendingUp className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              {trainingJobsLoading ? (
                <Skeleton className="h-16 w-full" />
              ) : trainingJobsError ? (
                errorRecoveryTemplates.genericError(trainingJobsError, refetchTrainingJobs)
              ) : (
                <div className="space-y-2">
                  <div className="text-xl sm:text-2xl font-bold">{activeJobs}</div>
                  <p className="text-xs text-muted-foreground">Active jobs</p>
                  <div className="flex flex-wrap gap-2 text-xs">
                    <Badge variant="outline">
                      <CheckCircle className="h-3 w-3 mr-1" />
                      {completedJobs} completed
                    </Badge>
                    {failedJobs > 0 && (
                      <Badge variant="destructive">
                        <XCircle className="h-3 w-3 mr-1" />
                        {failedJobs} failed
                      </Badge>
                    )}
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        </SectionErrorBoundary>

        {/* Dataset Summary */}
        <SectionErrorBoundary sectionName="Dataset Summary">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between pb-2">
              <CardTitle className="text-sm font-medium">Dataset Summary</CardTitle>
              <Database className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              {datasetsLoading ? (
                <Skeleton className="h-16 w-full" />
              ) : datasetsError ? (
                errorRecoveryTemplates.genericError(datasetsError, refetchDatasets)
              ) : (
                <div className="space-y-2">
                  <div className="text-xl sm:text-2xl font-bold">{totalDatasets}</div>
                  <p className="text-xs text-muted-foreground">Total datasets</p>
                  <div className="flex flex-wrap gap-2 text-xs">
                    <Badge variant="outline">
                      <CheckCircle className="h-3 w-3 mr-1" />
                      {validDatasets} ready
                    </Badge>
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        </SectionErrorBoundary>

        {/* Adapter Lifecycle */}
        <SectionErrorBoundary sectionName="Adapter Lifecycle">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between pb-2">
              <CardTitle className="text-sm font-medium">Adapter Lifecycle</CardTitle>
              <Cpu className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              {adaptersLoading ? (
                <Skeleton className="h-16 w-full" />
              ) : adaptersError ? (
                errorRecoveryTemplates.genericError(adaptersError, refetchAdapters)
              ) : (
                <div className="space-y-2">
                  <div className="text-xl sm:text-2xl font-bold">{totalAdapters}</div>
                  <p className="text-xs text-muted-foreground">Total adapters</p>
                  <div className="flex flex-wrap gap-2 text-xs">
                    <Badge variant="outline">
                      <Activity className="h-3 w-3 mr-1" />
                      {loadedAdapters} loaded
                    </Badge>
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        </SectionErrorBoundary>

        {/* System Health */}
        <SectionErrorBoundary sectionName="System Health">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between pb-2">
              <CardTitle className="text-sm font-medium">System Health</CardTitle>
              <Activity className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="space-y-2">
                <div className="text-xl sm:text-2xl font-bold text-green-600">Operational</div>
                <p className="text-xs text-muted-foreground">All systems running</p>
                <div className="flex flex-wrap gap-2 text-xs">
                  <Badge variant="outline">
                    <CheckCircle className="h-3 w-3 mr-1 text-green-600" />
                    Healthy
                  </Badge>
                </div>
              </div>
            </CardContent>
          </Card>
        </SectionErrorBoundary>
      </KpiGrid>

      {/* Content Grid */}
      <ContentGrid>
        {/* Active Training Jobs */}
        <SectionErrorBoundary sectionName="Active Training Jobs">
          <Card>
            <CardHeader>
              <CardTitle>Active Training Jobs</CardTitle>
            </CardHeader>
            <CardContent>
              {trainingJobsLoading ? (
                <div className="space-y-3">
                  <Skeleton className="h-20 w-full" />
                  <Skeleton className="h-20 w-full" />
                </div>
              ) : trainingJobsError ? (
                errorRecoveryTemplates.genericError(trainingJobsError, refetchTrainingJobs)
              ) : recentJobs.length === 0 ? (
                <div className="text-center py-8 text-muted-foreground">
                  <List className="h-12 w-12 mx-auto mb-3 opacity-50" />
                  <p className="text-sm">No training jobs yet</p>
                  <p className="text-xs mt-1">Start your first training job</p>
                  <Button asChild className="mt-4" size="sm">
                    <Link to="/training" state={{ openTrainingWizard: true }}>
                      Start Training
                    </Link>
                  </Button>
                </div>
              ) : (
                <div className="space-y-3">
                  {recentJobs.map((job) => (
                    <div
                      key={job.id}
                      className="border rounded-lg p-3 space-y-2 hover:bg-muted/50 transition-colors"
                    >
                      <div className="flex items-center justify-between">
                        <div className="flex items-center gap-2">
                          {getStatusIcon(job.status)}
                          <span className="font-medium text-sm truncate">
                            {job.adapter_name || job.id}
                          </span>
                        </div>
                        <Badge variant={getStatusVariant(job.status)}>{job.status}</Badge>
                      </div>
                      <div className="text-xs text-muted-foreground">
                        Dataset: {job.dataset_id || '—'}
                      </div>
                      {job.status === 'running' && job.progress_pct !== undefined && (
                        <div className="space-y-1">
                          <div className="flex justify-between text-xs">
                            <span>Progress</span>
                            <span>{job.progress_pct.toFixed(0)}%</span>
                          </div>
                          <Progress value={job.progress_pct} className="h-2" />
                        </div>
                      )}
                      <div className="text-xs text-muted-foreground">
                        {formatTimeAgo(job.updated_at || job.created_at)}
                      </div>
                    </div>
                  ))}
                  <Button asChild variant="outline" className="w-full" size="sm">
                    <Link to="/training/jobs">View All Jobs</Link>
                  </Button>
                </div>
              )}
            </CardContent>
          </Card>
        </SectionErrorBoundary>

        {/* Recent Activity */}
        <SectionErrorBoundary sectionName="Recent Activity">
          <Card>
            <CardHeader>
              <CardTitle>Recent Activity</CardTitle>
            </CardHeader>
            <CardContent>
              {trainingJobsLoading ? (
                <div className="space-y-3">
                  <Skeleton className="h-16 w-full" />
                  <Skeleton className="h-16 w-full" />
                </div>
              ) : trainingJobsError ? (
                errorRecoveryTemplates.genericError(trainingJobsError, refetchTrainingJobs)
              ) : recentActivity.length === 0 ? (
                <div className="text-center py-8 text-muted-foreground">
                  <Activity className="h-12 w-12 mx-auto mb-3 opacity-50" />
                  <p className="text-sm">No recent activity</p>
                </div>
              ) : (
                <div className="space-y-3">
                  {recentActivity.map((activity) => (
                    <div key={activity.id} className="flex items-start gap-3">
                      <div className="mt-1">{getStatusIcon(activity.status)}</div>
                      <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium">{activity.action}</p>
                        <div className="flex items-center gap-2 text-xs text-muted-foreground">
                          <span>{formatTimeAgo(activity.time)}</span>
                          <Badge variant={getStatusVariant(activity.status)} className="text-xs">
                            {activity.status}
                          </Badge>
                        </div>
                        {activity.status === 'running' && activity.progress > 0 && (
                          <Progress value={activity.progress} className="h-1 mt-1" />
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        </SectionErrorBoundary>
      </ContentGrid>
    </div>
  );
}
