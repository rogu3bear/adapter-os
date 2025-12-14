// Unified Management Panel for AdapterOS
// Consolidates system overview, service management, and quick actions
import React, { useState } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import apiClient from '@/api/client';
import { logger, toError } from '@/utils/logger';
import { usePolling } from '@/hooks/realtime/usePolling';
import { LoadingState } from './ui/loading-state';
import { LastUpdated } from './ui/last-updated';
import { useRBAC } from '@/hooks/security/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { GlossaryTooltip } from './ui/glossary-tooltip';
import { KpiGrid, ContentGrid } from './ui/grid';
import {
  Activity,
  AlertTriangle,
  CheckCircle,
  XCircle,
  RefreshCw,
  Server,
  Database,
  Users,
  Box,
  Zap,
  Shield,
  Settings,
  Cpu,
  MemoryStick,
  HardDrive,
  TrendingUp,
  Play,
  Square,
  RotateCcw,
  Eye,
  FileText,
  BarChart3,
  ArrowRight,
  Clock,
  Wifi,
  WifiOff,
} from 'lucide-react';
import type {
  SystemMetrics,
  Tenant,
  Node,
  Alert,
  BaseModelStatus,
  Adapter,
} from '@/api/types';
import { useNavigate } from 'react-router-dom';

interface ManagementPanelProps {
  tenantId?: string;
  onToolbarChange?: (actions: React.ReactNode) => void;
}

interface ServiceStatus {
  id: string;
  name: string;
  status: 'running' | 'stopped' | 'error';
  category: 'core' | 'monitoring';
  port?: number;
  lastChecked?: Date;
}

export function ManagementPanel({ tenantId, onToolbarChange }: ManagementPanelProps) {
  const navigate = useNavigate();
  const [activeTab, setActiveTab] = useState('overview');
  const [serviceActions, setServiceActions] = useState<Record<string, boolean>>({});
  const { can, userRole } = useRBAC();

  // Permission checks
  const canManageWorkers = can('worker:manage');
  const canManageNodes = can('node:manage');

  // Fetch comprehensive system data
  const fetchData = async () => {
    const [metricsRes, tenantsRes, nodesRes, alertsRes, modelsRes, adaptersRes] = await Promise.all([
      apiClient.getSystemMetrics().catch(() => null),
      apiClient.listTenants().catch(() => []),
      apiClient.listNodes().catch(() => []),
      apiClient.listAlerts({ limit: 20 }).catch(() => []),
      apiClient.getAllModelsStatus().catch(() => ({ models: [], total_memory_mb: 0, active_model_count: 0 })),
      apiClient.listAdapters().catch(() => []),
    ]);

    return {
      metrics: metricsRes,
      tenants: tenantsRes,
      nodes: nodesRes,
      alerts: alertsRes,
      models: modelsRes.models || [],
      adapters: adaptersRes,
    };
  };

  const {
    data,
    isLoading: loading,
    lastUpdated,
    error: pollingError,
    refetch: refreshData,
  } = usePolling(
    fetchData,
    'normal', // Standard updates for management panel
    {
      showLoadingIndicator: false,
      onError: (err) => {
        logger.error(
          'Failed to fetch management panel data',
          { component: 'ManagementPanel', operation: 'fetchData' },
          toError(err),
        );
      },
    }
  );

  const systemMetrics = data?.metrics || null;
  const tenants = data?.tenants || [];
  const nodes = data?.nodes || [];
  const alerts = data?.alerts || [];
  const models = data?.models || [];
  const adapters = data?.adapters || [];

  // Compute key metrics
  const activeNodes = nodes.filter((n) => n.status === 'healthy').length;
  const activeTenants = tenants.filter((t) => t.status === 'active' || !t.status).length;
  const criticalAlerts = alerts.filter((a) => a.severity === 'critical' && a.status === 'active').length;
  const warningAlerts = alerts.filter((a) => a.severity === 'warning' && a.status === 'active').length;
  const loadedModels = models.filter((m) => m.is_loaded).length;
  const activeAdapters = adapters.filter((a) => a.state === 'hot' || a.state === 'warm').length;

  // Mock service status (in real implementation, fetch from supervisor API)
  const services: ServiceStatus[] = [
    { id: 'backend', name: 'Backend Server', status: 'running', category: 'core', port: 8080 },
    { id: 'ui', name: 'UI Frontend', status: 'running', category: 'core', port: 3200 },
    { id: 'supervisor', name: 'Service Supervisor', status: 'running', category: 'core' },
    { id: 'telemetry', name: 'Telemetry Collector', status: 'running', category: 'monitoring' },
    { id: 'metrics', name: 'Metrics Exporter', status: 'running', category: 'monitoring' },
  ];

  const runningServices = services.filter((s) => s.status === 'running').length;
  const totalServices = services.length;

  // Service control handlers
  const handleServiceAction = async (serviceId: string, action: 'start' | 'stop' | 'restart') => {
    setServiceActions((prev) => ({ ...prev, [serviceId]: true }));
    try {
      // In real implementation, call supervisor API
      logger.info(`Service ${action} requested`, { serviceId, action });
      // Simulate API call
      await new Promise((resolve) => setTimeout(resolve, 500));
      refreshData();
    } catch (error) {
      logger.error('Service action failed', { serviceId, action }, toError(error));
    } finally {
      setServiceActions((prev) => ({ ...prev, [serviceId]: false }));
    }
  };

  // Toolbar actions
  React.useEffect(() => {
    if (!onToolbarChange) return;
    onToolbarChange(
      <div className="flex flex-wrap items-center gap-3">
        {lastUpdated && <LastUpdated timestamp={lastUpdated} className="text-xs text-muted-foreground" />}
        <Button onClick={() => refreshData()} disabled={loading} variant="outline" size="sm">
          <RefreshCw className={`w-4 h-4 mr-2 ${loading ? 'animate-spin' : ''}`} />
          Refresh
        </Button>
      </div>
    );
    return () => onToolbarChange(null);
  }, [onToolbarChange, loading, lastUpdated, refreshData]);

  if (loading && !data) {
    return (
      <LoadingState
        title="Loading management panel"
        description="Collecting system status, services, and metrics..."
        skeletonLines={5}
      />
    );
  }

  // Handle polling error
  if (pollingError && !data) {
    return errorRecoveryTemplates.pollingError(pollingError.message, refreshData);
  }

  return (
    <div className="space-y-6">
      {/* Polling Error Banner (when we have stale data) */}
      {pollingError && data && errorRecoveryTemplates.pollingError(pollingError.message, refreshData)}

      {/* Critical Alerts Banner */}
      {criticalAlerts > 0 && (
        <Card className="border-red-500 bg-red-50 dark:bg-red-950">
          <CardContent className="pt-6">
            <div className="flex items-center gap-3">
              <AlertTriangle className="w-6 h-6 text-red-600" />
              <div className="flex-1">
                <h3 className="font-semibold text-red-900 dark:text-red-100">
                  {criticalAlerts} Critical Alert{criticalAlerts > 1 ? 's' : ''}
                </h3>
                <p className="text-sm text-red-700 dark:text-red-300">
                  Immediate attention required
                </p>
              </div>
              <Button
                variant="destructive"
                onClick={() => navigate('/metrics')}
                className="ml-auto"
              >
                View Alerts
              </Button>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Main Tabs */}
      <Tabs value={activeTab} onValueChange={setActiveTab} className="space-y-6">
        <TabsList className="grid w-full grid-cols-4">
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <GlossaryTooltip termId="management-services">
            <TabsTrigger value="services">Services</TabsTrigger>
          </GlossaryTooltip>
          <GlossaryTooltip termId="management-resources">
            <TabsTrigger value="resources">Resources</TabsTrigger>
          </GlossaryTooltip>
          <GlossaryTooltip termId="management-workers">
            <TabsTrigger value="actions">Quick Actions</TabsTrigger>
          </GlossaryTooltip>
        </TabsList>

        {/* Overview Tab */}
        <TabsContent value="overview" className="space-y-6">
          {/* System Health Summary */}
          <KpiGrid>
            <Card>
              <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                <CardTitle className="text-sm font-medium">System Status</CardTitle>
                {systemMetrics ? (
                  <CheckCircle className="h-4 w-4 text-green-600" />
                ) : (
                  <XCircle className="h-4 w-4 text-red-600" />
                )}
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">
                  {systemMetrics ? 'Healthy' : 'Degraded'}
                </div>
                <p className="text-xs text-muted-foreground">
                  {runningServices}/{totalServices} services running
                </p>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                <CardTitle className="text-sm font-medium">Active Tenants</CardTitle>
                <Users className="h-4 w-4 text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{activeTenants}</div>
                <p className="text-xs text-muted-foreground">
                  {tenants.length} total tenant{tenants.length !== 1 ? 's' : ''}
                </p>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                <CardTitle className="text-sm font-medium">Loaded Models</CardTitle>
                <Database className="h-4 w-4 text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{loadedModels}</div>
                <p className="text-xs text-muted-foreground">
                  {models.length} total model{models.length !== 1 ? 's' : ''}
                </p>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                <CardTitle className="text-sm font-medium">Active Adapters</CardTitle>
                <Box className="h-4 w-4 text-muted-foreground" />
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{activeAdapters}</div>
                <p className="text-xs text-muted-foreground">
                  {adapters.length} total adapter{adapters.length !== 1 ? 's' : ''}
                </p>
              </CardContent>
            </Card>
          </KpiGrid>

          {/* Resource Usage */}
          {systemMetrics && (
            <div className="grid gap-4 md:grid-cols-3">
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm font-medium flex items-center gap-2">
                    <Cpu className="h-4 w-4" />
                    CPU Usage
                    <GlossaryTooltip termId="cpu-usage" />
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="text-2xl font-bold">
                    {systemMetrics.cpu_usage_percent?.toFixed(1) || '0'}%
                  </div>
                  <div className="mt-2 h-2 w-full bg-muted rounded-full overflow-hidden">
                    <div
                      className="h-full bg-primary transition-all"
                      style={{ width: `${systemMetrics.cpu_usage_percent || 0}%` }}
                    />
                  </div>
                </CardContent>
              </Card>

              <Card>
                <CardHeader>
                  <CardTitle className="text-sm font-medium flex items-center gap-2">
                    <MemoryStick className="h-4 w-4" />
                    Memory Usage
                    <GlossaryTooltip termId="memory-usage" />
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="text-2xl font-bold">
                    {systemMetrics.memory_used_gb
                      ? `${systemMetrics.memory_used_gb.toFixed(1)} GB`
                      : '0 GB'}
                  </div>
                  <p className="text-xs text-muted-foreground mt-1">
                    {systemMetrics.memory_total_gb
                      ? `of ${systemMetrics.memory_total_gb.toFixed(1)} GB`
                      : ''}
                  </p>
                  <div className="mt-2 h-2 w-full bg-muted rounded-full overflow-hidden">
                    <div
                      className="h-full bg-primary transition-all"
                      style={{
                        width: `${
                          systemMetrics.memory_total_gb && systemMetrics.memory_used_gb
                            ? (systemMetrics.memory_used_gb / systemMetrics.memory_total_gb) * 100
                            : 0
                        }%`,
                      }}
                    />
                  </div>
                </CardContent>
              </Card>

              <Card>
                <CardHeader>
                  <CardTitle className="text-sm font-medium flex items-center gap-2">
                    <HardDrive className="h-4 w-4" />
                    Disk Usage
                    <GlossaryTooltip termId="disk-usage" />
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="text-2xl font-bold">
                    {systemMetrics.disk_used_gb?.toFixed(1) || '0'} GB
                  </div>
                  <p className="text-xs text-muted-foreground mt-1">
                    {systemMetrics.disk_total_gb
                      ? `of ${systemMetrics.disk_total_gb.toFixed(1)} GB`
                      : ''}
                  </p>
                  <div className="mt-2 h-2 w-full bg-muted rounded-full overflow-hidden">
                    <div
                      className="h-full bg-primary transition-all"
                      style={{
                        width: `${
                          systemMetrics.disk_total_gb && systemMetrics.disk_used_gb
                            ? (systemMetrics.disk_used_gb / systemMetrics.disk_total_gb) * 100
                            : 0
                        }%`,
                      }}
                    />
                  </div>
                </CardContent>
              </Card>
            </div>
          )}

          {/* Recent Alerts */}
          {alerts.length > 0 && (
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center justify-between">
                  <span>Recent Alerts</span>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => navigate('/metrics')}
                  >
                    View All
                    <ArrowRight className="ml-2 h-4 w-4" />
                  </Button>
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="space-y-2">
                  {alerts.slice(0, 5).map((alert) => (
                    <div
                      key={alert.id}
                      className="flex items-center justify-between p-2 rounded-lg border"
                    >
                      <div className="flex items-center gap-2">
                        {alert.severity === 'critical' ? (
                          <AlertTriangle className="h-4 w-4 text-red-600" />
                        ) : (
                          <AlertTriangle className="h-4 w-4 text-yellow-600" />
                        )}
                        <span className="text-sm font-medium">{alert.title}</span>
                        <Badge variant={alert.severity === 'critical' ? 'destructive' : 'secondary'}>
                          {alert.severity}
                        </Badge>
                      </div>
                      <span className="text-xs text-muted-foreground">
                        {alert.created_at ? new Date(alert.created_at).toLocaleTimeString() : 'N/A'}
                      </span>
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>
          )}
        </TabsContent>

        {/* Services Tab */}
        <TabsContent value="services" className="space-y-6">
          <Card>
            <CardHeader>
              <CardTitle>Service Management</CardTitle>
              <CardDescription>
                Monitor and control core services and monitoring tools
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-4">
                {services.map((service) => (
                  <div
                    key={service.id}
                    className="flex items-center justify-between p-4 border rounded-lg"
                  >
                    <div className="flex items-center gap-3">
                      <div
                        className={`h-3 w-3 rounded-full ${
                          service.status === 'running'
                            ? 'bg-green-500'
                            : service.status === 'error'
                              ? 'bg-red-500'
                              : 'bg-gray-400'
                        }`}
                      />
                      <div>
                        <div className="font-medium">{service.name}</div>
                        <div className="text-sm text-muted-foreground">
                          {service.category} •{' '}
                          {service.port ? `Port ${service.port}` : 'No port'}
                        </div>
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <Badge
                        variant={
                          service.status === 'running'
                            ? 'default'
                            : service.status === 'error'
                              ? 'destructive'
                              : 'secondary'
                        }
                      >
                        {service.status}
                      </Badge>
                      {canManageWorkers && (
                        <div className="flex gap-1">
                          {service.status === 'running' ? (
                            <>
                              <GlossaryTooltip brief="Restart service">
                                <Button
                                  size="sm"
                                  variant="outline"
                                  onClick={() => handleServiceAction(service.id, 'restart')}
                                  disabled={serviceActions[service.id]}
                                >
                                  <RotateCcw className="h-4 w-4" />
                                </Button>
                              </GlossaryTooltip>
                              <GlossaryTooltip brief="Stop service">
                                <Button
                                  size="sm"
                                  variant="outline"
                                  onClick={() => handleServiceAction(service.id, 'stop')}
                                  disabled={serviceActions[service.id]}
                                >
                                  <Square className="h-4 w-4" />
                                </Button>
                              </GlossaryTooltip>
                            </>
                          ) : (
                            <GlossaryTooltip brief="Start service">
                              <Button
                                size="sm"
                                variant="outline"
                                onClick={() => handleServiceAction(service.id, 'start')}
                                disabled={serviceActions[service.id]}
                              >
                                <Play className="h-4 w-4" />
                              </Button>
                            </GlossaryTooltip>
                          )}
                        </div>
                      )}
                      {!canManageWorkers && (
                        <GlossaryTooltip termId="requires-admin">
                          <span className="text-xs text-muted-foreground">No permission</span>
                        </GlossaryTooltip>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>

          {/* Node Status */}
          {nodes.length > 0 && (
            <Card>
              <CardHeader>
                <CardTitle>Compute Nodes</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="space-y-2">
                  {nodes.map((node) => (
                    <div
                      key={node.id}
                      className="flex items-center justify-between p-2 rounded-lg border"
                    >
                      <div className="flex items-center gap-2">
                        {node.status === 'healthy' ? (
                          <Wifi className="h-4 w-4 text-green-600" />
                        ) : (
                          <WifiOff className="h-4 w-4 text-gray-400" />
                        )}
                        <span className="font-medium">{node.id}</span>
                        <Badge variant={node.status === 'healthy' ? 'default' : 'secondary'}>
                          {node.status}
                        </Badge>
                      </div>
                      {canManageNodes ? (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => navigate('/admin/tenants')}
                        >
                          View
                        </Button>
                      ) : (
                        <GlossaryTooltip termId="requires-admin">
                          <span className="text-xs text-muted-foreground">View only</span>
                        </GlossaryTooltip>
                      )}
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>
          )}
        </TabsContent>

        {/* Resources Tab */}
        <TabsContent value="resources" className="space-y-6">
          <ContentGrid>
            {/* Tenants */}
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center justify-between">
                  <span className="flex items-center gap-2">
                    <Users className="h-5 w-5" />
                    Tenants
                  </span>
                  <Button variant="ghost" size="sm" onClick={() => navigate('/admin/tenants')}>
                    Manage
                    <ArrowRight className="ml-2 h-4 w-4" />
                  </Button>
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="space-y-2">
                  {tenants.slice(0, 5).map((tenant) => (
                    <div
                      key={tenant.id}
                      className="flex items-center justify-between p-2 rounded-lg border"
                    >
                      <div>
                        <div className="font-medium">{tenant.name || tenant.id}</div>
                        <div className="text-sm text-muted-foreground">
                          {tenant.status || 'active'}
                        </div>
                      </div>
                      <Badge variant={tenant.status === 'active' ? 'default' : 'secondary'}>
                        {tenant.status || 'active'}
                      </Badge>
                    </div>
                  ))}
                  {tenants.length > 5 && (
                    <p className="text-sm text-muted-foreground text-center pt-2">
                      +{tenants.length - 5} more
                    </p>
                  )}
                </div>
              </CardContent>
            </Card>

            {/* Adapters */}
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center justify-between">
                  <span className="flex items-center gap-2">
                    <Box className="h-5 w-5" />
                    Adapters
                  </span>
                  <Button variant="ghost" size="sm" onClick={() => navigate('/adapters')}>
                    Manage
                    <ArrowRight className="ml-2 h-4 w-4" />
                  </Button>
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="space-y-2">
                  {adapters.slice(0, 5).map((adapter) => (
                    <div
                      key={adapter.id}
                      className="flex items-center justify-between p-2 rounded-lg border"
                    >
                      <div>
                        <div className="font-medium">{adapter.id}</div>
                        <div className="text-sm text-muted-foreground">
                          {adapter.state} • Rank {adapter.rank}
                        </div>
                      </div>
                      <Badge
                        variant={
                          adapter.state === 'hot'
                            ? 'default'
                            : adapter.state === 'warm'
                              ? 'secondary'
                              : 'outline'
                        }
                      >
                        {adapter.state}
                      </Badge>
                    </div>
                  ))}
                  {adapters.length > 5 && (
                    <p className="text-sm text-muted-foreground text-center pt-2">
                      +{adapters.length - 5} more
                    </p>
                  )}
                </div>
              </CardContent>
            </Card>

            {/* Models */}
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center justify-between">
                  <span className="flex items-center gap-2">
                    <Database className="h-5 w-5" />
                    Base Models
                  </span>
                  <Button variant="ghost" size="sm" onClick={() => navigate('/base-models')}>
                    Manage
                    <ArrowRight className="ml-2 h-4 w-4" />
                  </Button>
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="space-y-2">
                  {models.slice(0, 5).map((model) => (
                    <div
                      key={model.model_id}
                      className="flex items-center justify-between p-2 rounded-lg border"
                    >
                      <div>
                        <div className="font-medium">{model.model_id}</div>
                        <div className="text-sm text-muted-foreground">
                          {model.is_loaded ? 'Loaded' : 'Not loaded'}
                        </div>
                      </div>
                      <Badge variant={model.is_loaded ? 'default' : 'secondary'}>
                        {model.is_loaded ? 'Loaded' : 'Idle'}
                      </Badge>
                    </div>
                  ))}
                  {models.length > 5 && (
                    <p className="text-sm text-muted-foreground text-center pt-2">
                      +{models.length - 5} more
                    </p>
                  )}
                </div>
              </CardContent>
            </Card>

            {/* Policies */}
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center justify-between">
                  <span className="flex items-center gap-2">
                    <Shield className="h-5 w-5" />
                    Policies
                  </span>
                  <Button variant="ghost" size="sm" onClick={() => navigate('/security/policies')}>
                    Configure
                    <ArrowRight className="ml-2 h-4 w-4" />
                  </Button>
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-sm text-muted-foreground">
                  <p>20 canonical policy packs</p>
                  <p className="mt-2">Egress, Determinism, Router, Evidence, and more</p>
                </div>
              </CardContent>
            </Card>
          </ContentGrid>
        </TabsContent>

        {/* Quick Actions Tab */}
        <TabsContent value="actions" className="space-y-6">
          <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
            {/* ML Pipeline Actions */}
            <Card className="cursor-pointer hover:bg-muted/50 transition-colors" onClick={() => navigate('/trainer')}>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  <Zap className="h-5 w-5" />
                  Train Adapter
                </CardTitle>
                <CardDescription>Start training a new LoRA adapter</CardDescription>
              </CardHeader>
            </Card>

            <Card className="cursor-pointer hover:bg-muted/50 transition-colors" onClick={() => navigate('/training')}>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  <Activity className="h-5 w-5" />
                  Training Jobs
                </CardTitle>
                <CardDescription>Monitor active training sessions</CardDescription>
              </CardHeader>
            </Card>

            <Card className="cursor-pointer hover:bg-muted/50 transition-colors" onClick={() => navigate('/adapters')}>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  <Box className="h-5 w-5" />
                  Manage Adapters
                </CardTitle>
                <CardDescription>View and configure adapters</CardDescription>
              </CardHeader>
            </Card>

            {/* Operations Actions */}
            <Card className="cursor-pointer hover:bg-muted/50 transition-colors" onClick={() => navigate('/inference')}>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  <Play className="h-5 w-5" />
                  Run Inference
                </CardTitle>
                <CardDescription>Test inference with adapters</CardDescription>
              </CardHeader>
            </Card>

            <Card className="cursor-pointer hover:bg-muted/50 transition-colors" onClick={() => navigate('/telemetry')}>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  <Eye className="h-5 w-5" />
                  Telemetry
                </CardTitle>
                <CardDescription>View event logs and monitoring</CardDescription>
              </CardHeader>
            </Card>

            <Card className="cursor-pointer hover:bg-muted/50 transition-colors" onClick={() => navigate('/replay')}>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  <RotateCcw className="h-5 w-5" />
                  Replay
                </CardTitle>
                <CardDescription>Replay inference traces</CardDescription>
              </CardHeader>
            </Card>

            {/* Monitoring Actions */}
            <Card className="cursor-pointer hover:bg-muted/50 transition-colors" onClick={() => navigate('/metrics')}>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  <BarChart3 className="h-5 w-5" />
                  Metrics
                </CardTitle>
                <CardDescription>System performance metrics</CardDescription>
              </CardHeader>
            </Card>

            <Card className="cursor-pointer hover:bg-muted/50 transition-colors" onClick={() => navigate('/metrics')}>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  <Activity className="h-5 w-5" />
                  System Health
                </CardTitle>
                <CardDescription>Health monitoring and alerts</CardDescription>
              </CardHeader>
            </Card>

            <Card className="cursor-pointer hover:bg-muted/50 transition-colors" onClick={() => navigate('/routing')}>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  <TrendingUp className="h-5 w-5" />
                  Routing Inspector
                </CardTitle>
                <CardDescription>Analyze adapter routing decisions</CardDescription>
              </CardHeader>
            </Card>

            {/* Compliance Actions */}
            <Card className="cursor-pointer hover:bg-muted/50 transition-colors" onClick={() => navigate('/security/policies')}>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  <Shield className="h-5 w-5" />
                  Policies
                </CardTitle>
                <CardDescription>Configure security policies</CardDescription>
              </CardHeader>
            </Card>

            <Card className="cursor-pointer hover:bg-muted/50 transition-colors" onClick={() => navigate('/security/audit')}>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  <FileText className="h-5 w-5" />
                  Audit Logs
                </CardTitle>
                <CardDescription>Security audit trails</CardDescription>
              </CardHeader>
            </Card>

            {/* Administration Actions */}
            <Card className="cursor-pointer hover:bg-muted/50 transition-colors" onClick={() => navigate('/admin')}>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  <Settings className="h-5 w-5" />
                  IT Admin
                </CardTitle>
                <CardDescription>System administration</CardDescription>
              </CardHeader>
            </Card>
          </div>
        </TabsContent>
      </Tabs>
    </div>
  );
}

