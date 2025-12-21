// 【ui/src/components/ITAdminDashboard.tsx§74-78】 - Replace manual polling with standardized hook
import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { apiClient } from '@/api/services';
import { logger, toError } from '@/utils/logger';
import { usePolling } from '@/hooks/realtime/usePolling';
import { LastUpdated } from './ui/last-updated';
import { LoadingState } from './ui/loading-state';
import { KpiGrid, ContentGrid } from './ui/grid';
import {
  Users,
  Server,
  Database,
  Activity,
  AlertTriangle,
  CheckCircle,
  XCircle,
  RefreshCw,
  TrendingUp,
  HardDrive,
  Cpu,
  MemoryStick
} from 'lucide-react';
import type { 
  SystemMetrics, 
  Tenant, 
  Node, 
  Alert,
  BaseModelStatus,
  Adapter
} from '@/api/types';

interface ITAdminDashboardProps {
  tenantId?: string;
  onToolbarChange?: (actions: React.ReactNode) => void;
}

interface AdminToolbarProps {
  loading: boolean;
  lastUpdated?: Date | null;
  onRefresh: () => void;
}

function AdminToolbar({ loading, lastUpdated, onRefresh }: AdminToolbarProps) {
  return (
    <div className="flex flex-wrap items-center gap-3">
      {lastUpdated && <LastUpdated timestamp={lastUpdated} className="text-xs text-muted-foreground" />}
      <Button onClick={onRefresh} disabled={loading} variant="outline" size="sm">
        <RefreshCw className={`w-4 h-4 mr-2 ${loading ? 'animate-spin' : ''}`} />
        Refresh
      </Button>
    </div>
  );
}

export function ITAdminDashboard({ tenantId, onToolbarChange }: ITAdminDashboardProps) {
  // 【ui/src/hooks/usePolling.ts】 - Standardized polling hook
  const fetchData = async () => {
    const [metricsRes, tenantsRes, nodesRes, alertsRes, modelsRes, adaptersRes] = await Promise.all([
      apiClient.getSystemMetrics().catch(() => null),
      apiClient.listTenants().catch(() => []),
      apiClient.listNodes().catch(() => []),
      apiClient.listAlerts({ limit: 10 }).catch(() => []),
      apiClient.getAllModelsStatus().catch(() => ({ models: [], total_memory_mb: 0, active_model_count: 0 })),
      apiClient.listAdapters().catch(() => [])
    ]);

    return {
      metrics: metricsRes,
      tenants: tenantsRes,
      nodes: nodesRes,
      alerts: alertsRes,
      models: modelsRes.models || [],
      adapters: adaptersRes
    };
  };

  const { 
    data, 
    isLoading: loading, 
    lastUpdated, 
    error: pollingError,
    refetch: refreshData 
  } = usePolling(
    fetchData,
    'slow', // Background updates (system health, admin)
    {
      showLoadingIndicator: false,
      onError: (err) => {
        logger.error(
          'Failed to fetch admin data',
          { component: 'ITAdminDashboard', operation: 'fetchAdminData' },
          toError(err),
        );
      }
    }
  );

  const systemMetrics = data?.metrics || null;
  const tenants = data?.tenants || [];
  const nodes = data?.nodes || [];
  const alerts = data?.alerts || [];
  const models = data?.models || [];
  const adapters = data?.adapters || [];

  const activeNodes = nodes.filter(n => n.status === 'healthy').length;
  const activeTenants = tenants.filter(t => t.status === 'active' || !t.status).length;
  const criticalAlerts = alerts.filter(a => a.severity === 'critical' && a.status === 'active').length;
  const loadedModels = models.filter(m => m.is_loaded).length;

  React.useEffect(() => {
    if (!onToolbarChange) return;
    onToolbarChange(
      <AdminToolbar loading={loading} lastUpdated={lastUpdated ?? null} onRefresh={() => refreshData()} />
    );
    return () => onToolbarChange(null);
  }, [onToolbarChange, loading, lastUpdated, refreshData]);

  if (loading) {
    return (
      <LoadingState
        title="Loading admin insights"
        description="Collecting system health, tenant status, and alerts."
        skeletonLines={3}
      />
    );
  }

  return (
    <div className="space-y-6">

      {/* Critical Alerts Banner */}
      {criticalAlerts > 0 && (
        <Card className="border-red-500 bg-red-50 dark:bg-red-950">
          <CardContent className="pt-6">
            <div className="flex items-center gap-3">
              <AlertTriangle className="w-6 h-6 text-red-600" />
              <div>
                <h3 className="font-semibold text-red-900 dark:text-red-100">
                  {criticalAlerts} Critical Alert{criticalAlerts > 1 ? 's' : ''}
                </h3>
                <p className="text-sm text-red-700 dark:text-red-300">
                  Immediate attention required
                </p>
              </div>
              <Button variant="destructive" className="ml-auto">
                View Alerts
              </Button>
            </div>
          </CardContent>
        </Card>
      )}

      {/* System Overview Grid */}
      <KpiGrid>
        {/* System Health */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">System Health</CardTitle>
            <Activity className="w-4 h-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-green-600">
              {systemMetrics ? 'Healthy' : 'Unknown'}
            </div>
            <p className="text-xs text-muted-foreground mt-1">
              All systems operational
            </p>
          </CardContent>
        </Card>

        {/* Active Tenants */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Active Tenants</CardTitle>
            <Users className="w-4 h-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{activeTenants}</div>
            <p className="text-xs text-muted-foreground mt-1">
              {tenants.length} total tenants
            </p>
          </CardContent>
        </Card>

        {/* Nodes Online */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Nodes Online</CardTitle>
            <Server className="w-4 h-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{activeNodes}</div>
            <p className="text-xs text-muted-foreground mt-1">
              {nodes.length} total nodes
            </p>
          </CardContent>
        </Card>

        {/* Loaded Models */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Models Loaded</CardTitle>
            <Database className="w-4 h-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{loadedModels}</div>
            <p className="text-xs text-muted-foreground mt-1">
              {models.length} total models
            </p>
          </CardContent>
        </Card>
      </KpiGrid>

      {/* Resource Usage */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <TrendingUp className="w-5 h-5" />
            Resource Usage
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            {/* CPU Usage */}
            <div>
              <div className="flex items-center justify-between mb-2">
                <div className="flex items-center gap-2">
                  <Cpu className="w-4 h-4 text-muted-foreground" />
                  <span className="text-sm font-medium">CPU</span>
                </div>
                <span className="text-sm font-bold">
                  {(systemMetrics?.cpu_usage ?? systemMetrics?.cpu_usage_percent ?? 0).toFixed(1)}%
                </span>
              </div>
              <div className="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
                <div
                  className="bg-blue-600 h-2 rounded-full transition-all"
                  style={{ width: `${systemMetrics?.cpu_usage ?? systemMetrics?.cpu_usage_percent ?? 0}%` }}
                />
              </div>
            </div>

            {/* Memory Usage */}
            <div>
              <div className="flex items-center justify-between mb-2">
                <div className="flex items-center gap-2">
                  <MemoryStick className="w-4 h-4 text-muted-foreground" />
                  <span className="text-sm font-medium">Memory</span>
                </div>
                <span className="text-sm font-bold">
                  {(systemMetrics?.memory_usage ?? systemMetrics?.memory_usage_percent ?? 0).toFixed(1)}%
                </span>
              </div>
              <div className="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
                <div
                  className="bg-purple-600 h-2 rounded-full transition-all"
                  style={{ width: `${systemMetrics?.memory_usage ?? systemMetrics?.memory_usage_percent ?? 0}%` }}
                />
              </div>
            </div>

            {/* Disk Usage */}
            <div>
              <div className="flex items-center justify-between mb-2">
                <div className="flex items-center gap-2">
                  <HardDrive className="w-4 h-4 text-muted-foreground" />
                  <span className="text-sm font-medium">Disk</span>
                </div>
                <span className="text-sm font-bold">
                  {(systemMetrics?.disk_usage ?? systemMetrics?.disk_usage_percent ?? 0).toFixed(1)}%
                </span>
              </div>
              <div className="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
                <div
                  className="bg-orange-600 h-2 rounded-full transition-all"
                  style={{ width: `${systemMetrics?.disk_usage ?? systemMetrics?.disk_usage_percent ?? 0}%` }}
                />
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      <ContentGrid>
        {/* Tenant Management */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Users className="w-5 h-5" />
              Tenant Management
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-3">
              {tenants.slice(0, 5).map(tenant => (
                <div
                  key={tenant.id}
                  className="flex items-center justify-between p-3 border rounded-lg"
                >
                  <div>
                    <p className="font-medium">{tenant.name}</p>
                    <p className="text-xs text-muted-foreground">{tenant.id}</p>
                  </div>
                  <Badge variant={tenant.status === 'active' ? 'default' : 'secondary'}>
                    {tenant.status || 'active'}
                  </Badge>
                </div>
              ))}
              {tenants.length === 0 && (
                <p className="text-sm text-muted-foreground text-center py-4">
                  No tenants found
                </p>
              )}
              <Button variant="outline" className="w-full">
                View All Tenants
              </Button>
            </div>
          </CardContent>
        </Card>

        {/* Recent Alerts */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <AlertTriangle className="w-5 h-5" />
              Recent Alerts
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-3">
              {alerts.slice(0, 5).map(alert => (
                <div
                  key={alert.id}
                  className="flex items-start gap-3 p-3 border rounded-lg"
                >
                  {alert.severity === 'critical' ? (
                    <XCircle className="w-5 h-5 text-red-500 flex-shrink-0 mt-0.5" />
                  ) : alert.severity === 'warning' ? (
                    <AlertTriangle className="w-5 h-5 text-yellow-500 flex-shrink-0 mt-0.5" />
                  ) : (
                    <CheckCircle className="w-5 h-5 text-blue-500 flex-shrink-0 mt-0.5" />
                  )}
                  <div className="flex-1 min-w-0">
                    <p className="text-sm font-medium truncate">{alert.title}</p>
                    <p className="text-xs text-muted-foreground">{alert.message}</p>
                  </div>
                  <Badge variant={alert.status === 'active' ? 'destructive' : 'outline'}>
                    {alert.status}
                  </Badge>
                </div>
              ))}
              {alerts.length === 0 && (
                <p className="text-sm text-muted-foreground text-center py-4">
                  No active alerts
                </p>
              )}
              <Button variant="outline" className="w-full">
                View All Alerts
              </Button>
            </div>
          </CardContent>
        </Card>
      </ContentGrid>

      {/* Adapter Statistics */}
      <Card>
        <CardHeader>
          <CardTitle>Adapter Registry</CardTitle>
        </CardHeader>
        <CardContent>
          <KpiGrid>
            <div className="text-center p-4 border rounded-lg">
              <div className="text-2xl font-bold">{adapters.length}</div>
              <div className="text-xs text-muted-foreground mt-1">Total Adapters</div>
            </div>
            <div className="text-center p-4 border rounded-lg">
              <div className="text-2xl font-bold">
                {adapters.filter(a => a.active).length}
              </div>
              <div className="text-xs text-muted-foreground mt-1">Active</div>
            </div>
            <div className="text-center p-4 border rounded-lg">
              <div className="text-2xl font-bold">
                {adapters.filter(a => a.current_state === 'hot').length}
              </div>
              <div className="text-xs text-muted-foreground mt-1">Hot State</div>
            </div>
            <div className="text-center p-4 border rounded-lg">
              <div className="text-2xl font-bold">
                {(adapters.reduce((sum, a) => sum + (a.memory_bytes ?? 0), 0) / (1024 * 1024 * 1024)).toFixed(2)}GB
              </div>
              <div className="text-xs text-muted-foreground mt-1">Memory Used</div>
            </div>
          </KpiGrid>
        </CardContent>
      </Card>
    </div>
  );
}
