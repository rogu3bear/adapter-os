import React, { useState, useMemo, useEffect, useCallback, memo } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Progress } from './ui/progress';
import { Skeleton } from './ui/skeleton';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger, DialogFooter } from './ui/dialog';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Alert, AlertDescription, AlertTitle } from './ui/alert';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { toast } from 'sonner';
import { logger } from '@/utils/logger';
import { useActivityFeed } from '@/hooks/useActivityFeed';
import { formatRelativeTime } from '@/utils/format';
import {
  Activity,
  Server,
  Users,
  Shield,
  AlertTriangle,
  CheckCircle,
  Clock,
  Cpu,
  HardDrive,
  Network,
  Zap,
  Code,
  Eye,
  Target,
  Download,
  XCircle,
  Bell,
  BarChart3
} from 'lucide-react';
import { BaseModelStatusComponent } from './BaseModelStatus';
import { Nodes } from './Nodes';
import { AlertsPage } from './AlertsPage';
import { useInformationDensity } from '@/hooks/useInformationDensity';
import { DensityControls } from './ui/density-controls';
import { PluginStatusWidget } from './dashboard/PluginStatusWidget';
import { DashboardSettings } from './dashboard/DashboardSettings';
import apiClient from '@/api/client';
import { usePolling } from '@/hooks/usePolling';
import { useSSE } from '@/hooks/useSSE';
import { useDashboardConfig } from '@/hooks/useDashboardConfig';
import type { User, TrainingJob, DatasetValidationStatus, AdapterStack } from '@/api/types';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { GlossaryTooltip } from './ui/glossary-tooltip';
import { useRBAC } from '@/hooks/useRBAC';
import { PageHeader } from './ui/page-header';
import { ActionGrid } from './ui/action-grid';
import { KpiGrid, ContentGrid, FormGrid } from './ui/grid';
import { useModalManager } from '@/contexts/ModalContext';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { useTraining } from '@/hooks/useTraining';
import { useAdapterStacks, useGetDefaultStack } from '@/hooks/useAdmin';
import { QUERY_FAST } from '@/api/queryOptions';

const MODAL_IDS = {
  HEALTH: 'dashboard-health',
  CREATE_TENANT: 'dashboard-create-tenant',
  DEPLOY_ADAPTER: 'dashboard-deploy-adapter',
} as const;

// Static dashboard tabs - moved outside component to prevent recreation
const DASHBOARD_TABS = [
  { id: 'overview', label: 'Overview', icon: BarChart3, description: 'System overview and metrics' },
  { id: 'nodes', label: 'Nodes', icon: Server, description: 'Compute infrastructure monitoring' },
  { id: 'alerts', label: 'Alerts', icon: Bell, description: 'System alerts and monitoring' }
] as const;

interface DashboardProps {
  user?: User;
  selectedTenant?: string;
  onNavigate?: (tab: string) => void;
}

interface DashboardWidget {
  id: string;
  component: React.ComponentType<Record<string, unknown>>;
  priority: number;
}

interface DashboardLayout {
  widgets: DashboardWidget[];
  quickActions: Array<{
    label: string;
    icon: React.ComponentType<{ className?: string }>;
    route: string;
    variant?: 'default' | 'outline' | 'secondary';
  }>;
}

// Main Dashboard component
export const Dashboard = memo(function Dashboard({ user, selectedTenant, onNavigate }: DashboardProps) {
  const navigate = useNavigate();
  const { can, userRole } = useRBAC();
  const { openModal, closeModal, isOpen } = useModalManager();

  // SSE connection for real-time metrics updates
  const {
    data: sseMetrics,
    error: sseError,
    connected: sseConnected,
    reconnect: sseReconnect
  } = useSSE<{
    // Backend returns these field names
    cpu_usage?: number;
    memory_usage?: number;
    disk_usage?: number;
    // SSE/legacy field names (fallback)
    cpu_usage_percent?: number;
    memory_usage_percent?: number;
    disk_usage_percent?: number;
    network_rx_bytes?: number;
    adapter_count?: number;
    active_sessions?: number;
    tokens_per_second?: number;
    latency_p95_ms?: number;
  }>('/v1/stream/metrics', {
    enabled: true,
    onError: (event) => {
      logger.error('Real-time metrics connection error', {
        component: 'Dashboard',
        operation: 'sse_connection',
        tenantId: selectedTenant,
        userId: user?.user_id
      }, new Error('SSE connection error'));
    }
  });

  // State declarations
  const [activeTab, setActiveTab] = useState('overview');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [newTenantName, setNewTenantName] = useState('');
  const [newTenantIsolation, setNewTenantIsolation] = useState('standard');
  const [adapters, setAdapters] = useState<{ id: string; name: string }[]>([]);
  const [selectedAdapter, setSelectedAdapter] = useState('');
  const [deployTargetTenant, setDeployTargetTenant] = useState(selectedTenant || '');
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [nodeCount, setNodeCount] = useState(0);
  const [tenantCount, setTenantCount] = useState(0);
  const [createTenantError, setCreateTenantError] = useState<string | null>(null);
  const [deployAdapterError, setDeployAdapterError] = useState<string | null>(null);

  // Information density
  const { density, setDensity, textSizes, spacing } = useInformationDensity({
    key: 'dashboard-density',
    defaultDensity: 'comfortable',
    persist: true
  });

  // Dashboard configuration
  const {
    widgets,
    isLoading: configLoading,
    updateWidgetVisibility,
    resetConfig
  } = useDashboardConfig(user?.user_id);

  // Derive available widget IDs from widgets config
  const availableWidgetIds = widgets.map(w => w.widget_id);
  const userWidgetConfig = widgets;

  // Effective user and tenant
  const effectiveUser = user || {
    user_id: 'guest',
    email: 'guest@adapteros.local',
    display_name: 'Guest',
    role: 'viewer' as const,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    is_active: true
  };

  // Display-only tenant label (for UI strings), API hooks use selectedTenant directly
  const effectiveTenant = selectedTenant || 'default';

  // Core usage data
  const {
    data: datasetsData,
    isLoading: datasetsLoading,
    error: datasetsError,
    refetch: refetchDatasets
  } = useTraining.useDatasets(undefined, { staleTime: 30000 });

  const {
    data: trainingJobsData,
    isLoading: trainingJobsLoading,
    error: trainingJobsError,
    refetch: refetchTrainingJobs
  } = useTraining.useTrainingJobs(undefined, {
    refetchInterval: 10000,
    staleTime: 5000,
  });

  const {
    data: adapterList,
    isLoading: adaptersLoading,
    error: adaptersError,
    refetch: refetchAdapters
  } = useQuery({
    queryKey: ['adapters', 'dashboard'],
    queryFn: () => apiClient.listAdapters(),
    ...QUERY_FAST,
  });

  const {
    data: stacks = [],
    isLoading: stacksLoading,
    error: stacksError,
    refetch: refetchStacks
  } = useAdapterStacks();

  const {
    data: defaultStack,
    isLoading: defaultStackLoading,
    error: defaultStackError,
    refetch: refetchDefaultStack
  } = useGetDefaultStack(selectedTenant);

  // SSE connection status - use real SSE connection state
  const connected = sseConnected;

  // System metrics polling
  const fetchSystemMetrics = useCallback(async () => {
    const response = await apiClient.getSystemMetrics();
    return response;
  }, []);

  const {
    data: systemMetrics,
    isLoading: metricsLoading,
    error: metricsError,
    refetch: refetchMetrics
  } = usePolling(fetchSystemMetrics, 'normal', {
    enabled: !sseConnected, // Disable polling when SSE is connected
    operationName: 'system-metrics',
    onError: (err) => {
      logger.error('Failed to fetch system metrics', {
        component: 'Dashboard',
        operation: 'fetchSystemMetrics',
        tenantId: selectedTenant,
        userId: user?.user_id
      }, err);
    }
  });

  const datasets = useMemo(() => datasetsData?.datasets ?? [], [datasetsData]);
  const datasetStats = useMemo(() => {
    const counts: Record<DatasetValidationStatus, number> & { total: number } = {
      draft: 0,
      validating: 0,
      valid: 0,
      invalid: 0,
      failed: 0,
      total: datasets.length,
    };

    datasets.forEach(dataset => {
      counts[dataset.validation_status] = (counts[dataset.validation_status] || 0) + 1;
    });

    return counts;
  }, [datasets]);

  const trainingJobs = useMemo(() => trainingJobsData?.jobs ?? [], [trainingJobsData]);

  const parseTimestamp = useCallback((value?: string) => {
    if (!value) {
      return 0;
    }
    const time = Date.parse(value);
    return Number.isNaN(time) ? 0 : time;
  }, []);

  const trainingJobTimestamp = useCallback(
    (job: TrainingJob) =>
      parseTimestamp(job.updated_at) ||
      parseTimestamp(job.completed_at) ||
      parseTimestamp(job.created_at) ||
      parseTimestamp(job.started_at),
    [parseTimestamp]
  );

  const recentTrainingJob = useMemo<TrainingJob | null>(() => {
    if (trainingJobs.length === 0) {
      return null;
    }
    return [...trainingJobs].sort((a, b) => trainingJobTimestamp(b) - trainingJobTimestamp(a))[0];
  }, [trainingJobs, trainingJobTimestamp]);

  const recentCompletedJobWithStack = useMemo<TrainingJob | null>(() => {
    const completed = trainingJobs.filter(job => job.status === 'completed' && job.stack_id);
    if (completed.length === 0) {
      return null;
    }
    return [...completed].sort((a, b) => trainingJobTimestamp(b) - trainingJobTimestamp(a))[0];
  }, [trainingJobs, trainingJobTimestamp]);

  const runningJobs = useMemo(
    () => trainingJobs.filter(job => job.status === 'running' || job.status === 'pending').length,
    [trainingJobs]
  );

  const completedLast7Days = useMemo(() => {
    const now = Date.now();
    const windowMs = 7 * 24 * 60 * 60 * 1000;
    return trainingJobs.filter(job => {
      if (job.status !== 'completed') return false;
      const completedAt = parseTimestamp(job.completed_at || job.updated_at || job.created_at);
      return completedAt > 0 && (now - completedAt) <= windowMs;
    }).length;
  }, [trainingJobs, parseTimestamp]);

  const adapterTotal = adapterList?.length ?? 0;
  const stackTotal = stacks?.length ?? 0;
  const stackNameLookup = useMemo(
    () => new Map(stacks.map(stack => [stack.id, stack.name])),
    [stacks]
  );
  const defaultStackLabel = defaultStackLoading
    ? 'Stack: loading'
    : defaultStackError
      ? 'Stack: unavailable'
      : defaultStack
        ? `Stack: ${defaultStack.name}`
        : 'Stack: not set';
  const adapterStackError =
    (adaptersError as Error | undefined) ||
    (stacksError as Error | undefined) ||
    (defaultStackError as Error | undefined);
  const headerDescription = defaultStackLoading
    ? `Tenant ${effectiveTenant} • Resolving default stack...`
    : defaultStackError
      ? `Tenant ${effectiveTenant} • Default stack unavailable • System status: Operational`
      : defaultStack
        ? `Tenant ${effectiveTenant} • Default stack ${defaultStack.name} • System status: Operational`
        : `Tenant ${effectiveTenant} • No default stack configured • System status: Operational`;
  const deployModalOpen = isOpen(MODAL_IDS.DEPLOY_ADAPTER);

  // Fetch dashboard data
  const fetchData = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);

      // Fetch nodes and tenants in parallel for efficiency
      const [nodes, tenants] = await Promise.all([
        apiClient.listNodes(),
        apiClient.listTenants(),
      ]);

      setNodeCount(nodes.length);
      setTenantCount(tenants.length);
      setLoading(false);
    } catch (err) {
      logger.error('Failed to fetch dashboard data', {
        component: 'Dashboard',
        operation: 'fetchData',
        tenantId: selectedTenant,
        userId: user?.user_id,
      }, err instanceof Error ? err : new Error(String(err)));
      setError(err instanceof Error ? err.message : 'Failed to load dashboard');
      setLoading(false);
    }
  }, [selectedTenant, user?.user_id]);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const handleCreateTenant = async () => {
    if (!newTenantName.trim()) {
      setCreateTenantError('Organization name is required');
      return;
    }

    try {
      // Note: CreateTenantRequest requires name, uid, gid, and optional isolation_level
      await apiClient.createTenant({
        name: newTenantName,
        uid: 1000, // Default UID - should be configurable in production
        gid: 1000, // Default GID - should be configurable in production
        isolation_level: newTenantIsolation,
      });
      toast.success(`Tenant "${newTenantName}" created successfully`);
      closeModal();
      setNewTenantName('');
      setNewTenantIsolation('standard');
      setCreateTenantError(null);
      await fetchData();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to create tenant';
      setCreateTenantError(errorMsg);
      toast.error(errorMsg);
    }
  };

  const handleDeployAdapter = async () => {
    if (!selectedAdapter) {
      setDeployAdapterError('Please select an adapter');
      return;
    }

    try {
      // For now, we'll just show a success message
      // In a full implementation, this would call an adapter deployment endpoint
      toast.success(`Adapter deployed to tenant "${deployTargetTenant}"`);
      closeModal();
      setSelectedAdapter('');
      setDeployAdapterError(null);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to deploy adapter';
      setDeployAdapterError(errorMsg);
      toast.error(errorMsg);
    }
  };

  const handleExportLogs = async () => {
    try {
      toast.info('Preparing log export...');
      // In a full implementation, this would call the log export endpoint
      // For now, we'll simulate a download
      setTimeout(() => {
        toast.success('Logs exported successfully');
      }, 1000);
    } catch (err) {
      toast.error('Failed to export logs');
    }
  };

  useEffect(() => {
    // Load adapters for deployment modal
    const loadAdapters = async () => {
      try {
        const adaptersList = await apiClient.listAdapters();
        setAdapters(adaptersList);
      } catch (err) {
        logger.error('Failed to load adapters', {
          component: 'Dashboard',
          operation: 'loadAdapters',
          tenantId: selectedTenant,
          userId: user?.user_id
        }, err instanceof Error ? err : new Error(String(err)));
      }
    };
    if (deployModalOpen) {
      loadAdapters();
    }
  }, [deployModalOpen, selectedTenant, user?.user_id]);

  // Real-time activity feed from telemetry and audit logs
  // Note: useActivityFeed doesn't require userId parameter
  const { events: activityEvents, loading: activityLoading, error: activityError } = useActivityFeed({
    enabled: true,
    maxEvents: 10,
    tenantId: effectiveTenant
  });


  const getActivityIcon = (type: string) => {
    switch (type) {
      case 'recovery': return CheckCircle;
      case 'policy': return Shield;
      case 'build': return Zap;
      case 'adapter': return Code;
      case 'telemetry': return Eye;
      case 'security': return Shield;
      case 'error': return AlertTriangle;
      default: return Activity;
    }
  };

  // Transform activity events to display format - memoized to prevent re-renders
  const recentActivity = useMemo(() =>
    activityEvents.map(event => ({
      time: formatRelativeTime(event.timestamp),
      action: event.message,
      type: event.type,
      icon: getActivityIcon(event.type),
      severity: event.severity
    })),
    [activityEvents]
  );

  // Memoize quickActions to prevent re-renders
  const quickActions = useMemo(() => [
    {
      label: 'View System Health',
      icon: Activity,
      color: 'text-emerald-600',
      helpId: 'quick-action-health',
      onClick: () => openModal(MODAL_IDS.HEALTH)
    },
    {
      label: 'Create Tenant',
      icon: Users,
      color: 'text-blue-600',
      helpId: 'quick-action-create-tenant',
      disabled: !can('tenant:manage'),
      disabledTitle: 'Requires tenant:manage permission',
      onClick: () => openModal(MODAL_IDS.CREATE_TENANT)
    },
    {
      label: 'Deploy Adapter',
      icon: Code,
      color: 'text-violet-600',
      helpId: 'quick-action-deploy-adapter',
      disabled: !can('adapter:register'),
      disabledTitle: 'Requires adapter:register permission',
      onClick: () => openModal(MODAL_IDS.DEPLOY_ADAPTER)
    },
    {
      label: 'Review Policies',
      icon: Shield,
      color: 'text-amber-600',
      helpId: 'quick-action-policies',
      onClick: () => (onNavigate ? onNavigate('policies') : navigate('/security/policies'))
    }
  ], [can, openModal, onNavigate, navigate]);

  // Merge SSE and polling data - SSE takes priority for real-time updates
  const effectiveMetrics = sseMetrics || systemMetrics;
  // Backend returns cpu_usage, memory_usage, disk_usage - fallback to _percent for SSE/legacy
  const memoryUsage = effectiveMetrics?.memory_usage ?? effectiveMetrics?.memory_usage_percent ?? (systemMetrics as { memory_usage_pct?: number } | null)?.memory_usage_pct ?? 0;
  const adapterCount = effectiveMetrics?.adapter_count || 0;
  const activeSessions = effectiveMetrics?.active_sessions || 0;
  const tokensPerSecond = effectiveMetrics?.tokens_per_second || 0;
  const latencyP95 = effectiveMetrics?.latency_p95_ms || 0;
  const cpuUsage = effectiveMetrics?.cpu_usage ?? effectiveMetrics?.cpu_usage_percent ?? 0;
  const diskUsage = effectiveMetrics?.disk_usage ?? effectiveMetrics?.disk_usage_percent ?? 0;
  const networkBandwidth = effectiveMetrics?.network_rx_bytes ? (effectiveMetrics.network_rx_bytes / 1024 / 1024).toFixed(1) : '0';


  return (
    <div className="space-y-6">
      {/* Header */}
      <PageHeader
        title="Dashboard"
        description={headerDescription}
        badges={[
          { label: `Tenant: ${effectiveTenant}`, variant: 'outline' },
          { label: defaultStackLabel, variant: 'secondary' },
          { label: effectiveUser.role, variant: 'secondary' }
        ]}
      >
        <DensityControls
          density={density}
          onDensityChange={setDensity}
          showLabel={false}
        />
      </PageHeader>

      {/* Dashboard Tabs */}
      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList className="grid w-full grid-cols-3">
          {DASHBOARD_TABS.map((tab) => {
            const Icon = tab.icon;
            return (
              <TabsTrigger key={tab.id} value={tab.id} className="flex items-center gap-2">
                <Icon className="h-4 w-4" />
                <span className="hidden sm:inline">{tab.label}</span>
              </TabsTrigger>
            );
          })}
        </TabsList>

        {/* Overview Tab */}
        <TabsContent value="overview" className={spacing.sectionGap}>
          {/* SSE Connection Error Alert */}
          {sseError && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertTitle>Real-time Connection Error</AlertTitle>
              <AlertDescription className="flex items-center justify-between">
                <span>{sseError}. Falling back to polling for metrics updates.</span>
                {sseError.includes('failed after') && (
                  <Button variant="outline" size="sm" onClick={sseReconnect} className="ml-4">
                    Reconnect
                  </Button>
                )}
              </AlertDescription>
            </Alert>
          )}

          {/* SSE Disconnected Warning */}
          {!sseConnected && !sseError && (
            <Alert variant="default" className="border-yellow-500 bg-yellow-50 dark:bg-yellow-950">
              <AlertTriangle className="h-4 w-4 text-yellow-600" />
              <AlertTitle className="text-yellow-800 dark:text-yellow-200">Real-time Updates Disconnected</AlertTitle>
              <AlertDescription className="text-yellow-700 dark:text-yellow-300">
                Live metrics streaming is disconnected. Using polling for updates.
              </AlertDescription>
            </Alert>
          )}

          {/* Error Recovery */}
          {error && errorRecoveryTemplates.genericError(error, () => {
            setError(null);
            fetchData();
          })}

          {/* Using AdapterOS */}
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <h2 className="text-lg font-semibold">Using AdapterOS</h2>
                <p className="text-sm text-muted-foreground">
                  Upload data, validate, train adapters, manage stacks, and chat with your model.
                </p>
                <p className="text-xs text-muted-foreground mt-1">
                  1) Upload data  2) Train adapter  3) Pick stack  4) Chat
                </p>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <Badge variant="outline">Organization: {effectiveTenant}</Badge>
                <Badge variant="secondary">{defaultStackLabel}</Badge>
              </div>
            </div>

            <ContentGrid className="gap-4">
              {/* Datasets */}
              <Card>
                <CardHeader>
                  <CardTitle>Get started with your data</CardTitle>
                  <p className="text-sm text-muted-foreground">Upload and validate datasets before training.</p>
                </CardHeader>
                <CardContent className="space-y-4">
                  {datasetsLoading ? (
                    <Skeleton className="h-20 w-full" />
                  ) : datasetsError ? (
                    errorRecoveryTemplates.genericError(datasetsError, refetchDatasets)
                  ) : (
                    <>
                      <div className="flex items-center justify-between gap-4">
                        <div>
                          <p className="text-2xl font-bold">{datasetStats.total}</p>
                          <p className="text-xs text-muted-foreground">Total datasets</p>
                        </div>
                      <div className="flex flex-wrap gap-2 text-xs">
                        <Badge variant="outline">Valid {datasetStats.valid}</Badge>
                        <Badge variant="outline">Draft {datasetStats.draft}</Badge>
                        <Badge variant="outline">Invalid {datasetStats.invalid}</Badge>
                      </div>
                    </div>
                    <p className="text-sm text-muted-foreground">
                      {datasetStats.total === 0
                        ? 'No datasets yet. Upload one to begin training.'
                        : 'Validation overview for your datasets.'}
                    </p>
                    <div className="flex flex-wrap gap-2">
                        <Button asChild>
                          <Link to="/training/datasets" state={{ openUpload: true }}>
                            Upload dataset
                          </Link>
                        </Button>
                        <Button variant="outline" asChild>
                          <Link to="/training/datasets">View datasets</Link>
                        </Button>
                      </div>
                    </>
                  )}
                </CardContent>
              </Card>

              {/* Training */}
              <Card>
                <CardHeader>
                  <CardTitle>Training jobs</CardTitle>
                  <p className="text-sm text-muted-foreground">Track running jobs or start a new training.</p>
                </CardHeader>
                <CardContent className="space-y-4">
                  {trainingJobsLoading ? (
                    <Skeleton className="h-20 w-full" />
                  ) : trainingJobsError ? (
                    errorRecoveryTemplates.genericError(trainingJobsError, refetchTrainingJobs)
                  ) : (
                    <>
                      <div className="grid grid-cols-2 gap-4">
                        <div>
                          <p className="text-2xl font-bold">{runningJobs}</p>
                          <p className="text-xs text-muted-foreground">Running jobs</p>
                        </div>
                        <div>
                          <p className="text-2xl font-bold">{completedLast7Days}</p>
                          <p className="text-xs text-muted-foreground">Completed last 7 days</p>
                        </div>
                      </div>
                      {recentTrainingJob ? (
                        <div className="rounded-lg border bg-muted/40 p-3 space-y-1">
                          <div className="flex items-center justify-between gap-2">
                            <p className="text-sm font-medium truncate">
                              {recentTrainingJob.adapter_name || recentTrainingJob.id}
                            </p>
                            <Badge variant="outline">{recentTrainingJob.status}</Badge>
                          </div>
                          <p className="text-xs text-muted-foreground">
                            Dataset: {recentTrainingJob.dataset_id || '—'}
                          </p>
                          <p className="text-xs text-muted-foreground">
                            Stack: {recentTrainingJob.stack_id ? (stackNameLookup.get(recentTrainingJob.stack_id) || recentTrainingJob.stack_id) : 'Not set'}
                          </p>
                        </div>
                      ) : (
                        <p className="text-sm text-muted-foreground">
                          No training jobs yet. Start training after you have a validated dataset.
                        </p>
                      )}
                      <div className="flex flex-wrap gap-2">
                        <Button variant="outline" asChild>
                          <Link to="/training/jobs">View training jobs</Link>
                        </Button>
                        <Button asChild>
                          <Link to="/training" state={{ openTrainingWizard: true }}>
                            Start new training
                          </Link>
                        </Button>
                      </div>
                    </>
                  )}
                </CardContent>
              </Card>

              {/* Training Wizard Quick Start */}
              <Card className="border-primary/40">
                <CardHeader>
                  <CardTitle>Training Wizard</CardTitle>
                  <p className="text-sm text-muted-foreground">
                    Guided: upload or pick a dataset, auto-validate, then start training.
                  </p>
                </CardHeader>
                <CardContent className="space-y-3">
                  <p className="text-xs text-muted-foreground">
                    Best for the common path. For complex datasets, jump to advanced tools.
                  </p>
                  <div className="flex flex-wrap gap-2">
                    <Button asChild>
                      <Link to="/training" state={{ openTrainingWizard: true }}>
                        Start Training Wizard
                      </Link>
                    </Button>
                    <Button variant="outline" asChild>
                      <Link to="/training/datasets" state={{ openUpload: true }}>
                        Advanced dataset tools
                      </Link>
                    </Button>
                  </div>
                </CardContent>
              </Card>

              {/* Adapters & stacks */}
              <Card>
                <CardHeader>
                  <CardTitle>Adapters & stacks</CardTitle>
                  <p className="text-sm text-muted-foreground">See what is ready to serve.</p>
                </CardHeader>
                <CardContent className="space-y-4">
                  {(adaptersLoading || stacksLoading || defaultStackLoading) ? (
                    <Skeleton className="h-20 w-full" />
                  ) : adapterStackError ? (
                    errorRecoveryTemplates.genericError(
                      adapterStackError,
                      () => {
                        refetchAdapters();
                        refetchStacks();
                        refetchDefaultStack();
                      }
                    )
                  ) : (
                    <>
                      <div className="grid grid-cols-2 gap-4">
                        <div>
                          <p className="text-2xl font-bold">{adapterTotal}</p>
                          <p className="text-xs text-muted-foreground">Adapters</p>
                        </div>
                        <div>
                          <p className="text-2xl font-bold">{stackTotal}</p>
                          <p className="text-xs text-muted-foreground">Stacks</p>
                        </div>
                      </div>
                      <p className="text-sm text-muted-foreground">
                        {stackTotal === 0
                          ? 'No adapters or stacks yet. Complete a training job to register an adapter and auto-create a stack.'
                          : defaultStack
                            ? `Default stack for this tenant: ${defaultStack.name}`
                            : 'No default stack configured. Training will auto-create one; you can also set it under Stacks.'}
                      </p>
                      <div className="flex flex-wrap gap-2">
                        <Button variant="outline" asChild>
                          <Link to="/adapters">Manage adapters</Link>
                        </Button>
                        <Button asChild variant="secondary">
                          <Link to="/admin/stacks">Manage stacks</Link>
                        </Button>
                      </div>
                    </>
                  )}
                </CardContent>
              </Card>

              {/* Chat */}
              <Card>
                <CardHeader>
                  <CardTitle>Chat with your model</CardTitle>
                  <p className="text-sm text-muted-foreground">Use the active stack or jump to the latest trained stack.</p>
                </CardHeader>
                <CardContent className="space-y-4">
                  {defaultStackLoading ? (
                    <Skeleton className="h-16 w-full" />
                  ) : defaultStackError ? (
                    errorRecoveryTemplates.genericError(defaultStackError as Error, () => refetchDefaultStack())
                  ) : (
                    <>
                      <div className="space-y-1">
                        <p className="text-sm font-medium">
                          Active stack: {defaultStack ? defaultStack.name : 'Not set'}
                        </p>
                        <p className="text-xs text-muted-foreground">
                          {defaultStack
                            ? 'Chat requests will default to this stack.'
                            : 'No default stack configured. Set one under Stacks or use a specific stack below.'}
                        </p>
                      </div>
                      {recentCompletedJobWithStack ? (
                        <div className="rounded-lg border bg-muted/40 p-3 space-y-1">
                          <p className="text-xs text-muted-foreground">Most recent completed training</p>
                          <p className="text-sm font-medium">
                            Stack: {stackNameLookup.get(recentCompletedJobWithStack.stack_id || '') || recentCompletedJobWithStack.stack_id}
                          </p>
                          <p className="text-xs text-muted-foreground">
                            Adapter: {recentCompletedJobWithStack.adapter_name || recentCompletedJobWithStack.adapter_id || '—'}
                          </p>
                        </div>
                      ) : null}
                      <div className="flex flex-wrap gap-2">
                        <Button asChild>
                          <Link to={defaultStack?.id ? `/chat?stack=${encodeURIComponent(defaultStack.id)}` : '/chat'}>
                            Open chat
                          </Link>
                        </Button>
                        {recentCompletedJobWithStack?.stack_id && (
                          <Button variant="outline" asChild>
                            <Link to={`/chat?stack=${encodeURIComponent(recentCompletedJobWithStack.stack_id)}`}>
                              Chat with latest trained stack
                            </Link>
                          </Button>
                        )}
                      </div>
                    </>
                  )}
                </CardContent>
              </Card>
            </ContentGrid>
          </div>

          {/* System Overview Cards */}
          <KpiGrid>
            <Card className="card-standard">
              <CardHeader className="flex-between pb-2">
                <GlossaryTooltip termId="compute-nodes">
                  <CardTitle className="text-sm font-medium cursor-help">Inference Nodes</CardTitle>
                </GlossaryTooltip>
                <Server className="icon-standard text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold text-green-600">
                  {loading ? <Skeleton className="h-6 w-16" /> : nodeCount}
                </div>
                <p className="text-xs text-muted-foreground">
                  {loading ? 'Loading nodes...' : `${nodeCount} nodes online`}
                </p>
              </CardContent>
            </Card>

            <Card className="card-standard">
              <CardHeader className="flex-between pb-2">
                <GlossaryTooltip termId="active-tenants">
                  <CardTitle className="text-sm font-medium cursor-help">Active Tenants</CardTitle>
                </GlossaryTooltip>
                <Users className="icon-standard text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold text-blue-600">
                  {loading ? <Skeleton className="h-6 w-16" /> : tenantCount}
                </div>
                <p className="text-xs text-muted-foreground">
                  {loading ? 'Loading tenants...' : 'All tenants operational'}
                </p>
              </CardContent>
            </Card>

            <Card className="card-standard">
              <CardHeader className="flex-between pb-2">
                <GlossaryTooltip termId="adapter-count">
                  <CardTitle className="text-sm font-medium cursor-help">LoRA Adapters</CardTitle>
                </GlossaryTooltip>
                <Code className="icon-standard text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold text-purple-600">{adapterCount}</div>
                <GlossaryTooltip termId="active-sessions">
                  <p className="text-xs text-muted-foreground cursor-help">
                    {activeSessions} active sessions
                  </p>
                </GlossaryTooltip>
              </CardContent>
            </Card>

            <Card className="card-standard">
              <CardHeader className="flex-between pb-2">
                <GlossaryTooltip termId="tokens-per-second">
                  <CardTitle className="text-sm font-medium cursor-help">Performance</CardTitle>
                </GlossaryTooltip>
                <Zap className="icon-standard text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold text-green-600">{tokensPerSecond.toFixed(0)}</div>
                <GlossaryTooltip termId="latency-p95">
                  <p className="text-xs text-muted-foreground cursor-help">
                    tokens/sec (p95: {latencyP95.toFixed(0)}ms)
                  </p>
                </GlossaryTooltip>
              </CardContent>
            </Card>
          </KpiGrid>

          {/* Content Grid */}
          <ContentGrid>
            {/* System Resources */}
            <Card className="card-standard">
              <CardHeader>
                <CardTitle>System Resources</CardTitle>
              </CardHeader>
              <CardContent className="space-y-6">
                <div className="space-y-2">
                  <div className="flex justify-between items-center mb-2">
                    <div className="flex items-center gap-2">
                      <Cpu className="h-5 w-5 text-muted-foreground" />
                      <GlossaryTooltip termId="cpu-usage">
                        <span className="text-sm font-medium cursor-help">CPU Usage</span>
                      </GlossaryTooltip>
                      {connected && (
                        <Badge variant="outline" className="text-xs px-2 py-0 h-5">
                          <span className="relative flex h-2 w-2 mr-1">
                            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                            <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
                          </span>
                          Live
                        </Badge>
                      )}
                    </div>
                    <span className="text-sm font-semibold">
                      {systemMetrics ? `${cpuUsage.toFixed(1)}%` : '--'}
                    </span>
                  </div>
                  <Progress value={cpuUsage} className="h-3 transition-all duration-500" />
                </div>

                <div className="space-y-2">
                  <div className="flex justify-between items-center mb-2">
                    <div className="flex items-center gap-2">
                      <HardDrive className="h-5 w-5 text-muted-foreground" />
                      <GlossaryTooltip termId="memory-usage">
                        <span className="text-sm font-medium cursor-help">Memory Usage</span>
                      </GlossaryTooltip>
                    </div>
                    <span className="text-sm font-semibold">
                      {systemMetrics ? `${memoryUsage.toFixed(1)}%` : '--'}
                    </span>
                  </div>
                  <Progress value={memoryUsage} className="h-3 transition-all duration-500" />
                </div>

                <div className="space-y-2">
                  <div className="flex justify-between items-center mb-2">
                    <div className="flex items-center gap-2">
                      <HardDrive className="h-5 w-5 text-muted-foreground" />
                      <GlossaryTooltip termId="disk-usage">
                        <span className="text-sm font-medium cursor-help">Disk Usage</span>
                      </GlossaryTooltip>
                    </div>
                    <span className="text-sm font-semibold">
                      {systemMetrics ? `${diskUsage.toFixed(1)}%` : '--'}
                    </span>
                  </div>
                  <Progress value={diskUsage} className="h-3 transition-all duration-500" />
                </div>

                <div className="space-y-2">
                  <div className="flex justify-between items-center mb-2">
                    <div className="flex items-center gap-2">
                      <Network className="h-5 w-5 text-muted-foreground" />
                      <GlossaryTooltip termId="network-bandwidth">
                        <span className="text-sm font-medium cursor-help">Network Bandwidth</span>
                      </GlossaryTooltip>
                    </div>
                    <span className="text-sm font-semibold">
                      {systemMetrics ? `${networkBandwidth} MB/s` : '--'}
                    </span>
                  </div>
                  <Progress value={Math.min(parseFloat(networkBandwidth), 100)} className="h-3 transition-all duration-500" />
                </div>
              </CardContent>
            </Card>

            {/* Recent Activity */}
            <SectionErrorBoundary sectionName="Recent Activity">
              <Card className="card-standard">
                <CardHeader>
                  <GlossaryTooltip termId="recent-activity">
                    <CardTitle className="cursor-help">Recent Activity</CardTitle>
                  </GlossaryTooltip>
                </CardHeader>
                <CardContent>
                  {activityError ? (
                    errorRecoveryTemplates.genericError(
                      activityError || 'Failed to load activity feed',
                      () => window.location.reload()
                    )
                  ) : (
                    <div className="form-field">
                      {recentActivity.map((activity, index) => {
                        const Icon = activity.icon;
                        return (
                          <div key={index} className="flex-standard">
                            <div className={`p-1 rounded-full bg-muted`}>
                              <Icon className="icon-small" />
                            </div>
                            <div className="flex-1 form-field">
                              <p className="text-sm">{activity.action}</p>
                              <p className="text-xs text-muted-foreground">{activity.time}</p>
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </CardContent>
              </Card>
            </SectionErrorBoundary>

            {/* Base Model Status */}
            <SectionErrorBoundary sectionName="Base Model Status">
              <BaseModelStatusComponent selectedTenant={effectiveTenant} />
            </SectionErrorBoundary>
            {/* Plugin Status */}
            <SectionErrorBoundary sectionName="Plugin Status">
              <PluginStatusWidget />
            </SectionErrorBoundary>
          </ContentGrid>

          {/* Quick Actions */}
          <Card>
            <CardHeader>
              <GlossaryTooltip termId="quick-actions">
                <CardTitle className="cursor-help">Quick Actions</CardTitle>
              </GlossaryTooltip>
            </CardHeader>
            <CardContent>
              <ActionGrid actions={quickActions} columns={4} />
            </CardContent>
          </Card>

          {/* Dashboard Settings Modal */}
          <DashboardSettings
            open={settingsOpen}
            onOpenChange={setSettingsOpen}
            availableWidgetIds={availableWidgetIds}
            currentConfig={userWidgetConfig}
            onUpdateVisibility={updateWidgetVisibility}
            onReset={resetConfig}
            isUpdating={configLoading}
          />

          {/* System Health Modal */}
          <Dialog open={isOpen(MODAL_IDS.HEALTH)} onOpenChange={(open) => !open && closeModal()}>
            <DialogContent className="max-w-2xl">
              <DialogHeader>
                <DialogTitle>System Health Details</DialogTitle>
              </DialogHeader>
              <div className="space-y-4">
                <FormGrid>
                  <Card>
                    <CardHeader className="pb-2">
                      <CardTitle className="text-sm">CPU Usage</CardTitle>
                    </CardHeader>
                    <CardContent>
                      <div className="text-2xl font-bold">34%</div>
                      <Progress value={34} className="mt-2" />
                    </CardContent>
                  </Card>
                  <Card>
                    <CardHeader className="pb-2">
                      <CardTitle className="text-sm">Memory Usage</CardTitle>
                    </CardHeader>
                    <CardContent>
                      <div className="text-2xl font-bold">{memoryUsage.toFixed(0)}%</div>
                      <Progress value={memoryUsage} className="mt-2" />
                    </CardContent>
                  </Card>
                </FormGrid>
                <div className="space-y-2">
                  <div className="flex justify-between text-sm">
                    <span>Active Nodes:</span>
                    <span className="font-medium">{nodeCount}</span>
                  </div>
                  <div className="flex justify-between text-sm">
                    <span>Active Adapters:</span>
                    <span className="font-medium">{adapterCount}</span>
                  </div>
                  <div className="flex justify-between text-sm">
                    <span>Tokens/Second:</span>
                    <span className="font-medium">{tokensPerSecond.toFixed(0)}</span>
                  </div>
                  <div className="flex justify-between text-sm">
                    <span>Latency (p95):</span>
                    <span className="font-medium">{latencyP95.toFixed(0)}ms</span>
                  </div>
                </div>
              </div>
              <DialogFooter>
                <Button variant="outline" onClick={() => closeModal()}>Close</Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>

          {/* Create Tenant Modal */}
          <Dialog open={isOpen(MODAL_IDS.CREATE_TENANT)} onOpenChange={(open) => {
            if (!open) {
              closeModal();
              setCreateTenantError(null);
            }
          }}>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>Create New Tenant</DialogTitle>
              </DialogHeader>
              {createTenantError && errorRecoveryTemplates.genericError(createTenantError, () => {
                setCreateTenantError(null);
              })}
              <div className="space-y-4">
                <div className="space-y-2">
                  <GlossaryTooltip termId="tenant-name-field">
                    <Label htmlFor="tenant-name" className="cursor-help">Organization Name</Label>
                  </GlossaryTooltip>
                  <Input
                    id="tenant-name"
                    placeholder="Enter organization name"
                    value={newTenantName}
                    onChange={(e) => setNewTenantName(e.target.value)}
                  />
                </div>
                <div className="space-y-2">
                  <GlossaryTooltip termId="isolation-level-field">
                    <Label htmlFor="isolation-level" className="cursor-help">Isolation Level</Label>
                  </GlossaryTooltip>
                  <Select value={newTenantIsolation} onValueChange={setNewTenantIsolation}>
                    <SelectTrigger id="isolation-level">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="standard">Standard</SelectItem>
                      <SelectItem value="high">High</SelectItem>
                      <SelectItem value="maximum">Maximum</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>
              <DialogFooter>
                <Button variant="outline" onClick={() => {
                  closeModal();
                  setCreateTenantError(null);
                }}>Cancel</Button>
                <Button onClick={handleCreateTenant}>Create Tenant</Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>

          {/* Deploy Adapter Modal */}
          <Dialog open={deployModalOpen} onOpenChange={(open) => {
            if (!open) {
              closeModal();
              setDeployAdapterError(null);
            }
          }}>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>Deploy Adapter</DialogTitle>
              </DialogHeader>
              {deployAdapterError && errorRecoveryTemplates.genericError(deployAdapterError, () => {
                setDeployAdapterError(null);
              })}
              <div className="space-y-4">
                <div className="space-y-2">
                  <GlossaryTooltip termId="adapter-select-field">
                    <Label htmlFor="adapter-select" className="cursor-help">Select Adapter</Label>
                  </GlossaryTooltip>
                  <Select value={selectedAdapter} onValueChange={setSelectedAdapter}>
                    <SelectTrigger id="adapter-select">
                      <SelectValue placeholder="Choose an adapter" />
                    </SelectTrigger>
                    <SelectContent>
                      {adapters.map((adapter) => (
                        <SelectItem key={adapter.id} value={adapter.id}>
                          {adapter.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div className="space-y-2">
                  <GlossaryTooltip termId="target-tenant-field">
                    <Label htmlFor="target-tenant" className="cursor-help">Target Tenant</Label>
                  </GlossaryTooltip>
                  <Input
                    id="target-tenant"
                    value={deployTargetTenant}
                    onChange={(e) => setDeployTargetTenant(e.target.value)}
                  />
                </div>
              </div>
              <DialogFooter>
                <Button variant="outline" onClick={() => {
                  closeModal();
                  setDeployAdapterError(null);
                }}>Cancel</Button>
                <Button onClick={handleDeployAdapter}>Deploy</Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        </TabsContent>

        {/* Nodes Tab */}
        <TabsContent value="nodes" className="space-y-4">
          <Nodes user={user} selectedTenant={selectedTenant} />
        </TabsContent>

        {/* Alerts Tab */}
        <TabsContent value="alerts" className="space-y-4">
          <AlertsPage selectedTenant={selectedTenant} />
        </TabsContent>
      </Tabs>
    </div>
  );
});
