import React, { useMemo, useState, useCallback } from 'react';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from './ui/card';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { MetricsChart, MetricData } from './MetricsChart';
import {
  AlertTriangle,
  Activity,
  Cpu,
  Database,
  GaugeCircle,
  MemoryStick,
  ShieldCheck,
  Zap,
} from 'lucide-react';
import apiClient from '../api/client';
import * as types from '../api/types';
import { logger, toError } from '../utils/logger';
import { usePolling } from '../hooks/usePolling';

interface AlertEntry {
  id: string;
  metric: string;
  message: string;
  severity: 'info' | 'warning' | 'critical';
  value: number;
  threshold: number;
  timestamp: string;
}

interface AdapterOverview {
  adapterId: string;
  activationRate: number;
  avgLatencyMs: number;
  qualityScore?: number;
  totalRequests?: number;
}

const formatPercent = (value: number) => `${value.toFixed(1)}%`;
const formatLatency = (value: number) => `${value.toFixed(1)} ms`;

const pickMetric = (source: unknown, keys: string[]): number => {
  if (!source || typeof source !== 'object') return 0;
  const record = source as Record<string, unknown>;
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'number') {
      return value;
    }
  }
  return 0;
};

export const MonitoringDashboard: React.FC = () => {
  const [systemMetrics, setSystemMetrics] = useState<types.SystemMetrics | null>(null);
  const [adapterMetrics, setAdapterMetrics] = useState<AdapterOverview[]>([]);
  const [qualityMetrics, setQualityMetrics] = useState<types.QualityMetrics | null>(null);
  const [alerts, setAlerts] = useState<AlertEntry[]>([]);
  const [metricHistory, setMetricHistory] = useState<MetricData[]>([]);
  const [latencyHistory, setLatencyHistory] = useState<MetricData[]>([]);

  const cpuUsage = pickMetric(systemMetrics, ['cpu_usage', 'cpu_usage_percent', 'memory_usage_pct']);
  const memoryUsage = pickMetric(systemMetrics, ['memory_usage', 'memory_usage_percent', 'memory_usage_pct']);
  const gpuUtilization = pickMetric(systemMetrics, ['gpu_utilization', 'gpu_utilization_percent']);
  const uptimeSeconds = pickMetric(systemMetrics, ['uptime_seconds']);

  const evaluateAlerts = useCallback((
    system: types.SystemMetrics,
    adapters: types.AdapterMetrics[],
  ) => {
    const newAlerts: AlertEntry[] = [];
    const timestamp = new Date().toISOString();

    if (system.cpu_usage_percent && system.cpu_usage_percent > 85) {
      newAlerts.push({
        id: `${timestamp}-cpu`,
        metric: 'cpu_usage',
        message: 'CPU usage exceeded 85% threshold',
        severity: system.cpu_usage_percent > 92 ? 'critical' : 'warning',
        value: system.cpu_usage_percent,
        threshold: 85,
        timestamp,
      });
    }

    if (system.memory_usage_pct && system.memory_usage_pct > 80) {
      newAlerts.push({
        id: `${timestamp}-memory`,
        metric: 'memory_usage',
        message: 'Memory usage is elevated',
        severity: system.memory_usage_pct > 90 ? 'critical' : 'warning',
        value: system.memory_usage_pct,
        threshold: 80,
        timestamp,
      });
    }

    const latencyValue = pickMetric(system, ['avg_latency_ms', 'latency_p95_ms']);
    if (latencyValue > 400) {
      newAlerts.push({
        id: `${timestamp}-latency`,
        metric: 'latency_p95',
        message: 'P95 latency degraded',
        severity: latencyValue > 600 ? 'critical' : 'warning',
        value: latencyValue,
        threshold: 400,
        timestamp,
      });
    }

    adapters.forEach((adapter) => {
      const activationRate = pickMetric(adapter.performance, ['activation_rate']);
      if (activationRate > 95) {
        newAlerts.push({
          id: `${timestamp}-${adapter.adapter_id}-activation`,
          metric: 'adapter_activation',
          message: `Adapter ${adapter.adapter_id} is selected for nearly all requests`,
          severity: 'warning',
          value: activationRate,
          threshold: 95,
          timestamp,
        });
      }
    });

    setAlerts((prev) => [...newAlerts, ...prev].slice(0, 25));
  }, []);

  const fetchMonitoringData = useCallback(async () => {
    const [system, quality, adapters] = await Promise.all([
      apiClient.getSystemMetrics(),
      apiClient.getQualityMetrics(),
      apiClient.getAdapterMetrics(),
    ]);

    return { system, quality, adapters };
  }, []);

  usePolling(
    fetchMonitoringData,
    'normal',
    {
      operationName: 'MonitoringDashboard.fetchMonitoringData',
      showLoadingIndicator: false,
      onSuccess: (data) => {
        const { system, quality, adapters } = data as {
          system: types.SystemMetrics;
          quality: types.QualityMetrics;
          adapters: types.AdapterMetrics[];
        };

        setSystemMetrics(system);
        setQualityMetrics(quality);
        setAdapterMetrics(
          (adapters || []).map((entry) => {
            const perf = entry.performance;
            const activationRate = (() => {
              if (!perf) return 0;
              if (typeof perf.activation_rate === 'number') {
                return perf.activation_rate;
              }
              if (
                typeof perf.activation_count === 'number' &&
                typeof perf.total_requests === 'number' &&
                perf.total_requests > 0
              ) {
                return (perf.activation_count / perf.total_requests) * 100;
              }
              return perf.activation_count ?? 0;
            })();

            const avgLatencyMs = perf?.avg_latency_ms
              ?? (typeof perf?.avg_latency_us === 'number'
                ? perf!.avg_latency_us / 1000
                : 0);

            return {
              adapterId: entry.adapter_id,
              activationRate,
              avgLatencyMs,
              qualityScore: perf?.quality_score,
              totalRequests: perf?.total_requests,
            };
          })
        );

        const timestamp = new Date().toISOString();
        const cpuValue = pickMetric(system, ['cpu_usage', 'cpu_usage_percent', 'memory_usage_pct']);
        const latencyValue = pickMetric(system, ['avg_latency_ms', 'latency_p95_ms']);

        setMetricHistory((prev) =>
          [
            ...prev.slice(-59),
            { time: timestamp, value: cpuValue },
          ]
        );
        setLatencyHistory((prev) =>
          [
            ...prev.slice(-59),
            { time: timestamp, value: latencyValue },
          ]
        );

        evaluateAlerts(system, adapters);
      },
      onError: (error) => {
        logger.error('Failed to fetch monitoring data', {
          component: 'MonitoringDashboard',
          operation: 'fetchMonitoringData',
        }, error);
      }
    }
  );

  const cpuHistory = useMemo(
    () => metricHistory.map((entry) => ({ ...entry, label: 'CPU %' })),
    [metricHistory]
  );
  const latencyHistoryMs = useMemo(
    () => latencyHistory.map((entry) => ({ ...entry, label: 'Latency (ms)' })),
    [latencyHistory]
  );

  const criticalAlerts = alerts.filter((alert) => alert.severity === 'critical');

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-semibold tracking-tight">Monitoring Dashboard</h1>
          <p className="text-muted-foreground">
            Real-time visibility into router performance, adapter health, and system alerts.
          </p>
        </div>
        <Badge variant={criticalAlerts.length ? 'destructive' : 'secondary'}>
          {criticalAlerts.length ? `${criticalAlerts.length} critical alerts` : 'All systems nominal'}
        </Badge>
      </div>

      <Tabs defaultValue="overview" className="space-y-6">
        <TabsList>
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="adapters">Adapters</TabsTrigger>
          <TabsTrigger value="alerts">Alerts</TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="space-y-6">
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <SummaryCard
              title="CPU Utilization"
              icon={<Cpu className="h-5 w-5 text-blue-500" />}
              value={systemMetrics ? formatPercent(cpuUsage) : '--'}
              description="Target &lt; 75%"
            />
            <SummaryCard
              title="Memory Usage"
              icon={<MemoryStick className="h-5 w-5 text-purple-500" />}
              value={systemMetrics ? formatPercent(memoryUsage) : '--'}
              description="Includes GPU shared pools"
            />
            <SummaryCard
              title="GPU Utilization"
              icon={<GaugeCircle className="h-5 w-5 text-emerald-500" />}
              value={systemMetrics ? formatPercent(gpuUtilization) : '--'}
              description="Metal kernels"
            />
            <SummaryCard
              title="Uptime"
              icon={<Activity className="h-5 w-5 text-orange-500" />}
              value={systemMetrics ? `${Math.floor(uptimeSeconds / 3600)}h` : '--'}
              description="Since last kernel reload"
            />
          </div>

          <div className="grid gap-6 lg:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle>CPU Trend</CardTitle>
                <CardDescription>Real-time CPU utilization from system metrics</CardDescription>
              </CardHeader>
              <CardContent>
                <MetricsChart data={cpuHistory} yAxisLabel="CPU %" color="#2563eb" height={260} />
              </CardContent>
            </Card>
            <Card>
              <CardHeader>
                <CardTitle>Latency Trend</CardTitle>
                <CardDescription>P95 latency from quality metrics</CardDescription>
              </CardHeader>
              <CardContent>
                <MetricsChart data={latencyHistoryMs} yAxisLabel="Latency (ms)" color="#f97316" height={260} />
              </CardContent>
            </Card>
          </div>

          {qualityMetrics && (
            <Card>
              <CardHeader>
                <CardTitle>Quality KPIs</CardTitle>
                <CardDescription>End-to-end router quality and outcome metrics</CardDescription>
              </CardHeader>
              <CardContent className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
                <KpiBadge
                  icon={<ShieldCheck className="h-4 w-4" />}
                  label="Answer Relevance"
                  value={qualityMetrics.arr?.toFixed(2) ?? '--'}
                />
                <KpiBadge
                  icon={<Zap className="h-4 w-4" />}
                  label="Early Correctness (ECS@5)"
                  value={qualityMetrics.ecs5?.toFixed(2) ?? '--'}
                />
                <KpiBadge
                  icon={<Database className="h-4 w-4" />}
                  label="Hallucination Rate"
                  value={`${qualityMetrics.hlr?.toFixed(2) ?? '--'}%`}
                />
                <KpiBadge
                  icon={<AlertTriangle className="h-4 w-4" />}
                  label="Completion Rate"
                  value={`${qualityMetrics.cr?.toFixed(2) ?? '--'}%`}
                  variant={qualityMetrics.cr !== undefined && qualityMetrics.cr < 95 ? 'destructive' : 'default'}
                />
              </CardContent>
            </Card>
          )}
        </TabsContent>

        <TabsContent value="adapters" className="space-y-6">
          <Card>
            <CardHeader>
              <CardTitle>Adapter Performance</CardTitle>
              <CardDescription>Routing activations, latency, and quality scores</CardDescription>
            </CardHeader>
            <CardContent>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Adapter</TableHead>
                    <TableHead>Activation Rate</TableHead>
                    <TableHead>Latency</TableHead>
                    <TableHead>Quality</TableHead>
                    <TableHead>Requests</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {adapterMetrics.map((adapter) => (
                    <TableRow key={adapter.adapterId}>
                      <TableCell className="font-medium">{adapter.adapterId}</TableCell>
                      <TableCell>{formatPercent(adapter.activationRate ?? 0)}</TableCell>
                      <TableCell>{formatLatency(adapter.avgLatencyMs ?? 0)}</TableCell>
                      <TableCell>{adapter.qualityScore?.toFixed(2) ?? '--'}</TableCell>
                      <TableCell>{adapter.totalRequests ?? '--'}</TableCell>
                    </TableRow>
                  ))}
                  {adapterMetrics.length === 0 && (
                    <TableRow>
                      <TableCell colSpan={5} className="text-center text-muted-foreground">
                        No adapter metrics available yet.
                      </TableCell>
                    </TableRow>
                  )}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="alerts" className="space-y-6">
          <Card>
            <CardHeader>
              <CardTitle>Alert Feed</CardTitle>
              <CardDescription>Escalations derived from telemetry streams</CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              {alerts.length === 0 && (
                <div className="flex flex-col items-center justify-center gap-2 py-12 text-muted-foreground">
                  <ShieldCheck className="h-10 w-10" />
                  <p>No alerts in the last 24 hours.</p>
                </div>
              )}
              {alerts.map((alert) => (
                <div
                  key={alert.id}
                  className="flex items-start justify-between rounded-lg border p-4"
                >
                  <div className="flex items-start gap-3">
                    <SeverityIcon severity={alert.severity} />
                    <div>
                      <p className="text-sm font-medium">{alert.message}</p>
                      <p className="text-xs text-muted-foreground">
                        Observed {alert.metric}={alert.value.toFixed(2)} (threshold {alert.threshold})
                      </p>
                    </div>
                  </div>
                  <span className="text-xs text-muted-foreground">
                    {new Date(alert.timestamp).toLocaleTimeString()}
                  </span>
                </div>
              ))}
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
};

interface SummaryCardProps {
  title: string;
  value: string;
  description: string;
  icon: React.ReactNode;
}

const SummaryCard: React.FC<SummaryCardProps> = ({ title, value, description, icon }) => (
  <Card>
    <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
      <CardTitle className="text-sm font-medium">{title}</CardTitle>
      {icon}
    </CardHeader>
    <CardContent>
      <div className="text-2xl font-bold">{value}</div>
      <p className="text-xs text-muted-foreground">{description}</p>
    </CardContent>
  </Card>
);

interface KpiBadgeProps {
  label: string;
  value: string;
  icon: React.ReactNode;
  variant?: 'default' | 'destructive';
}

const KpiBadge: React.FC<KpiBadgeProps> = ({ label, value, icon, variant = 'default' }) => (
  <div className="flex items-center gap-3 rounded-lg border p-3">
    <div className="rounded-full bg-muted p-2 text-muted-foreground">{icon}</div>
    <div>
      <div className="text-sm text-muted-foreground">{label}</div>
      <div className="text-lg font-semibold">
        {variant === 'destructive' ? (
          <span className="text-destructive">{value}</span>
        ) : (
          value
        )}
      </div>
    </div>
  </div>
);

const SeverityIcon: React.FC<{ severity: AlertEntry['severity'] }> = ({ severity }) => {
  switch (severity) {
    case 'critical':
      return <AlertTriangle className="h-5 w-5 text-red-500" />;
    case 'warning':
      return <AlertTriangle className="h-5 w-5 text-yellow-500" />;
    default:
      return <ShieldCheck className="h-5 w-5 text-emerald-500" />;
  }
};
