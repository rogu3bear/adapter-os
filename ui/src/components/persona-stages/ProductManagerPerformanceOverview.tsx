import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '../ui/card';
import { Badge } from '../ui/badge';
import { Progress } from '../ui/progress';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  ChartConfig,
} from '../ui/chart';
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  AreaChart,
  Area,
  BarChart,
  Bar,
  ResponsiveContainer,
} from 'recharts';
import {
  Activity,
  Clock,
  AlertTriangle,
  CheckCircle2,
  TrendingUp,
  TrendingDown,
  Minus,
  Server,
  Zap,
  Target,
} from 'lucide-react';

interface KPIData {
  label: string;
  value: string;
  change: number;
  changeType: 'increase' | 'decrease' | 'neutral';
  icon: React.ReactNode;
  trend: number[];
}

interface TimeSeriesPoint {
  time: string;
  requests: number;
  latency: number;
  errors: number;
}

const generateTimeSeriesData = (hours: number): TimeSeriesPoint[] => {
  const data: TimeSeriesPoint[] = [];
  const now = new Date();
  for (let i = hours; i >= 0; i--) {
    const time = new Date(now.getTime() - i * 60 * 60 * 1000);
    data.push({
      time: time.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' }),
      requests: Math.floor(Math.random() * 500) + 1500 + Math.sin(i / 3) * 200,
      latency: Math.floor(Math.random() * 50) + 80 + Math.cos(i / 2) * 20,
      errors: Math.floor(Math.random() * 10) + Math.max(0, Math.sin(i) * 5),
    });
  }
  return data;
};

const chartConfig: ChartConfig = {
  requests: {
    label: 'Requests',
    color: 'hsl(var(--chart-1))',
  },
  latency: {
    label: 'Latency (ms)',
    color: 'hsl(var(--chart-2))',
  },
  errors: {
    label: 'Errors',
    color: 'hsl(var(--chart-3))',
  },
};

export default function ProductManagerPerformanceOverview() {
  const [timeRange, setTimeRange] = useState('24h');
  const [timeSeriesData, setTimeSeriesData] = useState<TimeSeriesPoint[]>([]);

  useEffect(() => {
    const hours = timeRange === '1h' ? 1 : timeRange === '6h' ? 6 : timeRange === '24h' ? 24 : 168;
    setTimeSeriesData(generateTimeSeriesData(hours));
  }, [timeRange]);

  const kpiData: KPIData[] = [
    {
      label: 'Total Requests',
      value: '2.4M',
      change: 12.5,
      changeType: 'increase',
      icon: <Activity className="h-5 w-5 text-blue-500" />,
      trend: [65, 72, 68, 80, 85, 90, 88],
    },
    {
      label: 'Avg Latency',
      value: '94ms',
      change: -8.2,
      changeType: 'decrease',
      icon: <Clock className="h-5 w-5 text-green-500" />,
      trend: [120, 115, 108, 102, 98, 95, 94],
    },
    {
      label: 'Error Rate',
      value: '0.12%',
      change: -15.3,
      changeType: 'decrease',
      icon: <AlertTriangle className="h-5 w-5 text-yellow-500" />,
      trend: [0.25, 0.22, 0.18, 0.15, 0.14, 0.13, 0.12],
    },
    {
      label: 'Uptime',
      value: '99.98%',
      change: 0.02,
      changeType: 'increase',
      icon: <Server className="h-5 w-5 text-purple-500" />,
      trend: [99.9, 99.92, 99.95, 99.96, 99.97, 99.98, 99.98],
    },
  ];

  const slaMetrics = [
    { name: 'Availability', target: 99.9, current: 99.98, unit: '%' },
    { name: 'Response Time (p99)', target: 200, current: 145, unit: 'ms' },
    { name: 'Error Rate', target: 0.5, current: 0.12, unit: '%', inverse: true },
    { name: 'Throughput', target: 10000, current: 12500, unit: 'req/s' },
  ];

  const getTrendIcon = (changeType: string) => {
    switch (changeType) {
      case 'increase':
        return <TrendingUp className="h-4 w-4" />;
      case 'decrease':
        return <TrendingDown className="h-4 w-4" />;
      default:
        return <Minus className="h-4 w-4" />;
    }
  };

  const getChangeColor = (changeType: string, isLatencyOrError: boolean = false) => {
    if (changeType === 'neutral') return 'text-gray-500';
    if (isLatencyOrError) {
      return changeType === 'decrease' ? 'text-green-500' : 'text-red-500';
    }
    return changeType === 'increase' ? 'text-green-500' : 'text-red-500';
  };

  const isSLAMet = (metric: typeof slaMetrics[0]) => {
    if (metric.inverse) {
      return metric.current <= metric.target;
    }
    return metric.current >= metric.target;
  };

  const getSLAProgress = (metric: typeof slaMetrics[0]) => {
    if (metric.inverse) {
      return Math.min(100, ((metric.target - metric.current) / metric.target) * 100 + 100);
    }
    return Math.min(100, (metric.current / metric.target) * 100);
  };

  return (
    <div className="space-y-6 p-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Performance Overview</h1>
          <p className="text-sm text-muted-foreground">
            High-level system metrics and SLA compliance
          </p>
        </div>
        <Select value={timeRange} onValueChange={setTimeRange}>
          <SelectTrigger className="w-[120px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="1h">Last 1h</SelectItem>
            <SelectItem value="6h">Last 6h</SelectItem>
            <SelectItem value="24h">Last 24h</SelectItem>
            <SelectItem value="7d">Last 7d</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {/* KPI Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        {kpiData.map((kpi) => (
          <Card key={kpi.label}>
            <CardContent className="p-4">
              <div className="flex items-start justify-between">
                <div>
                  <p className="text-sm text-muted-foreground">{kpi.label}</p>
                  <p className="text-2xl font-bold mt-1">{kpi.value}</p>
                  <div
                    className={`flex items-center gap-1 mt-1 text-sm ${getChangeColor(
                      kpi.changeType,
                      kpi.label.includes('Latency') || kpi.label.includes('Error')
                    )}`}
                  >
                    {getTrendIcon(kpi.changeType)}
                    <span>
                      {kpi.change > 0 ? '+' : ''}
                      {kpi.change}%
                    </span>
                  </div>
                </div>
                {kpi.icon}
              </div>
              <div className="mt-3 h-8">
                <ResponsiveContainer width="100%" height="100%">
                  <LineChart data={kpi.trend.map((v, i) => ({ value: v, index: i }))}>
                    <Line
                      type="monotone"
                      dataKey="value"
                      stroke="hsl(var(--primary))"
                      strokeWidth={2}
                      dot={false}
                    />
                  </LineChart>
                </ResponsiveContainer>
              </div>
            </CardContent>
          </Card>
        ))}
      </div>

      {/* Trend Charts */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Card>
          <CardHeader>
            <CardTitle className="text-lg">Request Volume</CardTitle>
            <CardDescription>Requests over time</CardDescription>
          </CardHeader>
          <CardContent>
            <ChartContainer config={chartConfig} className="h-[250px]">
              <AreaChart data={timeSeriesData}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis dataKey="time" tick={{ fontSize: 12 }} />
                <YAxis tick={{ fontSize: 12 }} />
                <ChartTooltip content={<ChartTooltipContent />} />
                <Area
                  type="monotone"
                  dataKey="requests"
                  stroke="var(--color-requests)"
                  fill="var(--color-requests)"
                  fillOpacity={0.3}
                />
              </AreaChart>
            </ChartContainer>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-lg">Latency Distribution</CardTitle>
            <CardDescription>Response time in milliseconds</CardDescription>
          </CardHeader>
          <CardContent>
            <ChartContainer config={chartConfig} className="h-[250px]">
              <LineChart data={timeSeriesData}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis dataKey="time" tick={{ fontSize: 12 }} />
                <YAxis tick={{ fontSize: 12 }} />
                <ChartTooltip content={<ChartTooltipContent />} />
                <Line
                  type="monotone"
                  dataKey="latency"
                  stroke="var(--color-latency)"
                  strokeWidth={2}
                  dot={false}
                />
              </LineChart>
            </ChartContainer>
          </CardContent>
        </Card>
      </div>

      {/* Error Rate Chart */}
      <Card>
        <CardHeader>
          <CardTitle className="text-lg">Error Rate</CardTitle>
          <CardDescription>Errors per time period</CardDescription>
        </CardHeader>
        <CardContent>
          <ChartContainer config={chartConfig} className="h-[200px]">
            <BarChart data={timeSeriesData}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis dataKey="time" tick={{ fontSize: 12 }} />
              <YAxis tick={{ fontSize: 12 }} />
              <ChartTooltip content={<ChartTooltipContent />} />
              <Bar dataKey="errors" fill="var(--color-errors)" radius={[4, 4, 0, 0]} />
            </BarChart>
          </ChartContainer>
        </CardContent>
      </Card>

      {/* SLA Compliance */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Target className="h-5 w-5" />
            SLA Compliance
          </CardTitle>
          <CardDescription>Service level agreement metrics</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="space-y-6">
            {slaMetrics.map((metric) => (
              <div key={metric.name} className="space-y-2">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <span className="font-medium">{metric.name}</span>
                    {isSLAMet(metric) ? (
                      <Badge variant="default" className="gap-1">
                        <CheckCircle2 className="h-3 w-3" />
                        Met
                      </Badge>
                    ) : (
                      <Badge variant="destructive" className="gap-1">
                        <AlertTriangle className="h-3 w-3" />
                        At Risk
                      </Badge>
                    )}
                  </div>
                  <div className="text-sm text-right">
                    <span className="font-bold">
                      {metric.current}
                      {metric.unit}
                    </span>
                    <span className="text-muted-foreground ml-2">
                      / {metric.target}
                      {metric.unit}
                    </span>
                  </div>
                </div>
                <Progress
                  value={getSLAProgress(metric)}
                  className={`h-2 ${isSLAMet(metric) ? '' : '[&>div]:bg-yellow-500'}`}
                />
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* Quick Stats Footer */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <Card>
          <CardContent className="p-4 text-center">
            <Zap className="h-6 w-6 mx-auto text-yellow-500" />
            <p className="text-sm text-muted-foreground mt-2">Peak RPS</p>
            <p className="text-xl font-bold">15,234</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4 text-center">
            <Activity className="h-6 w-6 mx-auto text-blue-500" />
            <p className="text-sm text-muted-foreground mt-2">Active Users</p>
            <p className="text-xl font-bold">1,847</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4 text-center">
            <Server className="h-6 w-6 mx-auto text-purple-500" />
            <p className="text-sm text-muted-foreground mt-2">Active Nodes</p>
            <p className="text-xl font-bold">12</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4 text-center">
            <CheckCircle2 className="h-6 w-6 mx-auto text-green-500" />
            <p className="text-sm text-muted-foreground mt-2">Health Score</p>
            <p className="text-xl font-bold">98%</p>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
