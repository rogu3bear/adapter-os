// AdapterActivations - Activations tab displaying activation history and trends
// Shows activation percentage over time with trend analysis

import React, { useMemo } from 'react';
import {
  Activity,
  ArrowDown,
  ArrowRight,
  ArrowUp,
  BarChart3,
  Clock,
  RefreshCw,
  TrendingDown,
  TrendingUp,
} from 'lucide-react';
import { format, parseISO, subDays, startOfDay } from 'date-fns';

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { EmptyState } from '@/components/ui/empty-state';
import { AdapterActivation } from '@/api/adapter-types';
import { formatPercent as formatPercentUtil } from '@/lib/formatters';

interface AdapterActivationsProps {
  adapterId: string;
  activations: AdapterActivation[] | null;
  isLoading: boolean;
  onRefresh: () => Promise<void>;
}

export default function AdapterActivations({
  adapterId,
  activations,
  isLoading,
  onRefresh,
}: AdapterActivationsProps) {
  // Process activation data for display
  const processedData = useMemo(() => {
    if (!activations || activations.length === 0) return null;

    // Find the most recent activation record for this adapter
    const adapterActivation = activations.find((a) => a.adapter_id === adapterId) || activations[0];

    // Calculate statistics from history
    const history = adapterActivation.history || [];
    const values = history.map((h) => h.value);

    const stats = {
      current: adapterActivation.activation_percent ?? 0,
      trend: adapterActivation.trend ?? 'stable',
      history: history,
      avg: values.length > 0 ? values.reduce((a, b) => a + b, 0) / values.length : 0,
      max: values.length > 0 ? Math.max(...values) : 0,
      min: values.length > 0 ? Math.min(...values) : 0,
    };

    return stats;
  }, [activations, adapterId]);

  // Get trend icon and color
  const getTrendInfo = (trend: string) => {
    switch (trend) {
      case 'increasing':
        return {
          icon: <TrendingUp className="h-4 w-4" />,
          color: 'text-success',
          bgColor: 'bg-success-surface',
          label: 'Increasing',
        };
      case 'decreasing':
        return {
          icon: <TrendingDown className="h-4 w-4" />,
          color: 'text-destructive',
          bgColor: 'bg-destructive-surface',
          label: 'Decreasing',
        };
      default:
        return {
          icon: <ArrowRight className="h-4 w-4" />,
          color: 'text-warning',
          bgColor: 'bg-warning-surface',
          label: 'Stable',
        };
    }
  };

  // Format percentage
  const formatPercent = (value: number): string => {
    return formatPercentUtil(value * 100, 2);
  };

  // Format timestamp
  const formatTime = (timestamp: string): string => {
    try {
      return format(parseISO(timestamp), 'MMM d, HH:mm');
    } catch {
      return timestamp;
    }
  };

  if (isLoading && !activations) {
    return <ActivationsSkeleton />;
  }

  if (!processedData) {
    return (
      <EmptyState
        icon={Activity}
        title="No activation data"
        description="Activation history will appear here once the adapter has been used for inference."
      />
    );
  }

  const trendInfo = getTrendInfo(processedData.trend);

  return (
    <div className="space-y-6">
      {/* Refresh Button */}
      <div className="flex justify-end">
        <Button variant="outline" size="sm" onClick={onRefresh} disabled={isLoading}>
          <RefreshCw className={`h-4 w-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
          Refresh
        </Button>
      </div>

      {/* Summary Cards */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center gap-2 text-muted-foreground mb-2">
              <Activity className="h-4 w-4" />
              <span className="text-sm">Current Activation</span>
              <GlossaryTooltip brief="Current activation percentage based on recent router decisions" />
            </div>
            <div className="text-2xl font-bold">{formatPercent(processedData.current)}</div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center gap-2 text-muted-foreground mb-2">
              <BarChart3 className="h-4 w-4" />
              <span className="text-sm">Trend</span>
              <GlossaryTooltip brief="Direction of activation changes over the past period" />
            </div>
            <div className={`flex items-center gap-2 ${trendInfo.color}`}>
              {trendInfo.icon}
              <span className="text-xl font-bold">{trendInfo.label}</span>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center gap-2 text-muted-foreground mb-2">
              <ArrowUp className="h-4 w-4" />
              <span className="text-sm">Peak Activation</span>
              <GlossaryTooltip brief="Highest activation percentage recorded" />
            </div>
            <div className="text-2xl font-bold">{formatPercent(processedData.max)}</div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center gap-2 text-muted-foreground mb-2">
              <ArrowDown className="h-4 w-4" />
              <span className="text-sm">Average</span>
              <GlossaryTooltip brief="Average activation percentage over the recorded period" />
            </div>
            <div className="text-2xl font-bold">{formatPercent(processedData.avg)}</div>
          </CardContent>
        </Card>
      </div>

      {/* Activation Chart Placeholder */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <BarChart3 className="h-5 w-5" />
            Activation History
            <GlossaryTooltip brief="Visual representation of activation percentage over time" />
          </CardTitle>
          <CardDescription>
            Activation percentage trend over the recorded period
          </CardDescription>
        </CardHeader>
        <CardContent>
          {processedData.history.length > 0 ? (
            <div className="space-y-4">
              {/* Simple bar chart visualization */}
              <div className="flex items-end gap-1 h-32">
                {processedData.history.slice(-20).map((point, idx) => {
                  const height = Math.max(4, point.value * 100);
                  return (
                    <div
                      key={idx}
                      className="flex-1 bg-primary/20 hover:bg-primary/40 transition-colors rounded-t relative group"
                      style={{ height: `${height}%` }}
                      title={`${formatTime(point.timestamp)}: ${formatPercent(point.value)}`}
                    >
                      <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1 hidden group-hover:block text-xs bg-popover border rounded px-2 py-1 whitespace-nowrap z-10">
                        {formatPercent(point.value)}
                        <br />
                        <span className="text-muted-foreground">{formatTime(point.timestamp)}</span>
                      </div>
                    </div>
                  );
                })}
              </div>
              <div className="flex justify-between text-xs text-muted-foreground">
                <span>
                  {processedData.history.length > 0 &&
                    formatTime(processedData.history[Math.max(0, processedData.history.length - 20)].timestamp)}
                </span>
                <span>
                  {processedData.history.length > 0 &&
                    formatTime(processedData.history[processedData.history.length - 1].timestamp)}
                </span>
              </div>
            </div>
          ) : (
            <div className="text-center py-8 text-muted-foreground">
              No historical data available
            </div>
          )}
        </CardContent>
      </Card>

      {/* Detailed History Table */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Clock className="h-5 w-5" />
            Activation Log
            <GlossaryTooltip brief="Detailed log of activation changes" />
          </CardTitle>
          <CardDescription>Recent activation percentage recordings</CardDescription>
        </CardHeader>
        <CardContent>
          {processedData.history.length > 0 ? (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Timestamp</TableHead>
                  <TableHead>Activation %</TableHead>
                  <TableHead>Change</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {processedData.history.slice(-10).reverse().map((point, idx, arr) => {
                  const prevValue = arr[idx + 1]?.value ?? point.value;
                  const change = point.value - prevValue;

                  return (
                    <TableRow key={idx}>
                      <TableCell className="text-muted-foreground">
                        {formatTime(point.timestamp)}
                      </TableCell>
                      <TableCell className="font-medium">
                        {formatPercent(point.value)}
                      </TableCell>
                      <TableCell>
                        {change !== 0 && (
                          <Badge
                            variant={change > 0 ? 'default' : 'secondary'}
                            className={change > 0 ? 'bg-success-surface text-success' : 'bg-destructive-surface text-destructive'}
                          >
                            {change > 0 ? '+' : ''}
                            {formatPercent(change)}
                          </Badge>
                        )}
                        {change === 0 && (
                          <span className="text-muted-foreground text-sm">No change</span>
                        )}
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          ) : (
            <div className="text-center py-8 text-muted-foreground">
              No activation logs recorded
            </div>
          )}
        </CardContent>
      </Card>

      {/* Activation Insights */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Activity className="h-5 w-5" />
            Insights
          </CardTitle>
          <CardDescription>Analysis and recommendations based on activation patterns</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            {processedData.current >= 0.7 && (
              <InsightItem
                type="info"
                message="High activation rate indicates this adapter is frequently selected by the router."
              />
            )}
            {processedData.current < 0.1 && processedData.current > 0 && (
              <InsightItem
                type="warning"
                message="Low activation rate may indicate the adapter is not well-suited for current queries."
              />
            )}
            {processedData.trend === 'decreasing' && (
              <InsightItem
                type="warning"
                message="Activation trend is decreasing. Consider reviewing adapter relevance or promoting if important."
              />
            )}
            {processedData.trend === 'increasing' && (
              <InsightItem
                type="success"
                message="Activation trend is increasing. The adapter is becoming more relevant to recent queries."
              />
            )}
            {processedData.history.length < 5 && (
              <InsightItem
                type="info"
                message="Limited history data available. More insights will appear as the adapter is used."
              />
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

// Insight item component
interface InsightItemProps {
  type: 'info' | 'warning' | 'success' | 'error';
  message: string;
}

function InsightItem({ type, message }: InsightItemProps) {
  const colors = {
    info: 'border-info/50 bg-info-surface',
    warning: 'border-warning/50 bg-warning-surface',
    success: 'border-success/50 bg-success-surface',
    error: 'border-destructive/50 bg-destructive-surface',
  };

  return (
    <div className={`p-3 rounded-md border ${colors[type]}`}>
      <p className="text-sm">{message}</p>
    </div>
  );
}

// Skeleton for loading state
function ActivationsSkeleton() {
  return (
    <div className="space-y-6">
      <div className="flex justify-end">
        <Skeleton className="h-9 w-24" />
      </div>
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        {[...Array(4)].map((_, i) => (
          <Card key={i}>
            <CardContent className="pt-6">
              <Skeleton className="h-4 w-24 mb-2" />
              <Skeleton className="h-8 w-16" />
            </CardContent>
          </Card>
        ))}
      </div>
      <Card>
        <CardHeader>
          <Skeleton className="h-6 w-48" />
          <Skeleton className="h-4 w-64" />
        </CardHeader>
        <CardContent>
          <Skeleton className="h-32 w-full" />
        </CardContent>
      </Card>
    </div>
  );
}
