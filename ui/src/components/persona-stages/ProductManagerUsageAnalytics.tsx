import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Progress } from '@/components/ui/progress';
import ResponsiveStatGrid from '@/components/ui/responsive-stat-grid';
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  ChartConfig,
} from '@/components/ui/chart';
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  BarChart,
  Bar,
  PieChart,
  Pie,
  Cell,
  ResponsiveContainer,
} from 'recharts';
import {
  BarChart3,
  Layers,
  Users,
  Coins,
  TrendingUp,
  Calendar,
  Clock,
} from 'lucide-react';

interface AdapterUsage {
  id: string;
  name: string;
  tenant: string;
  requests: number;
  tokens: number;
  avgLatency: number;
  errorRate: number;
}

interface HeatmapCell {
  day: string;
  hour: number;
  value: number;
}

const requestVolumeData = [
  { date: 'Jan 14', requests: 12500, tokens: 2500000 },
  { date: 'Jan 15', requests: 14200, tokens: 2840000 },
  { date: 'Jan 16', requests: 13800, tokens: 2760000 },
  { date: 'Jan 17', requests: 15600, tokens: 3120000 },
  { date: 'Jan 18', requests: 16800, tokens: 3360000 },
  { date: 'Jan 19', requests: 11200, tokens: 2240000 },
  { date: 'Jan 20', requests: 10800, tokens: 2160000 },
];

const topAdapters: AdapterUsage[] = [
  {
    id: 'adapter-1',
    name: 'code-review/r003',
    tenant: 'engineering',
    requests: 45200,
    tokens: 9040000,
    avgLatency: 85,
    errorRate: 0.08,
  },
  {
    id: 'adapter-2',
    name: 'docs-assistant/r002',
    tenant: 'product',
    requests: 38500,
    tokens: 7700000,
    avgLatency: 92,
    errorRate: 0.12,
  },
  {
    id: 'adapter-3',
    name: 'data-analysis/r001',
    tenant: 'analytics',
    requests: 32100,
    tokens: 6420000,
    avgLatency: 110,
    errorRate: 0.05,
  },
  {
    id: 'adapter-4',
    name: 'customer-support/r004',
    tenant: 'support',
    requests: 28900,
    tokens: 5780000,
    avgLatency: 78,
    errorRate: 0.15,
  },
  {
    id: 'adapter-5',
    name: 'translation/r002',
    tenant: 'localization',
    requests: 21500,
    tokens: 4300000,
    avgLatency: 65,
    errorRate: 0.03,
  },
];

const tokenBreakdown = [
  { name: 'Input Tokens', value: 18500000, color: 'hsl(var(--chart-1))' },
  { name: 'Output Tokens', value: 8200000, color: 'hsl(var(--chart-2))' },
  { name: 'System Tokens', value: 1300000, color: 'hsl(var(--chart-3))' },
];

const generateHeatmapData = (): HeatmapCell[] => {
  const days = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  const data: HeatmapCell[] = [];

  days.forEach((day) => {
    for (let hour = 0; hour < 24; hour++) {
      let value = Math.random() * 100;
      // Simulate realistic patterns
      if (day === 'Sat' || day === 'Sun') {
        value *= 0.3;
      } else if (hour >= 9 && hour <= 17) {
        value *= 1.5;
      } else if (hour >= 0 && hour <= 6) {
        value *= 0.2;
      }
      data.push({ day, hour, value: Math.floor(value) });
    }
  });

  return data;
};

const heatmapData = generateHeatmapData();

const chartConfig: ChartConfig = {
  requests: {
    label: 'Requests',
    color: 'hsl(var(--chart-1))',
  },
  tokens: {
    label: 'Tokens',
    color: 'hsl(var(--chart-2))',
  },
};

export default function ProductManagerUsageAnalytics() {
  const [timeRange, setTimeRange] = useState('7d');
  const [viewMode, setViewMode] = useState('requests');

  const totalRequests = requestVolumeData.reduce((sum, d) => sum + d.requests, 0);
  const totalTokens = requestVolumeData.reduce((sum, d) => sum + d.tokens, 0);
  const avgDailyRequests = Math.round(totalRequests / requestVolumeData.length);

  const getHeatmapColor = (value: number) => {
    if (value < 20) return 'bg-muted';
    if (value < 40) return 'bg-blue-200 dark:bg-blue-900';
    if (value < 60) return 'bg-blue-300 dark:bg-blue-800';
    if (value < 80) return 'bg-blue-400 dark:bg-blue-700';
    return 'bg-blue-500 dark:bg-blue-600';
  };

  const formatNumber = (num: number) => {
    if (num >= 1000000) return `${(num / 1000000).toFixed(1)}M`;
    if (num >= 1000) return `${(num / 1000).toFixed(1)}K`;
    return num.toString();
  };

  return (
    <div className="space-y-6 p-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Usage Analytics</h1>
          <p className="text-sm text-muted-foreground">
            Detailed usage metrics and patterns
          </p>
        </div>
        <Select value={timeRange} onValueChange={setTimeRange}>
          <SelectTrigger className="w-[120px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="24h">Last 24h</SelectItem>
            <SelectItem value="7d">Last 7d</SelectItem>
            <SelectItem value="30d">Last 30d</SelectItem>
            <SelectItem value="90d">Last 90d</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {/* Summary Cards */}
      <ResponsiveStatGrid>
        <Card>
          <CardContent className="p-4">
            <div className="flex items-center gap-3">
              <BarChart3 className="h-8 w-8 text-blue-500" />
              <div>
                <p className="text-sm text-muted-foreground">Total Requests</p>
                <p className="text-2xl font-bold">{formatNumber(totalRequests)}</p>
              </div>
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4">
            <div className="flex items-center gap-3">
              <Coins className="h-8 w-8 text-yellow-500" />
              <div>
                <p className="text-sm text-muted-foreground">Total Tokens</p>
                <p className="text-2xl font-bold">{formatNumber(totalTokens)}</p>
              </div>
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4">
            <div className="flex items-center gap-3">
              <TrendingUp className="h-8 w-8 text-green-500" />
              <div>
                <p className="text-sm text-muted-foreground">Avg Daily</p>
                <p className="text-2xl font-bold">{formatNumber(avgDailyRequests)}</p>
              </div>
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4">
            <div className="flex items-center gap-3">
              <Layers className="h-8 w-8 text-purple-500" />
              <div>
                <p className="text-sm text-muted-foreground">Active Adapters</p>
                <p className="text-2xl font-bold">{topAdapters.length}</p>
              </div>
            </div>
          </CardContent>
        </Card>
      </ResponsiveStatGrid>

      {/* Request Volume Chart */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle>Request Volume</CardTitle>
              <CardDescription>Requests and token usage over time</CardDescription>
            </div>
            <Tabs value={viewMode} onValueChange={setViewMode}>
              <TabsList>
                <TabsTrigger value="requests">Requests</TabsTrigger>
                <TabsTrigger value="tokens">Tokens</TabsTrigger>
              </TabsList>
            </Tabs>
          </div>
        </CardHeader>
        <CardContent>
          <ChartContainer config={chartConfig} className="h-[300px]">
            <AreaChart data={requestVolumeData}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis dataKey="date" tick={{ fontSize: 12 }} />
              <YAxis tick={{ fontSize: 12 }} tickFormatter={formatNumber} />
              <ChartTooltip content={<ChartTooltipContent />} />
              {viewMode === 'requests' ? (
                <Area
                  type="monotone"
                  dataKey="requests"
                  stroke="var(--color-requests)"
                  fill="var(--color-requests)"
                  fillOpacity={0.3}
                />
              ) : (
                <Area
                  type="monotone"
                  dataKey="tokens"
                  stroke="var(--color-tokens)"
                  fill="var(--color-tokens)"
                  fillOpacity={0.3}
                />
              )}
            </AreaChart>
          </ChartContainer>
        </CardContent>
      </Card>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        {/* Top Adapters Table */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Layers className="h-5 w-5" />
              Top Adapters
            </CardTitle>
            <CardDescription>Most used adapters by request count</CardDescription>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Adapter</TableHead>
                  <TableHead className="text-right">Requests</TableHead>
                  <TableHead className="text-right">Latency</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {topAdapters.map((adapter, index) => (
                  <TableRow key={adapter.id}>
                    <TableCell>
                      <div>
                        <div className="font-medium text-sm">
                          #{index + 1} {adapter.name}
                        </div>
                        <div className="text-xs text-muted-foreground">
                          {adapter.tenant}
                        </div>
                      </div>
                    </TableCell>
                    <TableCell className="text-right">
                      <div className="text-sm font-medium">
                        {formatNumber(adapter.requests)}
                      </div>
                      <div className="text-xs text-muted-foreground">
                        {formatNumber(adapter.tokens)} tokens
                      </div>
                    </TableCell>
                    <TableCell className="text-right">
                      <div className="text-sm">{adapter.avgLatency}ms</div>
                      <Badge
                        variant={adapter.errorRate > 0.1 ? 'destructive' : 'secondary'}
                        className="text-xs"
                      >
                        {adapter.errorRate}% err
                      </Badge>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>

        {/* Token Usage Breakdown */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Coins className="h-5 w-5" />
              Token Breakdown
            </CardTitle>
            <CardDescription>Distribution of token types</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="h-[200px]">
              <ResponsiveContainer width="100%" height="100%">
                <PieChart>
                  <Pie
                    data={tokenBreakdown}
                    cx="50%"
                    cy="50%"
                    innerRadius={60}
                    outerRadius={80}
                    paddingAngle={5}
                    dataKey="value"
                  >
                    {tokenBreakdown.map((entry, index) => (
                      <Cell key={`cell-${index}`} fill={entry.color} />
                    ))}
                  </Pie>
                </PieChart>
              </ResponsiveContainer>
            </div>
            <div className="space-y-3 mt-4">
              {tokenBreakdown.map((item) => (
                <div key={item.name} className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <div
                      className="w-3 h-3 rounded-full"
                      style={{ backgroundColor: item.color }}
                    />
                    <span className="text-sm">{item.name}</span>
                  </div>
                  <span className="text-sm font-medium">{formatNumber(item.value)}</span>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      </div>

      {/* User Activity Heatmap */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Users className="h-5 w-5" />
            User Activity Heatmap
          </CardTitle>
          <CardDescription>Request distribution by day and hour</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="overflow-x-auto">
            <div className="w-full">
              {/* Hour labels */}
              <div className="flex mb-2 ml-12">
                {Array.from({ length: 24 }, (_, i) => (
                  <div
                    key={i}
                    className="flex-1 text-xs text-muted-foreground text-center"
                  >
                    {i % 3 === 0 ? `${i}:00` : ''}
                  </div>
                ))}
              </div>

              {/* Heatmap grid */}
              {['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'].map((day) => (
                <div key={day} className="flex items-center mb-1">
                  <div className="w-12 text-xs text-muted-foreground">{day}</div>
                  <div className="flex flex-1 gap-0.5">
                    {heatmapData
                      .filter((cell) => cell.day === day)
                      .map((cell) => (
                        <div
                          key={`${cell.day}-${cell.hour}`}
                          className={`flex-1 h-4 rounded-sm ${getHeatmapColor(cell.value)}`}
                          title={`${cell.day} ${cell.hour}:00 - ${cell.value} requests`}
                        />
                      ))}
                  </div>
                </div>
              ))}

              {/* Legend */}
              <div className="flex items-center justify-end gap-2 mt-4">
                <span className="text-xs text-muted-foreground">Less</span>
                <div className="flex gap-1">
                  <div className="w-4 h-4 rounded-sm bg-muted" />
                  <div className="w-4 h-4 rounded-sm bg-blue-200 dark:bg-blue-900" />
                  <div className="w-4 h-4 rounded-sm bg-blue-300 dark:bg-blue-800" />
                  <div className="w-4 h-4 rounded-sm bg-blue-400 dark:bg-blue-700" />
                  <div className="w-4 h-4 rounded-sm bg-blue-500 dark:bg-blue-600" />
                </div>
                <span className="text-xs text-muted-foreground">More</span>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Usage by Tenant */}
      <Card>
        <CardHeader>
          <CardTitle>Usage by Tenant</CardTitle>
          <CardDescription>Request distribution across tenants</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            {['engineering', 'product', 'analytics', 'support', 'localization'].map(
              (tenant, index) => {
                const tenantRequests = topAdapters
                  .filter((a) => a.tenant === tenant)
                  .reduce((sum, a) => sum + a.requests, 0);
                const percentage = Math.round((tenantRequests / totalRequests) * 100);

                return (
                  <div key={tenant} className="space-y-2">
                    <div className="flex items-center justify-between">
                      <span className="text-sm font-medium capitalize">{tenant}</span>
                      <span className="text-sm text-muted-foreground">
                        {formatNumber(tenantRequests)} ({percentage}%)
                      </span>
                    </div>
                    <Progress value={percentage} className="h-2" />
                  </div>
                );
              }
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
