import React, { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Skeleton } from '@/components/ui/skeleton';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { ActionGrid } from '@/components/ui/action-grid';
import { HelpTooltip } from '@/components/ui/help-tooltip';
import { errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { KpiGrid, ContentGrid } from '@/components/ui/grid';
import {
  Server,
  Activity,
  Cpu,
  HardDrive,
  Network,
  AlertTriangle,
  CheckCircle,
  XCircle,
  Clock,
  Zap,
  FileText,
  Search,
  UserPlus,
  PlayCircle,
} from 'lucide-react';
import { useNodes, useWorkers, useSystemMetrics, useSystemHealthStatus, getHealthStatus } from '@/hooks/useSystem';
import type { Node, WorkerResponse } from '@/api/types';
import { usePolling } from '@/hooks/usePolling';
import apiClient from '@/api/client';
import { toast } from 'sonner';

interface SREDashboardProps {
  selectedTenant?: string;
}

interface AlertItem {
  id: string;
  severity: 'critical' | 'high' | 'medium' | 'low';
  message: string;
  timestamp: string;
  source: string;
}

export default function SREDashboard({ selectedTenant = 'default' }: SREDashboardProps) {
  const navigate = useNavigate();

  // Fetch system data
  const { metrics, isLoading: metricsLoading, error: metricsError, refetch: refetchMetrics } = useSystemMetrics('fast', true);
  const { nodes, isLoading: nodesLoading, error: nodesError, refetch: refetchNodes } = useNodes('normal', true);
  const { workers, isLoading: workersLoading, error: workersError, refetch: refetchWorkers } = useWorkers(selectedTenant, undefined, 'normal', true);

  // Fetch recent alerts
  const {
    data: alerts,
    isLoading: alertsLoading,
    error: alertsError
  } = usePolling<AlertItem[]>(
    async () => {
      try {
        const response = await apiClient.listAlerts({ limit: 10, sort: 'timestamp:desc' });
        // Return response directly if it's an array, otherwise extract alerts property
        return Array.isArray(response) ? response : ((response as any).alerts || []);
      } catch (err) {
        // If alerts endpoint doesn't exist, return empty array
        return [];
      }
    },
    'slow',
    {
      enabled: true,
      operationName: 'listAlerts',
    }
  );

  // Compute health status
  const healthStatus = useSystemHealthStatus(metrics);

  // Calculate derived metrics
  const nodeStats = useMemo(() => {
    if (!nodes || nodes.length === 0) {
      return { healthy: 0, offline: 0, error: 0, total: 0, healthPercentage: 0 };
    }

    const healthy = nodes.filter(n => n.status === 'healthy').length;
    const offline = nodes.filter(n => n.status === 'offline').length;
    const error = nodes.filter(n => n.status === 'error').length;
    const total = nodes.length;
    const healthPercentage = total > 0 ? (healthy / total) * 100 : 0;

    return { healthy, offline, error, total, healthPercentage };
  }, [nodes]);

  const workerStats = useMemo(() => {
    if (!workers || workers.length === 0) {
      return { running: 0, starting: 0, stopped: 0, error: 0, total: 0, utilization: 0 };
    }

    const running = workers.filter(w => w.status === 'running').length;
    const starting = workers.filter(w => w.status === 'starting').length;
    const stopped = workers.filter(w => w.status === 'stopped').length;
    const error = workers.filter(w => w.status === 'error').length;
    const total = workers.length;
    const utilization = total > 0 ? (running / total) * 100 : 0;

    return { running, starting, stopped, error, total, utilization };
  }, [workers]);

  const recentAlerts = useMemo(() => {
    return (alerts || []).slice(0, 5);
  }, [alerts]);

  // Quick actions for SRE
  const quickActions = useMemo(() => [
    {
      label: 'System Logs',
      icon: FileText,
      color: 'text-blue-600',
      helpId: 'sre-action-logs',
      onClick: () => navigate('/logs'),
    },
    {
      label: 'Node Diagnostics',
      icon: Search,
      color: 'text-purple-600',
      helpId: 'sre-action-diagnostics',
      onClick: () => navigate('/nodes'),
    },
    {
      label: 'Spawn Worker',
      icon: UserPlus,
      color: 'text-green-600',
      helpId: 'sre-action-spawn-worker',
      onClick: () => {
        toast.info('Worker spawn dialog opening...');
        // In production, this would open a modal or navigate to worker spawn page
      },
    },
    {
      label: 'Replay Session',
      icon: PlayCircle,
      color: 'text-amber-600',
      helpId: 'sre-action-replay',
      onClick: () => navigate('/replay'),
    },
  ], [navigate]);

  // Helper functions
  const getHealthStatusColor = (status: string) => {
    switch (status) {
      case 'healthy': return 'text-green-600';
      case 'warning': return 'text-yellow-600';
      case 'critical': return 'text-red-600';
      default: return 'text-gray-600';
    }
  };

  const getSeverityVariant = (severity: string): 'default' | 'destructive' | 'outline' | 'secondary' => {
    switch (severity) {
      case 'critical': return 'destructive';
      case 'high': return 'destructive';
      case 'medium': return 'outline';
      case 'low': return 'secondary';
      default: return 'default';
    }
  };

  const formatTimeAgo = (timestamp: string): string => {
    const now = new Date();
    const eventTime = new Date(timestamp);
    const diffMs = now.getTime() - eventTime.getTime();
    const diffMins = Math.floor(diffMs / (1000 * 60));
    const diffHours = Math.floor(diffMins / 60);

    if (diffMins < 1) return 'just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    return `${Math.floor(diffHours / 24)}d ago`;
  };

  // CPU/Memory/Disk values
  const cpuUsage = metrics?.cpu_usage_percent ?? metrics?.cpu_usage ?? 0;
  const memoryUsage = metrics?.memory_usage_percent ?? metrics?.memory_usage_pct ?? 0;
  const diskUsage = metrics?.disk_usage_percent ?? 0;
  const networkRx = metrics?.network_rx_bytes ? (metrics.network_rx_bytes / 1024 / 1024).toFixed(1) : '0';

  const cpuStatus = getHealthStatus(cpuUsage, 70, 90);
  const memStatus = getHealthStatus(memoryUsage, 75, 90);
  const diskStatus = getHealthStatus(diskUsage, 80, 95);

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">SRE Dashboard</h1>
          <p className="text-muted-foreground mt-1">
            Infrastructure monitoring and diagnostics for {selectedTenant}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Badge variant={healthStatus === 'healthy' ? 'default' : healthStatus === 'warning' ? 'outline' : 'destructive'}>
            {healthStatus === 'healthy' ? 'All Systems Operational' : healthStatus === 'warning' ? 'Warning' : 'Critical'}
          </Badge>
        </div>
      </div>

      {/* System Health KPIs */}
      <KpiGrid>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <HelpTooltip helpId="sre-nodes-health">
              <CardTitle className="text-sm font-medium cursor-help">Node Health</CardTitle>
            </HelpTooltip>
            <Server className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {nodesLoading ? (
              <Skeleton className="h-8 w-20" />
            ) : nodesError ? (
              <div className="text-sm text-destructive">Error</div>
            ) : (
              <>
                <div className="text-2xl font-bold">{nodeStats.healthy}/{nodeStats.total}</div>
                <Progress value={nodeStats.healthPercentage} className="mt-2 h-2" />
                <p className="text-xs text-muted-foreground mt-1">
                  {nodeStats.healthPercentage.toFixed(0)}% healthy
                </p>
              </>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <HelpTooltip helpId="sre-worker-utilization">
              <CardTitle className="text-sm font-medium cursor-help">Worker Pool</CardTitle>
            </HelpTooltip>
            <Activity className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {workersLoading ? (
              <Skeleton className="h-8 w-20" />
            ) : workersError ? (
              <div className="text-sm text-destructive">Error</div>
            ) : (
              <>
                <div className="text-2xl font-bold">{workerStats.running}/{workerStats.total}</div>
                <Progress value={workerStats.utilization} className="mt-2 h-2" />
                <p className="text-xs text-muted-foreground mt-1">
                  {workerStats.utilization.toFixed(0)}% active
                </p>
              </>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <HelpTooltip helpId="sre-cpu-usage">
              <CardTitle className="text-sm font-medium cursor-help">CPU Usage</CardTitle>
            </HelpTooltip>
            <Cpu className={`h-4 w-4 ${getHealthStatusColor(cpuStatus)}`} />
          </CardHeader>
          <CardContent>
            {metricsLoading ? (
              <Skeleton className="h-8 w-16" />
            ) : metricsError ? (
              <div className="text-sm text-destructive">Error</div>
            ) : (
              <>
                <div className={`text-2xl font-bold ${getHealthStatusColor(cpuStatus)}`}>
                  {cpuUsage.toFixed(1)}%
                </div>
                <Progress value={cpuUsage} className="mt-2 h-2" />
                <p className="text-xs text-muted-foreground mt-1">
                  {cpuStatus === 'healthy' ? 'Normal' : cpuStatus === 'warning' ? 'Elevated' : 'Critical'}
                </p>
              </>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <HelpTooltip helpId="sre-memory-usage">
              <CardTitle className="text-sm font-medium cursor-help">Memory Usage</CardTitle>
            </HelpTooltip>
            <HardDrive className={`h-4 w-4 ${getHealthStatusColor(memStatus)}`} />
          </CardHeader>
          <CardContent>
            {metricsLoading ? (
              <Skeleton className="h-8 w-16" />
            ) : metricsError ? (
              <div className="text-sm text-destructive">Error</div>
            ) : (
              <>
                <div className={`text-2xl font-bold ${getHealthStatusColor(memStatus)}`}>
                  {memoryUsage.toFixed(1)}%
                </div>
                <Progress value={memoryUsage} className="mt-2 h-2" />
                <p className="text-xs text-muted-foreground mt-1">
                  {memStatus === 'healthy' ? 'Normal' : memStatus === 'warning' ? 'Elevated' : 'Critical'}
                </p>
              </>
            )}
          </CardContent>
        </Card>
      </KpiGrid>

      {/* Main Content Grid */}
      <ContentGrid>
        {/* Node Health Grid */}
        <Card>
          <CardHeader>
            <CardTitle>Node Status</CardTitle>
          </CardHeader>
          <CardContent>
            {nodesLoading ? (
              <div className="space-y-2">
                <Skeleton className="h-16 w-full" />
                <Skeleton className="h-16 w-full" />
                <Skeleton className="h-16 w-full" />
              </div>
            ) : nodesError ? (
              errorRecoveryTemplates.genericError(nodesError, refetchNodes)
            ) : nodes.length === 0 ? (
              <div className="text-center py-8 text-muted-foreground">
                <Server className="h-12 w-12 mx-auto mb-2 opacity-50" />
                <p>No nodes registered</p>
              </div>
            ) : (
              <div className="space-y-2">
                {nodes.slice(0, 5).map((node: Node) => {
                  const StatusIcon = node.status === 'healthy' ? CheckCircle : node.status === 'offline' ? XCircle : AlertTriangle;
                  const statusColor = node.status === 'healthy' ? 'text-green-600' : node.status === 'offline' ? 'text-gray-400' : 'text-red-600';

                  return (
                    <div key={node.id} className="flex items-center justify-between p-3 border rounded-lg">
                      <div className="flex items-center gap-3">
                        <StatusIcon className={`h-5 w-5 ${statusColor}`} />
                        <div>
                          <p className="font-medium">{node.hostname}</p>
                          <p className="text-xs text-muted-foreground">
                            {node.memory_gb ? `${node.memory_gb}GB RAM` : 'Memory unknown'} • {node.gpu_count ? `${node.gpu_count} GPU${node.gpu_count > 1 ? 's' : ''}` : 'No GPU'}
                          </p>
                        </div>
                      </div>
                      <Badge variant={node.status === 'healthy' ? 'default' : node.status === 'offline' ? 'secondary' : 'destructive'}>
                        {node.status}
                      </Badge>
                    </div>
                  );
                })}
                {nodes.length > 5 && (
                  <Button variant="outline" className="w-full mt-2" onClick={() => navigate('/nodes')}>
                    View all {nodes.length} nodes
                  </Button>
                )}
              </div>
            )}
          </CardContent>
        </Card>

        {/* Worker Pool Status */}
        <Card>
          <CardHeader>
            <CardTitle>Worker Pool</CardTitle>
          </CardHeader>
          <CardContent>
            {workersLoading ? (
              <div className="space-y-2">
                <Skeleton className="h-12 w-full" />
                <Skeleton className="h-12 w-full" />
              </div>
            ) : workersError ? (
              errorRecoveryTemplates.genericError(workersError, refetchWorkers)
            ) : (
              <div className="space-y-4">
                <div className="grid grid-cols-2 gap-4">
                  <div className="p-3 border rounded-lg">
                    <div className="flex items-center justify-between">
                      <span className="text-sm text-muted-foreground">Running</span>
                      <Badge variant="default">{workerStats.running}</Badge>
                    </div>
                  </div>
                  <div className="p-3 border rounded-lg">
                    <div className="flex items-center justify-between">
                      <span className="text-sm text-muted-foreground">Starting</span>
                      <Badge variant="outline">{workerStats.starting}</Badge>
                    </div>
                  </div>
                  <div className="p-3 border rounded-lg">
                    <div className="flex items-center justify-between">
                      <span className="text-sm text-muted-foreground">Stopped</span>
                      <Badge variant="secondary">{workerStats.stopped}</Badge>
                    </div>
                  </div>
                  <div className="p-3 border rounded-lg">
                    <div className="flex items-center justify-between">
                      <span className="text-sm text-muted-foreground">Error</span>
                      <Badge variant="destructive">{workerStats.error}</Badge>
                    </div>
                  </div>
                </div>
                <div className="pt-2">
                  <div className="flex items-center justify-between text-sm mb-2">
                    <span className="text-muted-foreground">Utilization</span>
                    <span className="font-medium">{workerStats.utilization.toFixed(0)}%</span>
                  </div>
                  <Progress value={workerStats.utilization} className="h-2" />
                </div>
              </div>
            )}
          </CardContent>
        </Card>

        {/* Performance Charts */}
        <Card>
          <CardHeader>
            <CardTitle>Performance Metrics</CardTitle>
          </CardHeader>
          <CardContent>
            {metricsLoading ? (
              <div className="space-y-4">
                <Skeleton className="h-16 w-full" />
                <Skeleton className="h-16 w-full" />
                <Skeleton className="h-16 w-full" />
              </div>
            ) : metricsError ? (
              errorRecoveryTemplates.genericError(metricsError, refetchMetrics)
            ) : (
              <div className="space-y-4">
                <div className="space-y-2">
                  <div className="flex items-center justify-between text-sm">
                    <div className="flex items-center gap-2">
                      <Zap className="h-4 w-4 text-muted-foreground" />
                      <span>Throughput</span>
                    </div>
                    <span className="font-medium">{metrics?.tokens_per_second?.toFixed(0) ?? 0} tokens/sec</span>
                  </div>
                  <div className="flex items-center justify-between text-sm">
                    <div className="flex items-center gap-2">
                      <Clock className="h-4 w-4 text-muted-foreground" />
                      <span>Latency (p95)</span>
                    </div>
                    <span className="font-medium">{metrics?.latency_p95_ms?.toFixed(0) ?? 0} ms</span>
                  </div>
                  <div className="flex items-center justify-between text-sm">
                    <div className="flex items-center gap-2">
                      <Network className="h-4 w-4 text-muted-foreground" />
                      <span>Network RX</span>
                    </div>
                    <span className="font-medium">{networkRx} MB/s</span>
                  </div>
                  <div className="flex items-center justify-between text-sm">
                    <div className="flex items-center gap-2">
                      <HardDrive className="h-4 w-4 text-muted-foreground" />
                      <span>Disk Usage</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <span className="font-medium">{diskUsage.toFixed(1)}%</span>
                      <Progress value={diskUsage} className="h-2 w-24" />
                    </div>
                  </div>
                </div>
              </div>
            )}
          </CardContent>
        </Card>

        {/* Alert Timeline */}
        <Card>
          <CardHeader>
            <CardTitle>Recent Alerts</CardTitle>
          </CardHeader>
          <CardContent>
            {alertsLoading ? (
              <div className="space-y-2">
                <Skeleton className="h-12 w-full" />
                <Skeleton className="h-12 w-full" />
                <Skeleton className="h-12 w-full" />
              </div>
            ) : alertsError ? (
              <Alert variant="default">
                <AlertDescription>
                  Unable to load alerts. Check system logs for details.
                </AlertDescription>
              </Alert>
            ) : recentAlerts.length === 0 ? (
              <div className="text-center py-8 text-muted-foreground">
                <CheckCircle className="h-12 w-12 mx-auto mb-2 opacity-50 text-green-600" />
                <p>No recent alerts</p>
                <p className="text-xs mt-1">System is operating normally</p>
              </div>
            ) : (
              <div className="space-y-2">
                {recentAlerts.map((alert) => (
                  <div key={alert.id} className="flex items-start gap-3 p-3 border rounded-lg">
                    <AlertTriangle className={`h-5 w-5 mt-0.5 flex-shrink-0 ${
                      alert.severity === 'critical' ? 'text-red-600' :
                      alert.severity === 'high' ? 'text-orange-600' :
                      alert.severity === 'medium' ? 'text-yellow-600' :
                      'text-blue-600'
                    }`} />
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 mb-1">
                        <Badge variant={getSeverityVariant(alert.severity)} className="text-xs">
                          {alert.severity}
                        </Badge>
                        <span className="text-xs text-muted-foreground">{formatTimeAgo(alert.timestamp)}</span>
                      </div>
                      <p className="text-sm font-medium">{alert.message}</p>
                      <p className="text-xs text-muted-foreground mt-1">Source: {alert.source}</p>
                    </div>
                  </div>
                ))}
                <Button variant="outline" className="w-full mt-2" onClick={() => navigate('/alerts')}>
                  View all alerts
                </Button>
              </div>
            )}
          </CardContent>
        </Card>
      </ContentGrid>

      {/* Quick Actions */}
      <Card>
        <CardHeader>
          <CardTitle>Quick Actions</CardTitle>
        </CardHeader>
        <CardContent>
          <ActionGrid actions={quickActions} columns={4} />
        </CardContent>
      </Card>
    </div>
  );
}
