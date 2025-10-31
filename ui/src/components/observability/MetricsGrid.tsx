import React, { useEffect, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { MetricsChart } from '../MetricsChart';
import apiClient from '../../api/client';
import { logger, toError } from '../../utils/logger';

interface MetricsSnapshot {
  timestamp: number;
  counters: Record<string, number>;
  gauges: Record<string, number>;
  histograms: Record<string, any>;
}

export function MetricsGrid() {
  const [snapshot, setSnapshot] = useState<MetricsSnapshot | null>(null);
  const [timeSeries, setTimeSeries] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedMetric, setSelectedMetric] = useState<string>('');

  useEffect(() => {
    const fetchMetrics = async () => {
      try {
        // Fetch current snapshot
        const data = await apiClient.getMetricsSnapshot();
        const transformedSnapshot: MetricsSnapshot = {
          timestamp: data.timestamp,
          counters: data.counters,
          gauges: data.gauges,
          histograms: data.histograms,
        };
        setSnapshot(transformedSnapshot);

        // Fetch time series data for the last hour
        const endTime = new Date();
        const startTime = new Date(endTime.getTime() - 60 * 60 * 1000); // 1 hour ago

        const seriesData = await apiClient.getMetricsSeries({
          start_ms: startTime.getTime(),
          end_ms: endTime.getTime(),
        });

        setTimeSeries(seriesData);
      } catch (err) {
        logger.error('Failed to fetch metrics', { component: 'MetricsGrid', operation: 'fetchMetrics' }, toError(err));
      } finally {
        setLoading(false);
      }
    };

    fetchMetrics();
    const interval = setInterval(fetchMetrics, 5000); // Update every 5 seconds
    return () => clearInterval(interval);
  }, []);

  if (loading) {
    return <div className="text-center py-8">Loading metrics...</div>;
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
              {snapshot.gauges['adapteros_tokens_per_second']?.toFixed(1) || '0'}
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
