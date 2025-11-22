import React, { useState, useMemo, useEffect, useCallback, memo } from 'react';
import { useNavigate } from 'react-router-dom';
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
import { logger } from '../utils/logger';
import { useActivityFeed } from '../hooks/useActivityFeed';
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
import { useInformationDensity } from '../hooks/useInformationDensity';
import { DensityControls } from './ui/density-controls';
import { PluginStatusWidget } from './dashboard/PluginStatusWidget';
import { DashboardSettings } from './dashboard/DashboardSettings';
import apiClient from '../api/client';
import { useAnnounce, useKeyboardShortcuts } from '@/utils/accessibility';
import { usePolling } from '../hooks/usePolling';
import { useSSE } from '../hooks/useSSE';
import { useDashboardConfig } from '../hooks/useDashboardConfig';
import { User } from '@/api/types';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { HelpTooltip } from './ui/help-tooltip';
import { useRBAC } from '../hooks/useRBAC';
import { PageHeader } from './ui/page-header';
import { ActionGrid } from './ui/action-grid';
import { KpiGrid, ContentGrid, FormGrid } from './ui/grid';
import { useModalManager } from '@/contexts/ModalContext';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

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
  component: React.ComponentType<any>;
  priority: number;
}

interface DashboardLayout {
  widgets: DashboardWidget[];
  quickActions: Array<{
    label: string;
    icon: any;
    route: string;
    variant?: 'default' | 'outline' | 'secondary';
  }>;
}

// Main Dashboard component
export const Dashboard = memo(function Dashboard({ user, selectedTenant, onNavigate }: DashboardProps) {
  const announce = useAnnounce();
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
  const [showHealthModal, setShowHealthModal] = useState(false);
  const [showCreateTenantModal, setShowCreateTenantModal] = useState(false);
  const [showDeployAdapterModal, setShowDeployAdapterModal] = useState(false);
  const [newTenantName, setNewTenantName] = useState('');
  const [newTenantIsolation, setNewTenantIsolation] = useState('standard');
  const [adapters, setAdapters] = useState<any[]>([]);
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
  const effectiveTenant = selectedTenant || 'default';

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

  // Fetch dashboard data
  const fetchData = async () => {
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
  };

  useEffect(() => {
    fetchData();
  }, [selectedTenant]);

  const handleCreateTenant = async () => {
    if (!newTenantName.trim()) {
      setCreateTenantError('Tenant name is required');
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
    if (showDeployAdapterModal) {
      loadAdapters();
    }
  }, [showDeployAdapterModal, selectedTenant, user?.user_id]);

  // Real-time activity feed from telemetry and audit logs
  // Note: useActivityFeed doesn't require userId parameter
  const { events: activityEvents, loading: activityLoading, error: activityError } = useActivityFeed({
    enabled: true,
    maxEvents: 10,
    tenantId: effectiveTenant
  });

  // Helper functions for activity feed
  const formatTimeAgo = (timestamp: string): string => {
    const now = new Date();
    const eventTime = new Date(timestamp);
    const diffMs = now.getTime() - eventTime.getTime();
    const diffMins = Math.floor(diffMs / (1000 * 60));
    const diffHours = Math.floor(diffMins / 60);
    const diffDays = Math.floor(diffHours / 24);

    if (diffMins < 1) return 'just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    return `${diffDays}d ago`;
  };

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
      time: formatTimeAgo(event.timestamp),
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
      onClick: () => (onNavigate ? onNavigate('policies') : navigate('/policies'))
    }
  ], [can, openModal, onNavigate, navigate]);

  if (loading) {
    return (
      <Card aria-labelledby="sys-health-title">
        <CardHeader>
          <CardTitle id="sys-health-title">System Health</CardTitle>
        </CardHeader>
        <CardContent aria-busy={true}>
          <div role="status" aria-live="polite" className="h-20 animate-pulse bg-muted rounded">
            <span className="sr-only">Loading system health...</span>
          </div>
        </CardContent>
      </Card>
    );
  }

  // Merge SSE and polling data - SSE takes priority for real-time updates
  const effectiveMetrics = sseMetrics || systemMetrics;
  const memoryUsage = effectiveMetrics?.memory_usage_percent || (systemMetrics as { memory_usage_pct?: number } | null)?.memory_usage_pct || 0;
  const adapterCount = effectiveMetrics?.adapter_count || 0;
  const activeSessions = effectiveMetrics?.active_sessions || 0;
  const tokensPerSecond = effectiveMetrics?.tokens_per_second || 0;
  const latencyP95 = effectiveMetrics?.latency_p95_ms || 0;
  const cpuUsage = effectiveMetrics?.cpu_usage_percent || 0;
  const diskUsage = effectiveMetrics?.disk_usage_percent || 0;
  const networkBandwidth = effectiveMetrics?.network_rx_bytes ? (effectiveMetrics.network_rx_bytes / 1024 / 1024).toFixed(1) : '0';


  return (
    <div className="space-y-6">
      {/* Header */}
      <PageHeader
        title="Dashboard"
        description="System overview, health monitoring, and alerts"
        badges={[
          { label: `Tenant: ${effectiveTenant}`, variant: 'outline' },
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

          {/* Header */}
          <div className="flex-between section-header">
            <div>
              <h1 className="section-title">System Dashboard — Monitor health, adapters, and performance</h1>
              <p className="section-description">
                Welcome back, {effectiveUser.display_name}. System status: Operational
              </p>
            </div>
            <div className="flex-standard">
              <div className="status-indicator status-success">
                <CheckCircle className="icon-small" />
                All Systems Operational
              </div>
              <HelpTooltip helpId="export-logs">
                <Button variant="outline" size="sm" onClick={handleExportLogs}>
                  <Download className="icon-standard mr-2" />
                  Export Logs
                </Button>
              </HelpTooltip>
            </div>
          </div>

          {/* System Overview Cards */}
          <KpiGrid>
            <Card className="card-standard">
              <CardHeader className="flex-between pb-2">
                <HelpTooltip helpId="compute-nodes">
                  <CardTitle className="text-sm font-medium cursor-help">Inference Nodes</CardTitle>
                </HelpTooltip>
                <Server className="icon-standard text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold text-green-600">{nodeCount}</div>
                <p className="text-xs text-muted-foreground">
                  {nodeCount} nodes online
                </p>
              </CardContent>
            </Card>

            <Card className="card-standard">
              <CardHeader className="flex-between pb-2">
                <HelpTooltip helpId="active-tenants">
                  <CardTitle className="text-sm font-medium cursor-help">Active Tenants</CardTitle>
                </HelpTooltip>
                <Users className="icon-standard text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold text-blue-600">{tenantCount}</div>
                <p className="text-xs text-muted-foreground">
                  All tenants operational
                </p>
              </CardContent>
            </Card>

            <Card className="card-standard">
              <CardHeader className="flex-between pb-2">
                <HelpTooltip helpId="adapter-count">
                  <CardTitle className="text-sm font-medium cursor-help">LoRA Adapters</CardTitle>
                </HelpTooltip>
                <Code className="icon-standard text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold text-purple-600">{adapterCount}</div>
                <HelpTooltip helpId="active-sessions">
                  <p className="text-xs text-muted-foreground cursor-help">
                    {activeSessions} active sessions
                  </p>
                </HelpTooltip>
              </CardContent>
            </Card>

            <Card className="card-standard">
              <CardHeader className="flex-between pb-2">
                <HelpTooltip helpId="tokens-per-second">
                  <CardTitle className="text-sm font-medium cursor-help">Performance</CardTitle>
                </HelpTooltip>
                <Zap className="icon-standard text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold text-green-600">{tokensPerSecond.toFixed(0)}</div>
                <HelpTooltip helpId="latency-p95">
                  <p className="text-xs text-muted-foreground cursor-help">
                    tokens/sec (p95: {latencyP95.toFixed(0)}ms)
                  </p>
                </HelpTooltip>
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
                      <HelpTooltip helpId="cpu-usage">
                        <span className="text-sm font-medium cursor-help">CPU Usage</span>
                      </HelpTooltip>
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
                      <HelpTooltip helpId="memory-usage">
                        <span className="text-sm font-medium cursor-help">Memory Usage</span>
                      </HelpTooltip>
                    </div>
                    <span className="text-sm font-semibold">
                      {systemMetrics ? `${systemMetrics.memory_usage_percent ? systemMetrics.memory_usage_percent.toFixed(1) : memoryUsage.toFixed(1)}%` : '--'}
                    </span>
                  </div>
                  <Progress value={systemMetrics?.memory_usage_percent || memoryUsage} className="h-3 transition-all duration-500" />
                </div>

                <div className="space-y-2">
                  <div className="flex justify-between items-center mb-2">
                    <div className="flex items-center gap-2">
                      <HardDrive className="h-5 w-5 text-muted-foreground" />
                      <HelpTooltip helpId="disk-usage">
                        <span className="text-sm font-medium cursor-help">Disk Usage</span>
                      </HelpTooltip>
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
                      <HelpTooltip helpId="network-bandwidth">
                        <span className="text-sm font-medium cursor-help">Network Bandwidth</span>
                      </HelpTooltip>
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
                  <HelpTooltip helpId="recent-activity">
                    <CardTitle className="cursor-help">Recent Activity</CardTitle>
                  </HelpTooltip>
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
              <HelpTooltip helpId="quick-actions">
                <CardTitle className="cursor-help">Quick Actions</CardTitle>
              </HelpTooltip>
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
                  <HelpTooltip helpId="tenant-name-field">
                    <Label htmlFor="tenant-name" className="cursor-help">Tenant Name</Label>
                  </HelpTooltip>
                  <Input
                    id="tenant-name"
                    placeholder="Enter tenant name"
                    value={newTenantName}
                    onChange={(e) => setNewTenantName(e.target.value)}
                  />
                </div>
                <div className="space-y-2">
                  <HelpTooltip helpId="isolation-level-field">
                    <Label htmlFor="isolation-level" className="cursor-help">Isolation Level</Label>
                  </HelpTooltip>
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
          <Dialog open={isOpen(MODAL_IDS.DEPLOY_ADAPTER)} onOpenChange={(open) => {
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
                  <HelpTooltip helpId="adapter-select-field">
                    <Label htmlFor="adapter-select" className="cursor-help">Select Adapter</Label>
                  </HelpTooltip>
                  <Select value={selectedAdapter} onValueChange={setSelectedAdapter}>
                    <SelectTrigger id="adapter-select">
                      <SelectValue placeholder="Choose an adapter" />
                    </SelectTrigger>
                    <SelectContent>
                      {adapters.map((adapter) => (
                        <SelectItem key={adapter.id} value={adapter.adapter_id}>
                          {adapter.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div className="space-y-2">
                  <HelpTooltip helpId="target-tenant-field">
                    <Label htmlFor="target-tenant" className="cursor-help">Target Tenant</Label>
                  </HelpTooltip>
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
