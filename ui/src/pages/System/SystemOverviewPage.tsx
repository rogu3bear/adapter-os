import { useMemo, useState, useCallback } from 'react';
import { Link } from 'react-router-dom';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
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
} from '@/hooks/system/useSystemMetrics';
import { useMetricsStream } from '@/hooks/streaming/useStreamingEndpoints';
import type { MetricsSnapshotEvent } from '@/api/streaming-types';
import { Activity, Wifi, WifiOff } from 'lucide-react';
import { HealthBadge, MetricCard } from './shared/MetricComponents';
import {
  buildSystemNodesLink,
  buildSystemWorkersLink,
  buildSystemMemoryLink,
  buildSystemMetricsLink,
} from '@/utils/navLinks';

function QuickLinkCard({
  title,
  description,
  href,
  count,
  isLoading,
}: {
  title: string;
  description: string;
  href: string;
  count?: number;
  isLoading?: boolean;
}) {
  return (
    <Card className="hover:bg-muted/50 transition-colors">
      <Link to={href}>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle className="text-lg">{title}</CardTitle>
            {isLoading ? (
              <Skeleton className="h-6 w-8" />
            ) : count !== undefined ? (
              <Badge variant="secondary">{count}</Badge>
            ) : null}
          </div>
          <CardDescription>{description}</CardDescription>
        </CardHeader>
      </Link>
    </Card>
  );
}

export default function SystemOverviewPage() {
  const [useSSE, setUseSSE] = useState(true);
  const [sseMetrics, setSSEMetrics] = useState<MetricsSnapshotEvent | null>(null);

  // SSE stream for live metrics
  const { data: sseData, error: sseError, connected: sseConnected, reconnect } = useMetricsStream({
    enabled: useSSE,
    onMessage: useCallback((event: unknown) => {
      if (event && typeof event === 'object' && 'system' in event) {
        setSSEMetrics(event as MetricsSnapshotEvent);
      }
    }, []),
  });

  // Fallback to polling if SSE is disabled or fails
  const { metrics, isLoading: metricsLoading, error: metricsError, lastUpdated } = useSystemMetrics('normal', !useSSE);
  const { nodes, isLoading: nodesLoading } = useNodes('slow');
  const { workers, isLoading: workersLoading } = useWorkers(undefined, undefined, 'slow');

  // Use SSE metrics if available, otherwise fall back to polling
  const activeMetrics = useSSE && sseMetrics ? {
    cpu_usage_percent: sseMetrics.system.cpu_percent,
    memory_usage_percent: sseMetrics.system.memory_percent,
    disk_usage_percent: sseMetrics.system.disk_percent,
    gpu_usage_percent: 0,
    tokens_per_second: sseMetrics.throughput.tokens_per_second,
    inferences_per_second: sseMetrics.throughput.inferences_per_second,
    latency_p95_ms: sseMetrics.latency.p95_ms,
    active_adapters: 0,
    active_sessions: 0,
  } : metrics;

  const computed = useComputedMetrics(activeMetrics);
  const healthStatus = useSystemHealthStatus(activeMetrics);

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
    <DensityProvider pageKey="system-overview">
      <FeatureLayout
        title="System Overview"
        description="Monitor system health, nodes, and workers"
        maxWidth="xl"
        badges={[{ label: healthStatus.toUpperCase(), variant: healthStatus === 'healthy' ? 'success' : healthStatus === 'warning' ? 'warning' : 'destructive' }]}
        headerActions={
          <div className="flex items-center gap-4">
            {useSSE && (
              <div className="flex items-center gap-2 text-sm">
                {sseConnected ? (
                  <>
                    <Wifi className="h-4 w-4 text-green-500" />
                    <span className="text-muted-foreground">Live</span>
                  </>
                ) : sseError ? (
                  <>
                    <WifiOff className="h-4 w-4 text-destructive" />
                    <span className="text-destructive text-xs">Disconnected</span>
                    <Button variant="ghost" size="sm" onClick={reconnect}>
                      Reconnect
                    </Button>
                  </>
                ) : (
                  <>
                    <Activity className="h-4 w-4 animate-pulse text-yellow-500" />
                    <span className="text-muted-foreground">Connecting...</span>
                  </>
                )}
              </div>
            )}
            {lastUpdated && !useSSE && (
              <span className="text-sm text-muted-foreground">
                Last updated: {lastUpdated.toLocaleTimeString()}
              </span>
            )}
            {sseMetrics && useSSE && (
              <span className="text-sm text-muted-foreground">
                Last updated: {new Date(sseMetrics.timestamp_ms).toLocaleTimeString()}
              </span>
            )}
            <Button
              variant="outline"
              size="sm"
              onClick={() => setUseSSE(!useSSE)}
            >
              {useSSE ? 'Switch to Polling' : 'Switch to Live'}
            </Button>
          </div>
        }
      >
        {(metricsError || (useSSE && sseError && !sseConnected)) && (
          <Card className="border-destructive bg-destructive/10 mb-6">
            <CardContent className="pt-6">
              <p className="text-destructive">
                Failed to load system metrics: {metricsError?.message || sseError?.message}
              </p>
              {useSSE && sseError && (
                <Button variant="outline" size="sm" onClick={() => setUseSSE(false)} className="mt-2">
                  Fall back to polling
                </Button>
              )}
            </CardContent>
          </Card>
        )}

        {/* System Health Overview */}
        <section className="mb-8">
          <h2 className="text-lg font-semibold mb-4">System Health</h2>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
            <MetricCard
              title="CPU Usage"
              value={computed?.cpuUsage != null ? computed.cpuUsage.toFixed(1) : '--'}
              unit="%"
              progress={computed?.cpuUsage ?? undefined}
              status={computed ? getHealthStatus(computed.cpuUsage, 70, 90) : undefined}
              isLoading={metricsLoading}
            />
            <MetricCard
              title="Memory Usage"
              value={computed?.memoryUsage != null ? computed.memoryUsage.toFixed(1) : '--'}
              unit="%"
              progress={computed?.memoryUsage ?? undefined}
              status={computed ? getHealthStatus(computed.memoryUsage, 75, 90) : undefined}
              isLoading={metricsLoading}
            />
            <MetricCard
              title="Disk Usage"
              value={computed?.diskUsage != null ? computed.diskUsage.toFixed(1) : '--'}
              unit="%"
              progress={computed?.diskUsage ?? undefined}
              status={computed ? getHealthStatus(computed.diskUsage, 80, 95) : undefined}
              isLoading={metricsLoading}
            />
            <MetricCard
              title="GPU Usage"
              value={computed?.gpuUsage != null ? computed.gpuUsage.toFixed(1) : '--'}
              unit="%"
              progress={computed?.gpuUsage ?? undefined}
              status={computed ? getHealthStatus(computed.gpuUsage, 80, 95) : undefined}
              isLoading={metricsLoading}
            />
          </div>
        </section>

        {/* Performance Metrics */}
        <section className="mb-8">
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
              value={computed?.tokensPerSecond != null ? computed.tokensPerSecond.toFixed(1) : '--'}
              isLoading={metricsLoading}
            />
            <MetricCard
              title="Latency (P95)"
              value={computed?.latencyP95Ms != null ? computed.latencyP95Ms.toFixed(0) : '--'}
              unit="ms"
              isLoading={metricsLoading}
            />
          </div>
        </section>

        {/* Node and Worker Summary */}
        <section className="mb-8">
          <h2 className="text-lg font-semibold mb-4">Infrastructure</h2>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle>Nodes</CardTitle>
                  <Button variant="outline" size="sm" asChild>
                    <Link to={buildSystemNodesLink()}>View All</Link>
                  </Button>
                </div>
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
                  <Button variant="outline" size="sm" asChild>
                    <Link to={buildSystemWorkersLink()}>View All</Link>
                  </Button>
                </div>
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

        {/* Quick Links */}
        <section>
          <h2 className="text-lg font-semibold mb-4">Quick Access</h2>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
            <QuickLinkCard
              title="Nodes"
              description="Manage cluster nodes"
              href={buildSystemNodesLink()}
              count={nodeStats.total}
              isLoading={nodesLoading}
            />
            <QuickLinkCard
              title="Workers"
              description="View worker processes"
              href={buildSystemWorkersLink()}
              count={workerStats.total}
              isLoading={workersLoading}
            />
            <QuickLinkCard
              title="Memory"
              description="Monitor memory usage"
              href={buildSystemMemoryLink()}
            />
            <QuickLinkCard
              title="Metrics"
              description="Detailed system metrics"
              href={buildSystemMetricsLink()}
            />
          </div>
        </section>
      </FeatureLayout>
    </DensityProvider>
  );
}
