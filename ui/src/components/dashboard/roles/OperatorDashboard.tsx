import React, { useMemo, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Skeleton } from '@/components/ui/skeleton';
import { ContentGrid, KpiGrid } from '@/components/ui/grid';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useTraining } from '@/hooks/training';
import { useAdapters } from '@/hooks/adapters/useAdapters';
import { OperatorChatLayout } from '@/components/operator';
import { useSystemMetrics, useComputedMetrics } from '@/hooks/system/useSystem';
import { useSettings } from '@/hooks/config/useSettings';
import { useQuery } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import { useInferenceSessions } from '@/hooks/inference/useInferenceSessions';
import { getRoleLanguage } from '@/config/roleConfigs';
import { buildTrainingDatasetsLink, buildTrainingJobsLink, buildTrainingOverviewLink, buildAdaptersListLink, buildInferenceLink } from '@/utils/navLinks';
import {
  Upload,
  Play,
  List,
  Settings,
  TrendingUp,
  Database,
  Activity,
  CheckCircle,
  Clock,
  XCircle,
  AlertCircle,
  MessageSquare,
  Zap,
  Shield,
  ShieldAlert,
  Radar,
  Radio,
  Brain,
} from 'lucide-react';

interface OperatorDashboardProps {
  selectedTenant?: string;
}

const operatorLanguage = getRoleLanguage('operator');

function OperatorSummaryCards() {
  const navigate = useNavigate();
  const { metrics: systemMetrics, isLoading: sysLoading, error: sysError } = useSystemMetrics('fast', true);
  const computed = useComputedMetrics(systemMetrics);

  const {
    data: adaptersData,
    isLoading: adaptersLoading,
    error: adaptersError,
    refetch: refetchAdapters,
  } = useAdapters();
  const {
    data: trainingJobsData,
    isLoading: trainingJobsLoading,
    error: trainingJobsError,
    refetch: refetchTraining,
  } = useTraining.useTrainingJobs(undefined, { refetchInterval: 15000, staleTime: 5000 });
  const { data: settingsData } = useSettings();
  const { data: routingAnomalies, isLoading: routingLoading, error: routingError } = useQuery({
    queryKey: ['routing-anomalies'],
    queryFn: () => apiClient.getRoutingDecisions({ limit: 5, anomalies_only: true }),
    staleTime: 30_000,
  });
  const { data: documentsData, isLoading: docsLoading, error: docsError } = useQuery({
    queryKey: ['documents-freshness'],
    queryFn: () => apiClient.listDocuments(),
    staleTime: 60_000,
  });
  const { recentSessions } = useInferenceSessions({ maxSessions: 5, storageKey: 'inference-sessions' });

  const healthSummary = useMemo(() => {
    if (!computed) return null;
    return {
      cpu: computed.cpuUsage,
      memory: computed.memoryUsage,
      gpu: computed.gpuUsage,
      nodes: computed.nodeCount,
      workers: computed.workerCount,
    };
  }, [computed]);

  const adaptersInPlay = (adaptersData?.adapters || []).slice(0, 5);
  const trainingQueue = trainingJobsData?.jobs ?? [];
  const latestDoc = useMemo(() => {
    const docs = documentsData || [];
    if (!docs.length) return null;
    return docs
      .slice()
      .sort((a, b) => new Date(b.updated_at || b.created_at).getTime() - new Date(a.updated_at || a.created_at).getTime())[0];
  }, [documentsData]);

  const policyPosture = useMemo(() => {
    const security = settingsData?.security;
    if (!security) return null;
    return {
      egressEnabled: security.egress_enabled,
      requireMfa: security.require_mfa,
    };
  }, [settingsData]);

  return (
    <SectionErrorBoundary sectionName="Operator Summary">
      <ContentGrid>
        {/* Health at a glance */}
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium">{operatorLanguage.systemHealthLabel}</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 text-sm">
            {sysLoading ? (
              <Skeleton className="h-16 w-full" />
            ) : sysError ? (
              <div className="text-muted-foreground">Live status unavailable</div>
            ) : healthSummary ? (
              <>
                <div className="flex items-center gap-2 text-green-700">
                  <CheckCircle className="h-4 w-4" />
                  <span>Systems are running smoothly.</span>
                </div>
                <p className="text-xs text-muted-foreground">
                  Technical meters stay tucked away unless you need them.
                </p>
                <Accordion type="single" collapsible className="border rounded-md">
                  <AccordionItem value="technical">
                    <AccordionTrigger className="px-3 text-sm">
                      {operatorLanguage.technicalDetailsLabel}
                    </AccordionTrigger>
                    <AccordionContent className="px-3 pb-3 space-y-3">
                      <div className="space-y-2">
                        <div className="flex items-center justify-between text-xs">
                          <span className="flex items-center gap-2">
                            <Activity className="h-3 w-3 text-muted-foreground" />
                            CPU
                          </span>
                          <span>{healthSummary.cpu != null ? `${healthSummary.cpu.toFixed(1)}%` : 'N/A'}</span>
                        </div>
                        <Progress value={healthSummary.cpu ?? 0} className="h-2" />
                      </div>
                      <div className="space-y-2">
                        <div className="flex items-center justify-between text-xs">
                          <span className="flex items-center gap-2">
                            <Activity className="h-3 w-3 text-muted-foreground" />
                            Memory
                          </span>
                          <span>{healthSummary.memory != null ? `${healthSummary.memory.toFixed(1)}%` : 'N/A'}</span>
                        </div>
                        <Progress value={healthSummary.memory ?? 0} className="h-2" />
                      </div>
                      <div className="flex items-center justify-between text-xs">
                        <span className="flex items-center gap-2">
                          <Activity className="h-3 w-3 text-muted-foreground" />
                          Accelerators
                        </span>
                        <span>{healthSummary.gpu != null ? `${healthSummary.gpu.toFixed(1)}%` : 'N/A'}</span>
                      </div>
                      <div className="flex items-center justify-between text-xs text-muted-foreground">
                        <span>Workers</span>
                        <span>{healthSummary.workers ?? 'N/A'}</span>
                      </div>
                    </AccordionContent>
                  </AccordionItem>
                </Accordion>
              </>
            ) : (
              <div className="text-muted-foreground">No status data yet.</div>
            )}
          </CardContent>
        </Card>

        {/* Last routing anomalies */}
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium">Routing watchlist</CardTitle>
          </CardHeader>
          <CardContent className="text-sm text-muted-foreground space-y-2">
            {routingLoading ? (
              <Skeleton className="h-16 w-full" />
            ) : routingError ? (
              <div>Unavailable</div>
            ) : routingAnomalies && routingAnomalies.length > 0 ? (
              <ul className="space-y-1">
                {routingAnomalies.map(decision => (
                  <li key={decision.id || decision.request_id} className="border rounded p-2">
                    <div className="flex items-center justify-between">
                      <span className="font-medium text-xs">#{(decision.request_id || decision.id).slice(0, 6)}</span>
                      <span className="text-[11px] text-muted-foreground">
                        {decision.timestamp ? new Date(decision.timestamp).toLocaleTimeString() : ''}
                      </span>
                    </div>
                    <div className="text-xs">AI modules: {decision.selected_adapters?.join(', ') || 'unknown'}</div>
                  </li>
                ))}
              </ul>
            ) : (
              <div className="flex items-center gap-2 text-foreground">
                <Radar className="h-4 w-4" />
                No routing flags right now
              </div>
            )}
          </CardContent>
        </Card>

        {/* Adapters currently in play */}
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium flex items-center gap-2">
              <Brain className="h-4 w-4" />
              AI modules in rotation
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-2">
            {adaptersLoading ? (
              <Skeleton className="h-16 w-full" />
            ) : adaptersError ? (
              errorRecoveryTemplates.genericError(adaptersError, refetchAdapters)
            ) : adaptersInPlay.length === 0 ? (
              <div className="text-sm text-muted-foreground">No AI modules are active yet.</div>
            ) : (
              <ul className="space-y-1 text-sm">
                {adaptersInPlay.map(adapter => (
                  <li key={adapter.id} className="flex items-center gap-2">
                    <Badge variant="outline">{adapter.name || adapter.id}</Badge>
                    <span className="text-xs text-muted-foreground">{adapter.current_state || 'unknown'}</span>
                  </li>
                ))}
              </ul>
            )}
          </CardContent>
        </Card>

        {/* One-run sanity probe */}
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium">Quick check run</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            <p className="text-muted-foreground">Run a quick action with your latest safe settings.</p>
            <Button
              size="sm"
              onClick={() => {
                const preset = recentSessions[0];
                navigate(buildInferenceLink(), preset ? { state: { presetSession: preset } } : undefined);
              }}
            >
              Run quick check
            </Button>
          </CardContent>
        </Card>

        {/* Policy posture */}
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium">Policy posture</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-2 text-sm text-muted-foreground">
            {policyPosture ? (
              <>
                <div className="flex items-center gap-2">
                  <Shield className="h-4 w-4" />
                  Guardrails: {policyPosture.requireMfa ? 'MFA required' : 'MFA optional'}
                </div>
                <div className="text-xs text-muted-foreground">
                  Outbound data: {policyPosture.egressEnabled ? 'Allowed for this workspace' : 'Blocked for safety'}
                </div>
              </>
            ) : (
              <div className="flex items-center gap-2">
                <Shield className="h-4 w-4" />
                Guardrail summary unavailable
              </div>
            )}
          </CardContent>
        </Card>

        {/* Egress posture */}
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium">Outbound sharing</CardTitle>
          </CardHeader>
          <CardContent className="flex items-center gap-2 text-sm text-muted-foreground">
            <ShieldAlert className="h-4 w-4" />
            {policyPosture
              ? policyPosture.egressEnabled
                ? 'Outbound enabled'
                : 'Outbound blocked'
              : 'Sharing status unavailable'}
          </CardContent>
        </Card>

        {/* Training queue */}
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium">{operatorLanguage.learningTasksLabel}</CardTitle>
          </CardHeader>
          <CardContent>
            {trainingJobsLoading ? (
              <Skeleton className="h-16 w-full" />
            ) : trainingJobsError ? (
              errorRecoveryTemplates.genericError(trainingJobsError, refetchTraining)
            ) : trainingQueue.length === 0 ? (
              <div className="text-sm text-muted-foreground">{operatorLanguage.emptyTasksCopy}</div>
            ) : (
              <ul className="space-y-2 text-sm">
                {trainingQueue.slice(0, 5).map(job => (
                  <li key={job.id} className="flex items-center justify-between border rounded p-2">
                    <span className="truncate">{job.adapter_name || job.id}</span>
                    <Badge variant={getStatusVariant(job.status)}>{job.status}</Badge>
                  </li>
                ))}
              </ul>
            )}
          </CardContent>
        </Card>

        {/* RAG freshness */}
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium">Document freshness</CardTitle>
          </CardHeader>
          <CardContent className="flex items-center gap-2 text-sm text-muted-foreground">
            <Radio className="h-4 w-4" />
            {docsLoading
              ? 'Loading...'
              : docsError
                ? 'Unavailable'
                : latestDoc
                  ? `Last update: ${new Date(latestDoc.updated_at || latestDoc.created_at).toLocaleString()}`
                  : 'No document updates yet'}
          </CardContent>
        </Card>
      </ContentGrid>
    </SectionErrorBoundary>
  );
}
// Helper functions
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

/**
 * Training dashboard content - the original operator dashboard view
 */
function TrainingDashboardContent({ selectedTenant }: { selectedTenant: string }) {
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
    action: `Learning task: ${job.adapter_name || job.id}`,
    status: job.status,
    time: job.updated_at || job.created_at,
    progress: job.progress_pct || 0,
  }));

  return (
    <div className="space-y-6 p-6 overflow-auto">
      {/* Quick Actions */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base sm:text-lg">Quick Actions</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
            <Button asChild variant="default" className="w-full h-auto p-3 sm:p-4">
              <Link to={buildTrainingDatasetsLink()} state={{ openUpload: true }}>
                <Upload className="h-4 w-4 mr-2" />
                <span className="text-sm">Upload Dataset</span>
              </Link>
            </Button>
            <Button asChild variant="default" className="w-full h-auto p-3 sm:p-4">
              <Link to={buildTrainingOverviewLink()} state={{ openTrainingWizard: true }}>
                <Play className="h-4 w-4 mr-2" />
                <span className="text-sm">Start Training</span>
              </Link>
            </Button>
            <Button asChild variant="outline" className="w-full h-auto p-3 sm:p-4">
              <Link to={buildTrainingJobsLink()}>
                <List className="h-4 w-4 mr-2" />
                <span className="text-sm hidden sm:inline">View Learning Tasks</span>
                <span className="text-sm sm:hidden">Tasks</span>
              </Link>
            </Button>
            <Button asChild variant="outline" className="w-full h-auto p-3 sm:p-4">
              <Link to={buildAdaptersListLink()}>
                <Settings className="h-4 w-4 mr-2" />
                <span className="text-sm hidden sm:inline">Manage AI Modules</span>
                <span className="text-sm sm:hidden">Modules</span>
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

        {/* Active AI Modules */}
        <SectionErrorBoundary sectionName="Active AI Modules">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between pb-2">
              <CardTitle className="text-sm font-medium">Active AI Modules</CardTitle>
              <Brain className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              {adaptersLoading ? (
                <Skeleton className="h-16 w-full" />
              ) : adaptersError ? (
                errorRecoveryTemplates.genericError(adaptersError, refetchAdapters)
              ) : (
                <div className="space-y-2">
                  <div className="text-xl sm:text-2xl font-bold">{totalAdapters}</div>
                  <p className="text-xs text-muted-foreground">AI modules available</p>
                  <div className="flex flex-wrap gap-2 text-xs">
                    <Badge variant="outline">
                      <Activity className="h-3 w-3 mr-1" />
                      {loadedAdapters} active now
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
        {/* Active Learning Tasks */}
        <SectionErrorBoundary sectionName={operatorLanguage.learningTasksLabel}>
          <Card>
            <CardHeader>
              <CardTitle>{operatorLanguage.learningTasksLabel}</CardTitle>
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
                  <p className="text-sm">{operatorLanguage.emptyTasksCopy}</p>
                  <p className="text-xs mt-1">Start your first learning task</p>
                  <Button asChild className="mt-4" size="sm">
                    <Link to={buildTrainingOverviewLink()} state={{ openTrainingWizard: true }}>
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
                      {job.status === 'running' && job.progress_pct != null && (
                        <div className="space-y-1">
                          <div className="flex justify-between text-xs">
                            <span>Progress</span>
                            <span>{job.progress_pct.toFixed(0)}%</span>
                          </div>
                          <Progress value={job.progress_pct} className="h-2" />
                        </div>
                      )}
                      <div className="text-xs text-muted-foreground">
                        {formatTimeAgo((job.updated_at ?? job.created_at) ?? new Date().toISOString())}
                      </div>
                    </div>
                  ))}
                  <Button asChild variant="outline" className="w-full" size="sm">
                    <Link to={buildTrainingJobsLink()}>View all tasks</Link>
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
                  <p className="text-sm">{operatorLanguage.emptyActivityCopy}</p>
                </div>
              ) : (
                <div className="space-y-3">
                  {recentActivity.map((activity) => (
                    <div key={activity.id} className="flex items-start gap-3">
                      <div className="mt-1">{getStatusIcon(activity.status)}</div>
                      <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium">{activity.action}</p>
                        <div className="flex items-center gap-2 text-xs text-muted-foreground">
                          <span>{formatTimeAgo(activity.time ?? new Date().toISOString())}</span>
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

/**
 * OperatorDashboard - Chat-first dashboard for operators
 *
 * Features tabbed interface with Chat (default) and Training views.
 * Chat tab includes auto-model-loading and full ChatInterface.
 */
export default function OperatorDashboard({
  selectedTenant = 'default',
}: OperatorDashboardProps) {
  const [activeTab, setActiveTab] = useState<'chat' | 'training'>('chat');

  return (
    <div className="h-full flex flex-col space-y-6">
      <OperatorSummaryCards />

      <Tabs
        value={activeTab}
        onValueChange={(v) => setActiveTab(v as 'chat' | 'training')}
        className="h-full flex flex-col"
      >
        <div className="border-b px-4 py-2 flex items-center justify-between">
          <TabsList>
            <TabsTrigger value="chat" className="gap-2">
              <MessageSquare className="h-4 w-4" />
              Chat
            </TabsTrigger>
            <TabsTrigger value="training" className="gap-2">
              <Zap className="h-4 w-4" />
              Training
            </TabsTrigger>
          </TabsList>
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Badge variant="outline">{selectedTenant}</Badge>
            <Badge variant="secondary">Operator</Badge>
          </div>
        </div>

        <TabsContent value="chat" className="flex-1 mt-0 overflow-hidden">
          <OperatorChatLayout tenantId={selectedTenant} />
        </TabsContent>

        <TabsContent value="training" className="flex-1 mt-0 overflow-auto">
          <TrainingDashboardContent selectedTenant={selectedTenant} />
        </TabsContent>
      </Tabs>
    </div>
  );
}
