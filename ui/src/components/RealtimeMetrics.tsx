// 【ui/src/components/RealtimeMetrics.tsx§1-25】 - Replace manual SSE+polling with standardized hook
import React, { useState, useEffect, useRef } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Badge } from './ui/badge';
import { Button } from './ui/button';
import { Progress } from './ui/progress';
import {
  Activity,
  Cpu,
  HardDrive,
  Zap,
  Clock,
  TrendingUp,
  Database,
  GitBranch,
  RefreshCw
} from 'lucide-react';
import { SystemMetrics, User } from '@/api/types';
import { LineChart, Line, AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';
import { toast } from 'sonner';
import { apiClient } from '@/api/services';
import { logger, toError } from '@/utils/logger';
import { usePolling } from '@/hooks/realtime/usePolling';
import { LastUpdated } from './ui/last-updated';
import { METRIC_COLORS } from '@/constants/chart-colors';

interface RealtimeMetricsProps {
  user: User;
  selectedTenant: string;
}

interface MetricsHistory {
  timestamp: number;
  cpu: number;
  memory: number;
  gpu: number;
  tokensPerSec: number;
  latency: number;
}

export function RealtimeMetrics({ user, selectedTenant }: RealtimeMetricsProps) {
  const [metrics, setMetrics] = useState<SystemMetrics | null>(null);
  const [history, setHistory] = useState<MetricsHistory[]>([]);

  const MAX_HISTORY = 120; // Keep last 120 data points

  // 【ui/src/hooks/usePolling.ts】 - Standardized polling hook for real-time metrics
  const fetchMetricsData = async () => {
    const data = await apiClient.getSystemMetrics();
    return data;
  };

  const {
    data: polledMetrics,
    isLoading,
    lastUpdated,
    error: pollingError,
    refetch: refreshMetrics
  } = usePolling(
    fetchMetricsData,
    'fast', // Real-time updates for metrics
    {
      showLoadingIndicator: false,
      onError: (err) => {
        logger.error('Metrics fetch failed', {
          component: 'RealtimeMetrics',
          operation: 'polling',
          tenantId: selectedTenant,
        }, err);
      }
    }
  );

  // Training metrics
  const [trainingJobs, setTrainingJobs] = useState({
    active: 0,
    completed: 0,
    pending: 0,
    total: 0,
  });

  // Workload metrics
  const [workload, setWorkload] = useState({
    activeWorkers: 0,
    queuedRequests: 0,
    throughput: 0,
    avgLatency: 0,
  });

  // Import metrics
  const [imports, setImports] = useState({
    reposScanning: 0,
    adaptersLoaded: 0,
    modelsImported: 0,
    totalArtifacts: 0,
  });

  // Update metrics and derived state when polling data arrives
  useEffect(() => {
    if (polledMetrics) {
      setMetrics(polledMetrics);

      // Add to history
      setHistory(prev => {
        const newHistory = [...prev, {
          timestamp: Date.now(),
          cpu: polledMetrics.cpu_usage_percent || 0,
          memory: polledMetrics.memory_usage_pct || 0,
          gpu: polledMetrics.gpu_utilization_percent || 0,
          tokensPerSec: polledMetrics.tokens_per_second || 0,
          latency: polledMetrics.latency_p95_ms || 0,
        }];

        // Keep only last MAX_HISTORY points
        if (newHistory.length > MAX_HISTORY) {
          return newHistory.slice(-MAX_HISTORY);
        }
        return newHistory;
      });

      // Update training metrics (mock for now)
      setTrainingJobs({
        active: Math.floor(Math.random() * 5),
        completed: 42,
        pending: Math.floor(Math.random() * 10),
        total: 67,
      });

      // Update workload metrics
      setWorkload({
        activeWorkers: polledMetrics?.active_sessions || 0,
        queuedRequests: Math.floor(Math.random() * 20),
        throughput: polledMetrics?.tokens_per_second || 0,
        avgLatency: polledMetrics?.latency_p95_ms || 0,
      });

      // Update import metrics (mock for now)
      setImports({
        reposScanning: Math.floor(Math.random() * 3),
        adaptersLoaded: polledMetrics?.adapter_count || 0,
        modelsImported: 3,
        totalArtifacts: 156,
      });
    }
  }, [polledMetrics]);
  
  // Format chart data
  const chartData = history.map((h, idx) => ({
    time: idx,
    CPU: h.cpu,
    Memory: h.memory,
    GPU: h.gpu,
  }));
  
  const throughputData = history.map((h, idx) => ({
    time: idx,
    'Tokens/s': h.tokensPerSec,
    'Latency': h.latency,
  }));
  
  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold flex items-center gap-2">
            <Activity className="h-5 w-5" />
            Real-time Metrics
          </h1>
          <p className="text-sm text-muted-foreground">
            System performance with real-time updates
          </p>
          {lastUpdated && <LastUpdated timestamp={lastUpdated} className="mt-1" />}
        </div>

        <Button onClick={() => refreshMetrics()} disabled={isLoading} variant="outline" size="sm">
          <RefreshCw className={`w-4 h-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
          Refresh
        </Button>
      </div>
      
      {/* System Resources */}
      <div className="grid grid-cols-4 gap-4">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium flex items-center gap-2">
              <Cpu className="h-3 w-3 text-blue-500" />
              CPU Usage
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {metrics?.cpu_usage_percent?.toFixed(1) || 0}%
            </div>
            <Progress value={Math.min(100, metrics?.cpu_usage_percent || 0)} className="mt-2" />
            <p className="text-xs text-muted-foreground mt-1">
              Cores: {metrics?.cpu_cores || 'N/A'}
            </p>
          </CardContent>
        </Card>
        
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium flex items-center gap-2">
              <HardDrive className="h-3 w-3 text-green-500" />
              Memory
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {metrics?.memory_usage_pct?.toFixed(1) || 0}%
            </div>
            <Progress value={metrics?.memory_usage_pct || 0} className="mt-2" />
            <p className="text-xs text-muted-foreground mt-1">
              {metrics?.memory_used_gb?.toFixed(1) || '0'}GB / {metrics?.memory_total_gb?.toFixed(1) || '0'}GB
            </p>
          </CardContent>
        </Card>
        
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium flex items-center gap-2">
              <Zap className="h-3 w-3 text-purple-500" />
              GPU
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {metrics?.gpu_utilization_percent?.toFixed(1) || 0}%
            </div>
            <Progress value={metrics?.gpu_utilization_percent || 0} className="mt-2" />
            <p className="text-xs text-muted-foreground mt-1">
              M3 Max
            </p>
          </CardContent>
        </Card>
        
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium flex items-center gap-2">
              <Clock className="h-3 w-3 text-orange-500" />
              Latency
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {metrics?.latency_p95_ms?.toFixed(0) || 0}ms
            </div>
            <Progress value={Math.min(100, (metrics?.latency_p95_ms || 0) / 5)} className="mt-2" />
            <p className="text-xs text-muted-foreground mt-1">
              p95: {metrics?.latency_p95_ms?.toFixed(0) || 0}ms
            </p>
          </CardContent>
        </Card>
      </div>
      
      {/* System Charts */}
      <div className="grid grid-cols-2 gap-4">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Resource Usage</CardTitle>
          </CardHeader>
          <CardContent>
            <ResponsiveContainer width="100%" height={200}>
              <AreaChart data={chartData}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis dataKey="time" hide />
                <YAxis domain={[0, 100]} />
                <Tooltip />
                <Area type="monotone" dataKey="CPU" stackId="1" stroke={METRIC_COLORS.cpu} fill={METRIC_COLORS.cpu} fillOpacity={0.6} />
                <Area type="monotone" dataKey="Memory" stackId="1" stroke={METRIC_COLORS.memory} fill={METRIC_COLORS.memory} fillOpacity={0.6} />
                <Area type="monotone" dataKey="GPU" stackId="1" stroke={METRIC_COLORS.gpu} fill={METRIC_COLORS.gpu} fillOpacity={0.6} />
              </AreaChart>
            </ResponsiveContainer>
          </CardContent>
        </Card>
        
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Throughput & Latency</CardTitle>
          </CardHeader>
          <CardContent>
            <ResponsiveContainer width="100%" height={200}>
              <LineChart data={throughputData}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis dataKey="time" hide />
                <YAxis yAxisId="left" />
                <YAxis yAxisId="right" orientation="right" />
                <Tooltip />
                <Line yAxisId="left" type="monotone" dataKey="Tokens/s" stroke={METRIC_COLORS.tokensPerSecond} strokeWidth={2} dot={false} />
                <Line yAxisId="right" type="monotone" dataKey="Latency" stroke={METRIC_COLORS.latency} strokeWidth={2} dot={false} />
              </LineChart>
            </ResponsiveContainer>
          </CardContent>
        </Card>
      </div>
      
      {/* Training Metrics */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <TrendingUp className="h-3 w-3" />
            Training Jobs
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-4 gap-4">
            <div>
              <p className="text-sm text-muted-foreground">Active</p>
              <p className="text-2xl font-bold text-green-500">{trainingJobs.active}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Pending</p>
              <p className="text-2xl font-bold text-yellow-500">{trainingJobs.pending}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Completed</p>
              <p className="text-2xl font-bold text-blue-500">{trainingJobs.completed}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Total</p>
              <p className="text-2xl font-bold">{trainingJobs.total}</p>
            </div>
          </div>
        </CardContent>
      </Card>
      
      {/* Workload Metrics */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Activity className="h-3 w-3" />
            Workload
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-4 gap-4">
            <div>
              <p className="text-sm text-muted-foreground">Active Workers</p>
              <p className="text-2xl font-bold">{workload.activeWorkers}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Queued Requests</p>
              <p className="text-2xl font-bold">{workload.queuedRequests}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Throughput</p>
              <p className="text-2xl font-bold">{workload.throughput.toFixed(1)}<span className="text-sm"> req/s</span></p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Avg Latency</p>
              <p className="text-2xl font-bold">{workload.avgLatency.toFixed(0)}<span className="text-sm"> ms</span></p>
            </div>
          </div>
        </CardContent>
      </Card>
      
      {/* Import Metrics */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Database className="h-3 w-3" />
            Imports & Artifacts
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-4 gap-4">
            <div>
              <p className="text-sm text-muted-foreground">Repos Scanning</p>
              <p className="text-2xl font-bold flex items-center gap-2">
                {imports.reposScanning}
                {imports.reposScanning > 0 && (
                  <RefreshCw className="h-3 w-3 animate-spin text-blue-500" />
                )}
              </p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Adapters Loaded</p>
              <p className="text-2xl font-bold">{imports.adaptersLoaded}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Models Imported</p>
              <p className="text-2xl font-bold">{imports.modelsImported}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Total Artifacts</p>
              <p className="text-2xl font-bold">{imports.totalArtifacts}</p>
            </div>
          </div>
        </CardContent>
      </Card>
      
      {/* Live Stats */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">System Status</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div className="flex justify-between">
              <span className="text-muted-foreground">Uptime</span>
              <span className="font-mono">N/A</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Active Sessions</span>
              <span className="font-mono">{metrics?.active_sessions || 0}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Network RX</span>
              <span className="font-mono">{((metrics?.network_rx_bytes || 0) / 1024 / 1024).toFixed(1)} MB</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Disk Usage</span>
              <span className="font-mono">{metrics?.disk_usage_percent?.toFixed(1) || 0}%</span>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
