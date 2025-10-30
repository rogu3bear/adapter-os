import React, { useEffect, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { MetricsChart } from '../MetricsChart';
import apiClient from '../../api/client';

interface MetricsSnapshot {
  timestamp: number;
  counters: Record<string, number>;
  gauges: Record<string, number>;
  histograms: Record<string, any>;
}

export function MetricsGrid() {
  const [snapshot, setSnapshot] = useState<MetricsSnapshot | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const fetchSnapshot = async () => {
      try {
        const data = await apiClient.request<MetricsSnapshot>('/api/metrics/snapshot');
        setSnapshot(data);
      } catch (err) {
        console.error('Failed to fetch metrics snapshot', err);
      } finally {
        setLoading(false);
      }
    };

    fetchSnapshot();
    const interval = setInterval(fetchSnapshot, 1000); // Update every second
    return () => clearInterval(interval);
  }, []);

  if (loading) {
    return <div className="text-center py-8">Loading metrics...</div>;
  }

  if (!snapshot) {
    return <div className="text-center py-8">No metrics available</div>;
  }

  return (
    <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
      {/* Queue Depth */}
      <Card>
        <CardHeader>
          <CardTitle>Queue Depth</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="text-3xl font-bold">
            {snapshot.gauges['adapteros_queue_depth']?.toFixed(1) || '0'}
          </div>
        </CardContent>
      </Card>

      {/* Tokens per Second */}
      <Card>
        <CardHeader>
          <CardTitle>Tokens/sec</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="text-3xl font-bold">
            {snapshot.gauges['adapteros_tokens_per_second']?.toFixed(1) || '0'}
          </div>
        </CardContent>
      </Card>

      {/* Active Sessions */}
      <Card>
        <CardHeader>
          <CardTitle>Active Sessions</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="text-3xl font-bold">
            {snapshot.gauges['adapteros_active_sessions']?.toFixed(0) || '0'}
          </div>
        </CardContent>
      </Card>

      {/* Memory Usage */}
      <Card>
        <CardHeader>
          <CardTitle>Memory Usage</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="text-3xl font-bold">
            {(snapshot.gauges['adapteros_memory_usage_bytes'] / 1024 / 1024).toFixed(0) || '0'} MB
          </div>
        </CardContent>
      </Card>

      {/* Latency Chart */}
      <Card className="md:col-span-2 lg:col-span-3">
        <CardHeader>
          <CardTitle>Latency (p95)</CardTitle>
        </CardHeader>
        <CardContent>
          <MetricsChart
            data={[]} // Would populate from time series endpoint
            height={200}
          />
        </CardContent>
      </Card>
    </div>
  );
}
