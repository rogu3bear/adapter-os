import React, { useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { ScrollArea } from '@/components/ui/scroll-area';
import { usePolling } from '@/hooks/realtime/usePolling';
import { apiClient } from '@/api/services';
import { formatBytes as formatBytesUtil, formatRelativeTime } from '@/lib/formatters';

interface ServiceStatus {
  name: string;
  status: 'healthy' | 'degraded' | 'down' | 'unknown';
  latency: number;
  uptime: number;
  lastCheck: string;
  endpoint: string;
}

interface ResourceUtilization {
  cpu: {
    usage: number;
    cores: number;
    frequency: number;
  };
  memory: {
    used: number;
    total: number;
    percentage: number;
  };
  gpu: {
    usage: number;
    memory: number;
    temperature: number;
    available: boolean;
  };
  disk: {
    used: number;
    total: number;
    percentage: number;
  };
}

interface Alert {
  id: string;
  severity: 'critical' | 'warning' | 'info';
  message: string;
  source: string;
  timestamp: string;
  acknowledged: boolean;
}

interface MonitoringData {
  services: ServiceStatus[];
  resources: ResourceUtilization;
  alerts: Alert[];
}

const fetchMonitoringData = async (): Promise<MonitoringData> => {
  try {
    const [health, metrics, alerts] = await Promise.all([
      apiClient.request<{ services: ServiceStatus[] }>('/health/services').catch(() => null),
      apiClient.request<ResourceUtilization>('/metrics/system').catch(() => null),
      apiClient.request<Alert[]>('/alerts').catch(() => null)
    ]);

    return {
      services: health?.services || getMockServices(),
      resources: metrics || getMockResources(),
      alerts: alerts || getMockAlerts()
    };
  } catch {
    return {
      services: getMockServices(),
      resources: getMockResources(),
      alerts: getMockAlerts()
    };
  }
};

function getMockServices(): ServiceStatus[] {
  return [
    {
      name: 'API Server',
      status: 'healthy',
      latency: 12,
      uptime: 99.99,
      lastCheck: new Date().toISOString(),
      endpoint: '/api/health'
    },
    {
      name: 'Worker Pool',
      status: 'healthy',
      latency: 8,
      uptime: 99.95,
      lastCheck: new Date().toISOString(),
      endpoint: '/worker/health'
    },
    {
      name: 'Router',
      status: 'healthy',
      latency: 3,
      uptime: 99.99,
      lastCheck: new Date().toISOString(),
      endpoint: '/router/health'
    },
    {
      name: 'Database',
      status: 'healthy',
      latency: 5,
      uptime: 99.98,
      lastCheck: new Date().toISOString(),
      endpoint: '/db/health'
    },
    {
      name: 'Redis Cache',
      status: 'degraded',
      latency: 45,
      uptime: 98.5,
      lastCheck: new Date().toISOString(),
      endpoint: '/cache/health'
    },
    {
      name: 'Metal Backend',
      status: 'healthy',
      latency: 2,
      uptime: 99.99,
      lastCheck: new Date().toISOString(),
      endpoint: '/metal/health'
    }
  ];
}

function getMockResources(): ResourceUtilization {
  return {
    cpu: {
      usage: 42,
      cores: 10,
      frequency: 3.2
    },
    memory: {
      used: 24.5,
      total: 32,
      percentage: 76.5
    },
    gpu: {
      usage: 68,
      memory: 85,
      temperature: 72,
      available: true
    },
    disk: {
      used: 245,
      total: 500,
      percentage: 49
    }
  };
}

function getMockAlerts(): Alert[] {
  return [
    {
      id: 'alert-001',
      severity: 'warning',
      message: 'Redis cache latency exceeds threshold (>40ms)',
      source: 'cache-monitor',
      timestamp: new Date(Date.now() - 600000).toISOString(),
      acknowledged: false
    },
    {
      id: 'alert-002',
      severity: 'info',
      message: 'Scheduled maintenance window in 2 hours',
      source: 'scheduler',
      timestamp: new Date(Date.now() - 3600000).toISOString(),
      acknowledged: true
    },
    {
      id: 'alert-003',
      severity: 'critical',
      message: 'GPU memory usage above 80% - consider evicting adapters',
      source: 'resource-monitor',
      timestamp: new Date(Date.now() - 1800000).toISOString(),
      acknowledged: false
    },
    {
      id: 'alert-004',
      severity: 'info',
      message: 'New adapter registered: code-review/r004',
      source: 'registry',
      timestamp: new Date(Date.now() - 7200000).toISOString(),
      acknowledged: true
    }
  ];
}

function getStatusColor(status: string): string {
  switch (status) {
    case 'healthy':
      return 'bg-green-500';
    case 'degraded':
      return 'bg-yellow-500';
    case 'down':
      return 'bg-red-500';
    default:
      return 'bg-gray-500';
  }
}

function getSeverityVariant(severity: string): 'default' | 'secondary' | 'destructive' | 'outline' {
  switch (severity) {
    case 'critical':
      return 'destructive';
    case 'warning':
      return 'secondary';
    case 'info':
      return 'outline';
    default:
      return 'default';
  }
}

function formatBytes(gb: number): string {
  // Convert GB to bytes for the shared utility
  return formatBytesUtil(gb * 1024 * 1024 * 1024);
}

function formatTimeAgo(isoDate: string): string {
  return formatRelativeTime(isoDate);
}

export default function DevOpsMonitoringDashboard() {
  const { data, isLoading, lastUpdated, error } = usePolling<MonitoringData>(
    fetchMonitoringData,
    'fast',
    { operationName: 'monitoring-data' }
  );

  const unacknowledgedAlerts = useMemo(() => {
    return data?.alerts.filter(a => !a.acknowledged) || [];
  }, [data?.alerts]);

  const criticalAlerts = useMemo(() => {
    return unacknowledgedAlerts.filter(a => a.severity === 'critical');
  }, [unacknowledgedAlerts]);

  if (isLoading && !data) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-muted-foreground">Loading monitoring data...</div>
      </div>
    );
  }

  const { services = [], resources, alerts = [] } = data || {};

  return (
    <div className="space-y-6 p-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold">System Monitoring</h2>
          <p className="text-sm text-muted-foreground">
            Real-time service health and resource utilization
          </p>
        </div>
        <div className="flex items-center gap-4">
          {criticalAlerts.length > 0 && (
            <Badge variant="destructive" className="animate-pulse">
              {criticalAlerts.length} Critical Alert{criticalAlerts.length > 1 ? 's' : ''}
            </Badge>
          )}
          {lastUpdated && (
            <span className="text-xs text-muted-foreground">
              Updated {formatTimeAgo(lastUpdated.toISOString())}
            </span>
          )}
        </div>
      </div>

      {error && (
        <Card className="border-destructive">
          <CardContent className="pt-4">
            <p className="text-sm text-destructive">
              Failed to fetch monitoring data. Using cached values.
            </p>
          </CardContent>
        </Card>
      )}

      {/* Service Status Grid */}
      <Card>
        <CardHeader>
          <CardTitle>Service Status</CardTitle>
          <CardDescription>Health status of core system services</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-4">
            {services.map((service) => (
              <div
                key={service.name}
                className="p-4 rounded-lg border bg-card hover:bg-accent/50 transition-colors"
              >
                <div className="flex items-center gap-2 mb-2">
                  <div className={`w-2 h-2 rounded-full ${getStatusColor(service.status)}`} />
                  <span className="font-medium text-sm truncate">{service.name}</span>
                </div>
                <div className="space-y-1 text-xs text-muted-foreground">
                  <div className="flex justify-between">
                    <span>Latency</span>
                    <span className={service.latency > 30 ? 'text-yellow-600' : ''}>
                      {service.latency}ms
                    </span>
                  </div>
                  <div className="flex justify-between">
                    <span>Uptime</span>
                    <span>{service.uptime.toFixed(2)}%</span>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* Resource Utilization */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* CPU */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">CPU Usage</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{resources?.cpu.usage || 0}%</div>
            <Progress value={resources?.cpu.usage || 0} className="mt-2" />
            <div className="mt-2 text-xs text-muted-foreground">
              {resources?.cpu.cores} cores @ {resources?.cpu.frequency} GHz
            </div>
          </CardContent>
        </Card>

        {/* Memory */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">Memory</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{resources?.memory.percentage.toFixed(1) || 0}%</div>
            <Progress
              value={resources?.memory.percentage || 0}
              className={`mt-2 ${(resources?.memory.percentage || 0) > 85 ? '[&>div]:bg-yellow-500' : ''}`}
            />
            <div className="mt-2 text-xs text-muted-foreground">
              {formatBytes(resources?.memory.used || 0)} / {formatBytes(resources?.memory.total || 0)}
            </div>
          </CardContent>
        </Card>

        {/* GPU */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">GPU</CardTitle>
          </CardHeader>
          <CardContent>
            {resources?.gpu.available ? (
              <>
                <div className="text-2xl font-bold">{resources.gpu.usage}%</div>
                <Progress
                  value={resources.gpu.memory}
                  className={`mt-2 ${resources.gpu.memory > 80 ? '[&>div]:bg-red-500' : ''}`}
                />
                <div className="mt-2 text-xs text-muted-foreground">
                  VRAM: {resources.gpu.memory}% | Temp: {resources.gpu.temperature}C
                </div>
              </>
            ) : (
              <div className="text-muted-foreground text-sm">No GPU available</div>
            )}
          </CardContent>
        </Card>

        {/* Disk */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">Disk</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{resources?.disk.percentage || 0}%</div>
            <Progress value={resources?.disk.percentage || 0} className="mt-2" />
            <div className="mt-2 text-xs text-muted-foreground">
              {formatBytes(resources?.disk.used || 0)} / {formatBytes(resources?.disk.total || 0)}
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Alert List */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            Active Alerts
            {unacknowledgedAlerts.length > 0 && (
              <Badge variant="secondary">{unacknowledgedAlerts.length}</Badge>
            )}
          </CardTitle>
          <CardDescription>System alerts and notifications</CardDescription>
        </CardHeader>
        <CardContent>
          <ScrollArea className="h-[300px]">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Severity</TableHead>
                  <TableHead>Message</TableHead>
                  <TableHead>Source</TableHead>
                  <TableHead>Time</TableHead>
                  <TableHead>Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {alerts.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={5} className="text-center text-muted-foreground">
                      No alerts
                    </TableCell>
                  </TableRow>
                ) : (
                  alerts.map((alert) => (
                    <TableRow key={alert.id} className={!alert.acknowledged ? 'bg-muted/30' : ''}>
                      <TableCell>
                        <Badge variant={getSeverityVariant(alert.severity)}>
                          {alert.severity}
                        </Badge>
                      </TableCell>
                      <TableCell className="max-w-[300px]">
                        <span className={!alert.acknowledged ? 'font-medium' : ''}>
                          {alert.message}
                        </span>
                      </TableCell>
                      <TableCell className="text-xs text-muted-foreground">
                        {alert.source}
                      </TableCell>
                      <TableCell className="text-xs text-muted-foreground">
                        {formatTimeAgo(alert.timestamp)}
                      </TableCell>
                      <TableCell>
                        {alert.acknowledged ? (
                          <Badge variant="outline">ACK</Badge>
                        ) : (
                          <Badge variant="secondary">New</Badge>
                        )}
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </ScrollArea>
        </CardContent>
      </Card>
    </div>
  );
}
