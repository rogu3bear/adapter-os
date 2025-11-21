import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { MetricsChart } from '../MetricsChart';
import { Skeleton } from '../ui/skeleton';
import apiClient from '../../api/client';
import { logger } from '../../utils/logger';
import { usePolling } from '../../hooks/usePolling';

interface MetricsSnapshot {
  timestamp: number;
  counters: Record<string, number>;
  gauges: Record<string, number>;
  histograms: Record<string, any>;
}

interface MetricsData {
  snapshot: MetricsSnapshot;
  timeSeries: any[];
}

export function MetricsGrid() {
  const [selectedMetric, setSelectedMetric] = useState<string>('');

  const fetchMetrics = async (): Promise<MetricsData> => {
    // Fetch current snapshot
    const data = await apiClient.getMetricsSnapshot();
    const transformedSnapshot: MetricsSnapshot = {
      timestamp: data.timestamp,
      counters: data.counters,
      gauges: data.gauges,
      histograms: data.histograms,
    };

    // Fetch time series data for the last hour
    const endTime = new Date();
    const startTime = new Date(endTime.getTime() - 60 * 60 * 1000); // 1 hour ago

    const seriesData = await apiClient.getMetricsSeries({
      start_ms: startTime.getTime(),
      end_ms: endTime.getTime(),
    });

    return { snapshot: transformedSnapshot, timeSeries: seriesData };
  };

  const { data, isLoading: loading, error } = usePolling(
    fetchMetrics,
    'normal', // Reduced from fast to reduce rate limiting
    {
      showLoadingIndicator: false,
      onError: (err) => {
        logger.error('Failed to fetch metrics', { component: 'MetricsGrid', operation: 'fetchMetrics' }, err);
      }
    }
  );

  const snapshot = data?.snapshot ?? null;
  const timeSeries = data?.timeSeries ?? [];

  // Major error: metrics loading failure
  if (error) {
    return (
      <div className="text-center py-8">
        <div className="text-red-600 mb-2">Failed to load metrics</div>
        <div className="text-sm text-muted-foreground">Please refresh the page or try again later.</div>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="space-y-6">
        <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-24 w-full" />
        </div>
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }

  if (!snapshot) {
    return <div className="text-center py-8">No metrics available</div>;
  }

  const availableMetrics = Object.keys(snapshot.gauges).concat(Object.keys(snapshot.counters));

  return (
    <div className="space-y-6">
      {/* Current Metrics Grid */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
        {/* Queue Depth */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm">Queue Depth</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {snapshot.gauges['adapteros_queue_depth']?.toFixed(1) || '0'}
            </div>
            <p className="text-xs text-muted-foreground">Current pending requests</p>
          </CardContent>
        </Card>

        {/* Tokens per Second */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm">Tokens/sec</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {snapshot.gauges['adapteros_tokens_per_sec']?.toFixed(1) || '0'}
            </div>
            <p className="text-xs text-muted-foreground">Inference throughput</p>
          </CardContent>
        </Card>

        {/* Active Sessions */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm">Active Sessions</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {snapshot.gauges['adapteros_active_sessions']?.toFixed(0) || '0'}
            </div>
            <p className="text-xs text-muted-foreground">Concurrent users</p>
          </CardContent>
        </Card>

        {/* Memory Usage */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm">Memory Usage</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {(snapshot.gauges['adapteros_memory_usage_bytes'] / 1024 / 1024).toFixed(0) || '0'} MB
            </div>
            <p className="text-xs text-muted-foreground">Adapter memory footprint</p>
          </CardContent>
        </Card>
      </div>

      {/* Time Series Chart */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>Time Series Metrics</CardTitle>
            <select
              value={selectedMetric}
              onChange={(e) => setSelectedMetric(e.target.value)}
              className="px-3 py-1 border rounded text-sm"
            >
              <option value="">Select Metric...</option>
              {availableMetrics.map(metric => (
                <option key={metric} value={metric}>{metric}</option>
              ))}
            </select>
          </div>
        </CardHeader>
        <CardContent>
          {selectedMetric && timeSeries.length > 0 ? (
            <MetricsChart
              data={timeSeries
                .filter(series => series.series_name === selectedMetric)
                .flatMap(series => series.points.map((point: any) => ({
                  timestamp: new Date(point.timestamp).toLocaleTimeString(),
                  value: point.value,
                })))
              }
              height={300}
            />
          ) : (
            <div className="text-center py-8 text-muted-foreground">
              Select a metric to view its time series data
            </div>
          )}
        </CardContent>
      </Card>

      {/* Counters and Gauges Tables */}
      <div className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Gauge Metrics</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-2 max-h-48 overflow-y-auto">
              {Object.entries(snapshot.gauges).map(([key, value]) => (
                <div key={key} className="flex justify-between text-sm">
                  <span className="font-mono text-xs truncate mr-2">{key}</span>
                  <span className="font-bold">{typeof value === 'number' ? value.toFixed(2) : value}</span>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Counter Metrics</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-2 max-h-48 overflow-y-auto">
              {Object.entries(snapshot.counters).map(([key, value]) => (
                <div key={key} className="flex justify-between text-sm">
                  <span className="font-mono text-xs truncate mr-2">{key}</span>
                  <span className="font-bold">{value}</span>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
