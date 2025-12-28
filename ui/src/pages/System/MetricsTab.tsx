import { useState, useEffect } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { LineChart, Line, AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer } from 'recharts';
import { useSystemMetrics, useComputedMetrics } from '@/hooks/system/useSystemMetrics';
import { METRIC_COLORS } from '@/constants/chart-colors';

interface MetricDataPoint {
  timestamp: string;
  cpuUsage: number | null;
  memoryUsage: number | null;
  diskUsage: number | null;
  gpuUsage: number | null;
  tokensPerSecond: number | null;
  latencyP95Ms: number | null;
}

export default function MetricsTab() {
  const { metrics, isLoading, error, lastUpdated } = useSystemMetrics('fast');
  const computed = useComputedMetrics(metrics);
  const [historicalData, setHistoricalData] = useState<MetricDataPoint[]>([]);

  // Collect metrics over time for charts
  useEffect(() => {
    if (!computed) return;

    const dataPoint: MetricDataPoint = {
      timestamp: new Date().toLocaleTimeString(),
      cpuUsage: computed.cpuUsage,
      memoryUsage: computed.memoryUsage,
      diskUsage: computed.diskUsage,
      gpuUsage: computed.gpuUsage,
      tokensPerSecond: computed.tokensPerSecond,
      latencyP95Ms: computed.latencyP95Ms,
    };

    setHistoricalData((prev) => {
      // Keep last 20 data points
      const updated = [...prev, dataPoint];
      return updated.slice(-20);
    });
  }, [computed]);

  if (error) {
    return (
      <DensityProvider pageKey="system-metrics">
        <FeatureLayout
          title="System Metrics"
          description="Real-time system performance metrics and charts"
          maxWidth="xl"
        >
          <Card className="border-destructive bg-destructive/10">
            <CardContent className="pt-6">
              <p className="text-destructive">Failed to load metrics: {error.message}</p>
            </CardContent>
          </Card>
        </FeatureLayout>
      </DensityProvider>
    );
  }

  if (isLoading && !computed) {
    return (
      <DensityProvider pageKey="system-metrics">
        <FeatureLayout
          title="System Metrics"
          description="Real-time system performance metrics and charts"
          maxWidth="xl"
        >
          <div className="space-y-6">
            <Skeleton className="h-64 w-full" />
            <Skeleton className="h-64 w-full" />
          </div>
        </FeatureLayout>
      </DensityProvider>
    );
  }

  return (
    <DensityProvider pageKey="system-metrics">
      <FeatureLayout
        title="System Metrics"
        description="Real-time system performance metrics and charts"
        maxWidth="xl"
        badges={lastUpdated ? [{ label: `Updated ${lastUpdated.toLocaleTimeString()}`, variant: 'secondary' as const }] : undefined}
      >
        <div className="space-y-6">
      {/* System Resource Usage Chart */}
      <Card>
        <CardHeader>
          <CardTitle>System Resource Usage</CardTitle>
          <CardDescription>Real-time CPU, Memory, Disk, and GPU utilization</CardDescription>
        </CardHeader>
        <CardContent>
          {historicalData.length === 0 ? (
            <div className="h-64 flex items-center justify-center text-muted-foreground">
              Collecting data...
            </div>
          ) : (
            <ResponsiveContainer width="100%" height={300}>
              <LineChart data={historicalData}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis dataKey="timestamp" />
                <YAxis domain={[0, 100]} />
                <Tooltip />
                <Legend />
                <Line
                  type="monotone"
                  dataKey="cpuUsage"
                  stroke={METRIC_COLORS.cpu}
                  name="CPU %"
                  strokeWidth={2}
                />
                <Line
                  type="monotone"
                  dataKey="memoryUsage"
                  stroke={METRIC_COLORS.memory}
                  name="Memory %"
                  strokeWidth={2}
                />
                <Line
                  type="monotone"
                  dataKey="diskUsage"
                  stroke={METRIC_COLORS.disk}
                  name="Disk %"
                  strokeWidth={2}
                />
                <Line
                  type="monotone"
                  dataKey="gpuUsage"
                  stroke={METRIC_COLORS.gpu}
                  name="GPU %"
                  strokeWidth={2}
                />
              </LineChart>
            </ResponsiveContainer>
          )}
        </CardContent>
      </Card>

      {/* Performance Metrics Chart */}
      <Card>
        <CardHeader>
          <CardTitle>Performance Metrics</CardTitle>
          <CardDescription>Throughput and latency over time</CardDescription>
        </CardHeader>
        <CardContent>
          {historicalData.length === 0 ? (
            <div className="h-64 flex items-center justify-center text-muted-foreground">
              Collecting data...
            </div>
          ) : (
            <ResponsiveContainer width="100%" height={300}>
              <AreaChart data={historicalData}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis dataKey="timestamp" />
                <YAxis yAxisId="left" />
                <YAxis yAxisId="right" orientation="right" />
                <Tooltip />
                <Legend />
                <Area
                  yAxisId="left"
                  type="monotone"
                  dataKey="tokensPerSecond"
                  stroke={METRIC_COLORS.tokensPerSecond}
                  fill={METRIC_COLORS.tokensPerSecond}
                  fillOpacity={0.6}
                  name="Tokens/sec"
                />
                <Area
                  yAxisId="right"
                  type="monotone"
                  dataKey="latencyP95Ms"
                  stroke={METRIC_COLORS.latency}
                  fill={METRIC_COLORS.latency}
                  fillOpacity={0.6}
                  name="Latency (ms)"
                />
              </AreaChart>
            </ResponsiveContainer>
          )}
        </CardContent>
      </Card>

      {/* Current Metrics Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <Card>
          <CardHeader className="pb-2">
            <CardDescription>CPU Usage</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {computed?.cpuUsage !== null && computed?.cpuUsage !== undefined ? `${computed.cpuUsage.toFixed(1)}%` : '--'}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Memory Usage</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {computed?.memoryUsage !== null && computed?.memoryUsage !== undefined ? `${computed.memoryUsage.toFixed(1)}%` : '--'}
            </div>
            <div className="text-xs text-muted-foreground">
              {computed?.memoryUsedGb !== null && computed?.memoryUsedGb !== undefined ? computed.memoryUsedGb.toFixed(2) : '--'} / {computed?.memoryTotalGb !== null && computed?.memoryTotalGb !== undefined ? computed.memoryTotalGb.toFixed(2) : '--'} GB
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>GPU Usage</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {computed?.gpuUsage !== null && computed?.gpuUsage !== undefined ? `${computed.gpuUsage.toFixed(1)}%` : '--'}
            </div>
            <div className="text-xs text-muted-foreground">
              {computed?.gpuMemoryUsedMb !== null && computed?.gpuMemoryUsedMb !== undefined ? computed.gpuMemoryUsedMb.toFixed(0) : '--'} / {computed?.gpuMemoryTotalMb !== null && computed?.gpuMemoryTotalMb !== undefined ? computed.gpuMemoryTotalMb.toFixed(0) : '--'} MB
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Disk Usage</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {computed?.diskUsage !== null && computed?.diskUsage !== undefined ? `${computed.diskUsage.toFixed(1)}%` : '--'}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Network RX</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {computed?.networkRx !== null && computed?.networkRx !== undefined ? (computed.networkRx / 1024 / 1024).toFixed(2) : '--'}
            </div>
            <div className="text-xs text-muted-foreground">MB received</div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Network TX</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {computed?.networkTx !== null && computed?.networkTx !== undefined ? (computed.networkTx / 1024 / 1024).toFixed(2) : '--'}
            </div>
            <div className="text-xs text-muted-foreground">MB transmitted</div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Tokens/sec</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {computed?.tokensPerSecond !== null && computed?.tokensPerSecond !== undefined ? computed.tokensPerSecond.toFixed(1) : '--'}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Latency (P95)</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {computed?.latencyP95Ms !== null && computed?.latencyP95Ms !== undefined ? computed.latencyP95Ms.toFixed(0) : '--'}
            </div>
            <div className="text-xs text-muted-foreground">milliseconds</div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>CPU Temperature</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {computed?.cpuTemp !== null && computed?.cpuTemp !== undefined ? `${computed.cpuTemp.toFixed(1)}°C` : '--'}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>GPU Temperature</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {computed?.gpuTemp !== null && computed?.gpuTemp !== undefined ? `${computed.gpuTemp.toFixed(1)}°C` : '--'}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>GPU Power</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {computed?.gpuPower !== null && computed?.gpuPower !== undefined ? computed.gpuPower.toFixed(1) : '--'}
            </div>
            <div className="text-xs text-muted-foreground">watts</div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Cache Hit Rate</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {computed?.cacheHitRate !== null && computed?.cacheHitRate !== undefined ? `${(computed.cacheHitRate * 100).toFixed(1)}%` : '--'}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Error Rate</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {computed?.errorRate !== null && computed?.errorRate !== undefined ? `${(computed.errorRate * 100).toFixed(2)}%` : '--'}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Active Adapters</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{computed?.adapterCount ?? '--'}</div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Active Sessions</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{computed?.activeSessions ?? '--'}</div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Disk Read</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{computed?.diskReadMbps !== null && computed?.diskReadMbps !== undefined ? computed.diskReadMbps.toFixed(2) : '--'}</div>
            <div className="text-xs text-muted-foreground">MB/s</div>
          </CardContent>
        </Card>
      </div>
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
