import React, { useState, useCallback } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { DateRangePicker, type DateRange } from '@/components/ui/date-range-picker';
import { apiClient } from '@/api/client';
import { DensityProvider, useDensity } from '@/contexts/DensityContext';
import { DensityControls } from '@/components/ui/density-controls';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { HelpTooltip } from '@/components/ui/help-tooltip';
import { usePolling } from '@/hooks/usePolling';
import { toast } from 'sonner';
import {
  RefreshCw,
  TrendingUp,
  Activity,
  BarChart3,
  Download,
} from 'lucide-react';
import {
  LineChart,
  Line,
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  Legend,
  ResponsiveContainer,
} from 'recharts';
import type { MetricsSeriesResponse } from '@/api/types';

const METRIC_OPTIONS = [
  { value: 'cpu_usage', label: 'CPU Usage' },
  { value: 'memory_usage', label: 'Memory Usage' },
  { value: 'gpu_usage', label: 'GPU Usage' },
  { value: 'disk_usage', label: 'Disk Usage' },
  { value: 'tokens_per_second', label: 'Tokens/Second' },
  { value: 'latency_p50_ms', label: 'Latency P50' },
  { value: 'latency_p95_ms', label: 'Latency P95' },
  { value: 'latency_p99_ms', label: 'Latency P99' },
  { value: 'adapter_activations', label: 'Adapter Activations' },
  { value: 'error_rate', label: 'Error Rate' },
];

const TIME_RANGE_OPTIONS = [
  { value: '1h', label: 'Last Hour' },
  { value: '6h', label: 'Last 6 Hours' },
  { value: '24h', label: 'Last 24 Hours' },
  { value: '7d', label: 'Last 7 Days' },
  { value: '30d', label: 'Last 30 Days' },
  { value: 'custom', label: 'Custom Range' },
];

const AGGREGATION_OPTIONS = [
  { value: 'avg', label: 'Average' },
  { value: 'min', label: 'Minimum' },
  { value: 'max', label: 'Maximum' },
  { value: 'sum', label: 'Sum' },
];

function AdvancedMetricsPageInner() {
  const { density, setDensity } = useDensity();
  const { can } = useRBAC();

  const [selectedMetric, setSelectedMetric] = useState('cpu_usage');
  const [timeRange, setTimeRange] = useState('1h');
  const [customDateRange, setCustomDateRange] = useState<DateRange | undefined>();
  const [aggregation, setAggregation] = useState('avg');

  // Fetch metrics series
  const fetchMetricsSeries = useCallback(async () => {
    let startTime: number;
    let endTime: number;

    if (timeRange === 'custom' && customDateRange) {
      startTime = customDateRange.from.getTime();
      endTime = customDateRange.to.getTime();
    } else {
      startTime = new Date(getStartTime(timeRange)).getTime();
      endTime = new Date().getTime();
    }

    return await apiClient.getMetricsSeries({
      series_name: selectedMetric,
      start_ms: startTime,
      end_ms: endTime,
    });
  }, [selectedMetric, timeRange, customDateRange]);

  const {
    data: metricsSeries = [],
    isLoading,
    error,
    refetch,
    lastUpdated,
  } = usePolling<MetricsSeriesResponse[]>(fetchMetricsSeries, 'normal', {
    enabled: true,
    operationName: 'fetchMetricsSeries',
  });

  // Calculate start time based on time range
  function getStartTime(range: string): string {
    const now = new Date();
    switch (range) {
      case '1h':
        return new Date(now.getTime() - 60 * 60 * 1000).toISOString();
      case '6h':
        return new Date(now.getTime() - 6 * 60 * 60 * 1000).toISOString();
      case '24h':
        return new Date(now.getTime() - 24 * 60 * 60 * 1000).toISOString();
      case '7d':
        return new Date(now.getTime() - 7 * 24 * 60 * 60 * 1000).toISOString();
      case '30d':
        return new Date(now.getTime() - 30 * 24 * 60 * 60 * 1000).toISOString();
      default:
        return new Date(now.getTime() - 60 * 60 * 1000).toISOString();
    }
  }

  // Transform series data for charts - flatten data_points from all series
  const chartData = metricsSeries.flatMap((series) =>
    series.data_points.map((point) => ({
      timestamp: new Date(point.timestamp).toLocaleTimeString(),
      value: point.value,
    }))
  );

  // Export data as CSV
  const handleExportCSV = () => {
    if (chartData.length === 0) {
      toast.error('No data to export');
      return;
    }

    const csvHeader = 'Timestamp,Value\n';
    const csvRows = chartData
      .map((point) => `${point.timestamp},${point.value}`)
      .join('\n');
    const csvContent = csvHeader + csvRows;

    const blob = new Blob([csvContent], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `metrics-${selectedMetric}-${new Date().toISOString()}.csv`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);

    toast.success('Metrics exported successfully');
  };

  // Calculate statistics from flattened chart data
  const stats = chartData.length > 0 ? {
    min: Math.min(...chartData.map((p) => p.value)),
    max: Math.max(...chartData.map((p) => p.value)),
    avg: chartData.reduce((sum, p) => sum + p.value, 0) / chartData.length,
    latest: chartData[chartData.length - 1].value,
  } : null;

  return (
    <FeatureLayout
      title="Advanced Metrics"
      description="Time-series metrics and performance analysis"
      headerActions={<DensityControls density={density} onDensityChange={setDensity} />}
    >
      <div className="space-y-6">
        {/* Controls */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              Configuration
              <HelpTooltip helpId="metrics-config">
                <span className="cursor-help text-muted-foreground">(?)</span>
              </HelpTooltip>
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
              <div>
                <Label htmlFor="metric">Metric</Label>
                <Select value={selectedMetric} onValueChange={setSelectedMetric}>
                  <SelectTrigger id="metric">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {METRIC_OPTIONS.map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>
                        {opt.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div>
                <Label htmlFor="time-range">Time Range</Label>
                <Select value={timeRange} onValueChange={setTimeRange}>
                  <SelectTrigger id="time-range">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {TIME_RANGE_OPTIONS.map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>
                        {opt.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div>
                <Label htmlFor="aggregation">Aggregation</Label>
                <Select value={aggregation} onValueChange={setAggregation}>
                  <SelectTrigger id="aggregation">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {AGGREGATION_OPTIONS.map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>
                        {opt.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="flex items-end gap-2">
                <Button onClick={() => refetch()} disabled={isLoading} variant="outline">
                  <RefreshCw className={`h-4 w-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
                  Refresh
                </Button>
                <Button
                  onClick={handleExportCSV}
                  disabled={metricsSeries.length === 0}
                  variant="outline"
                >
                  <Download className="h-4 w-4 mr-2" />
                  Export
                </Button>
              </div>
            </div>

            {/* Custom Date Range Picker */}
            {timeRange === 'custom' && (
              <div className="mt-4">
                <DateRangePicker
                  label="Custom Date Range"
                  value={customDateRange}
                  onChange={setCustomDateRange}
                  maxDate={new Date()}
                />
              </div>
            )}

            {lastUpdated && (
              <div className="mt-4 text-xs text-muted-foreground">
                Last updated: {lastUpdated.toLocaleTimeString()}
              </div>
            )}
          </CardContent>
        </Card>

        {/* Statistics */}
        {stats && (
          <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium text-muted-foreground">
                  Minimum
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{stats.min.toFixed(2)}</div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium text-muted-foreground">
                  Maximum
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{stats.max.toFixed(2)}</div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium text-muted-foreground">
                  Average
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{stats.avg.toFixed(2)}</div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium text-muted-foreground">
                  Latest
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{stats.latest.toFixed(2)}</div>
              </CardContent>
            </Card>
          </div>
        )}

        {/* Time-Series Chart */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <BarChart3 className="h-5 w-5" />
              {METRIC_OPTIONS.find((m) => m.value === selectedMetric)?.label || 'Metrics'} Over Time
              {metricsSeries.length > 0 && (
                <span className="ml-2 text-sm font-normal text-muted-foreground">
                  ({metricsSeries.length} data points)
                </span>
              )}
            </CardTitle>
            <CardDescription>
              {aggregation.toUpperCase()} aggregation over {
                timeRange === 'custom' && customDateRange
                  ? `${customDateRange.from.toLocaleDateString()} - ${customDateRange.to.toLocaleDateString()}`
                  : TIME_RANGE_OPTIONS.find((t) => t.value === timeRange)?.label.toLowerCase()
              }
            </CardDescription>
          </CardHeader>
          <CardContent>
            {error && (
              <ErrorRecovery
                error={error.message}
                onRetry={() => refetch()}
              />
            )}

            {isLoading && metricsSeries.length === 0 ? (
              <div className="flex justify-center py-8">
                <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
              </div>
            ) : chartData.length === 0 ? (
              <div className="text-center py-8 text-muted-foreground">
                No data available for the selected time range
              </div>
            ) : (
              <ResponsiveContainer width="100%" height={400}>
                <AreaChart data={chartData}>
                  <defs>
                    <linearGradient id="colorValue" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="5%" stopColor="#8884d8" stopOpacity={0.8} />
                      <stop offset="95%" stopColor="#8884d8" stopOpacity={0} />
                    </linearGradient>
                  </defs>
                  <CartesianGrid strokeDasharray="3 3" />
                  <XAxis
                    dataKey="timestamp"
                    tick={{ fontSize: 12 }}
                    angle={-45}
                    textAnchor="end"
                    height={80}
                  />
                  <YAxis tick={{ fontSize: 12 }} />
                  <Tooltip />
                  <Legend />
                  <Area
                    type="monotone"
                    dataKey="value"
                    stroke="#8884d8"
                    fillOpacity={1}
                    fill="url(#colorValue)"
                    name={METRIC_OPTIONS.find((m) => m.value === selectedMetric)?.label}
                  />
                </AreaChart>
              </ResponsiveContainer>
            )}
          </CardContent>
        </Card>
      </div>
    </FeatureLayout>
  );
}

export default function AdvancedMetricsPage() {
  return (
    <DensityProvider pageKey="advanced-metrics">
      <AdvancedMetricsPageInner />
    </DensityProvider>
  );
}
