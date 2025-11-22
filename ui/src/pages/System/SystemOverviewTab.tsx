import { useMemo } from 'react';
import { Link } from 'react-router-dom';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Skeleton } from '@/components/ui/skeleton';
import { Button } from '@/components/ui/button';
import {
  useSystemMetrics,
  useNodes,
  useWorkers,
  useComputedMetrics,
  useSystemHealthStatus,
  getHealthStatus,
  type HealthStatus,
} from '@/hooks/useSystemMetrics';

function HealthBadge({ status }: { status: HealthStatus }) {
  const variant = {
    healthy: 'success' as const,
    warning: 'warning' as const,
    critical: 'destructive' as const,
    unknown: 'secondary' as const,
  }[status];

  return (
    <Badge variant={variant}>
      {status.charAt(0).toUpperCase() + status.slice(1)}
    </Badge>
  );
}

function MetricCard({
  title,
  value,
  unit,
  progress,
  status,
  isLoading,
}: {
  title: string;
  value: string | number;
  unit?: string;
  progress?: number;
  status?: HealthStatus;
  isLoading?: boolean;
}) {
  if (isLoading) {
    return (
      <Card>
        <CardHeader className="pb-2">
          <Skeleton className="h-4 w-24" />
        </CardHeader>
        <CardContent>
          <Skeleton className="h-8 w-16 mb-2" />
          <Skeleton className="h-2 w-full" />
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between">
          <CardDescription>{title}</CardDescription>
          {status && <HealthBadge status={status} />}
        </div>
      </CardHeader>
      <CardContent>
        <div className="text-2xl font-bold">
          {value}
          {unit && <span className="text-sm font-normal text-muted-foreground ml-1">{unit}</span>}
        </div>
        {progress !== undefined && (
          <Progress value={progress} className="mt-2" />
        )}
      </CardContent>
    </Card>
  );
}

export default function SystemOverviewTab() {
  const { metrics, isLoading: metricsLoading, error: metricsError, lastUpdated } = useSystemMetrics('normal');
  const { nodes, isLoading: nodesLoading } = useNodes('slow');
  const { workers, isLoading: workersLoading } = useWorkers(undefined, undefined, 'slow');

  const computed = useComputedMetrics(metrics);
  const healthStatus = useSystemHealthStatus(metrics);

  const nodeStats = useMemo(() => {
    const healthy = nodes.filter(n => n.status === 'healthy').length;
    const offline = nodes.filter(n => n.status === 'offline').length;
    const error = nodes.filter(n => n.status === 'error').length;
    return { healthy, offline, error, total: nodes.length };
  }, [nodes]);

  const workerStats = useMemo(() => {
    const running = workers.filter(w => w.status === 'running').length;
    const stopped = workers.filter(w => w.status === 'stopped').length;
    const errored = workers.filter(w => w.status === 'error').length;
    return { running, stopped, errored, total: workers.length };
  }, [workers]);

  return (
    <div className="space-y-6">
      {metricsError && (
        <Card className="border-destructive bg-destructive/10">
          <CardContent className="pt-6">
            <p className="text-destructive">Failed to load system metrics: {metricsError.message}</p>
          </CardContent>
        </Card>
      )}

      {lastUpdated && (
        <div className="flex justify-end">
          <span className="text-sm text-muted-foreground">
            Last updated: {lastUpdated.toLocaleTimeString()}
          </span>
        </div>
      )}

      {/* System Health Overview */}
      <section>
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-semibold">System Health</h2>
          <HealthBadge status={healthStatus} />
        </div>
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
          <MetricCard
            title="CPU Usage"
            value={computed?.cpuUsage.toFixed(1) ?? '--'}
            unit="%"
            progress={computed?.cpuUsage}
            status={computed ? getHealthStatus(computed.cpuUsage, 70, 90) : undefined}
            isLoading={metricsLoading}
          />
          <MetricCard
            title="Memory Usage"
            value={computed?.memoryUsage.toFixed(1) ?? '--'}
            unit="%"
            progress={computed?.memoryUsage}
            status={computed ? getHealthStatus(computed.memoryUsage, 75, 90) : undefined}
            isLoading={metricsLoading}
          />
          <MetricCard
            title="Disk Usage"
            value={computed?.diskUsage.toFixed(1) ?? '--'}
            unit="%"
            progress={computed?.diskUsage}
            status={computed ? getHealthStatus(computed.diskUsage, 80, 95) : undefined}
            isLoading={metricsLoading}
          />
          <MetricCard
            title="GPU Usage"
            value={computed?.gpuUsage.toFixed(1) ?? '--'}
            unit="%"
            progress={computed?.gpuUsage}
            status={computed ? getHealthStatus(computed.gpuUsage, 80, 95) : undefined}
            isLoading={metricsLoading}
          />
        </div>
      </section>

      {/* Performance Metrics */}
      <section>
        <h2 className="text-lg font-semibold mb-4">Performance</h2>
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
          <MetricCard
            title="Active Adapters"
            value={computed?.adapterCount ?? '--'}
            isLoading={metricsLoading}
          />
          <MetricCard
            title="Active Sessions"
            value={computed?.activeSessions ?? '--'}
            isLoading={metricsLoading}
          />
          <MetricCard
            title="Tokens/sec"
            value={computed?.tokensPerSecond.toFixed(1) ?? '--'}
            isLoading={metricsLoading}
          />
          <MetricCard
            title="Latency (P95)"
            value={computed?.latencyP95Ms.toFixed(0) ?? '--'}
            unit="ms"
            isLoading={metricsLoading}
          />
        </div>
      </section>

      {/* Infrastructure Summary */}
      <section>
        <h2 className="text-lg font-semibold mb-4">Infrastructure</h2>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <Card>
            <CardHeader>
              <div className="flex items-center justify-between">
                <CardTitle>Nodes</CardTitle>
                <Badge variant="secondary">{nodeStats.total}</Badge>
              </div>
              <CardDescription>Cluster node status</CardDescription>
            </CardHeader>
            <CardContent>
              {nodesLoading ? (
                <div className="space-y-2">
                  <Skeleton className="h-4 w-full" />
                  <Skeleton className="h-4 w-3/4" />
                </div>
              ) : (
                <div className="grid grid-cols-3 gap-4 text-center">
                  <div>
                    <div className="text-2xl font-bold text-green-600">{nodeStats.healthy}</div>
                    <div className="text-sm text-muted-foreground">Healthy</div>
                  </div>
                  <div>
                    <div className="text-2xl font-bold text-yellow-600">{nodeStats.offline}</div>
                    <div className="text-sm text-muted-foreground">Offline</div>
                  </div>
                  <div>
                    <div className="text-2xl font-bold text-red-600">{nodeStats.error}</div>
                    <div className="text-sm text-muted-foreground">Error</div>
                  </div>
                </div>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <div className="flex items-center justify-between">
                <CardTitle>Workers</CardTitle>
                <Badge variant="secondary">{workerStats.total}</Badge>
              </div>
              <CardDescription>Worker process status</CardDescription>
            </CardHeader>
            <CardContent>
              {workersLoading ? (
                <div className="space-y-2">
                  <Skeleton className="h-4 w-full" />
                  <Skeleton className="h-4 w-3/4" />
                </div>
              ) : (
                <div className="grid grid-cols-3 gap-4 text-center">
                  <div>
                    <div className="text-2xl font-bold text-green-600">{workerStats.running}</div>
                    <div className="text-sm text-muted-foreground">Running</div>
                  </div>
                  <div>
                    <div className="text-2xl font-bold text-gray-600">{workerStats.stopped}</div>
                    <div className="text-sm text-muted-foreground">Stopped</div>
                  </div>
                  <div>
                    <div className="text-2xl font-bold text-red-600">{workerStats.errored}</div>
                    <div className="text-sm text-muted-foreground">Error</div>
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </section>
    </div>
  );
}
