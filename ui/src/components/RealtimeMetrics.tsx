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
  Play,
  Pause,
  RefreshCw
} from 'lucide-react';
import { SystemMetrics, User } from '../api/types';
import { LineChart, Line, AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';

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
  const [isPaused, setIsPaused] = useState(false);
  const [updateInterval, setUpdateInterval] = useState(100); // ms
  const intervalRef = useRef<NodeJS.Timeout | null>(null);
  const MAX_HISTORY = 60; // Keep last 60 data points (6 seconds at 100ms)
  
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
  
  const fetchMetrics = async () => {
    try {
      // Fetch system metrics
      const response = await fetch('/api/v1/metrics/system', {
        headers: {
          'Authorization': `Bearer ${localStorage.getItem('token')}`,
        },
      });
      
      if (response.ok) {
        const data: SystemMetrics = await response.json();
        setMetrics(data);
        
        // Add to history
        setHistory(prev => {
          const newHistory = [...prev, {
            timestamp: Date.now(),
            cpu: data.cpu_usage || 0,
            memory: data.memory_usage || 0,
            gpu: data.gpu_utilization || 0,
            tokensPerSec: data.tokens_per_second || 0,
            latency: data.avg_latency_ms || 0,
          }];
          
          // Keep only last MAX_HISTORY points
          if (newHistory.length > MAX_HISTORY) {
            return newHistory.slice(-MAX_HISTORY);
          }
          return newHistory;
        });
      }
      
      // Fetch training metrics (mock for now)
      setTrainingJobs({
        active: Math.floor(Math.random() * 5),
        completed: 42,
        pending: Math.floor(Math.random() * 10),
        total: 67,
      });
      
      // Fetch workload metrics
      setWorkload({
        activeWorkers: data?.active_workers || 0,
        queuedRequests: Math.floor(Math.random() * 20),
        throughput: data?.requests_per_second || 0,
        avgLatency: data?.avg_latency_ms || 0,
      });
      
      // Fetch import metrics (mock for now)
      setImports({
        reposScanning: Math.floor(Math.random() * 3),
        adaptersLoaded: data?.adapter_count || 0,
        modelsImported: 3,
        totalArtifacts: 156,
      });
      
    } catch (error) {
      console.error('Failed to fetch metrics:', error);
    }
  };
  
  useEffect(() => {
    // Initial fetch
    fetchMetrics();
    
    // Set up interval for real-time updates
    if (!isPaused) {
      intervalRef.current = setInterval(fetchMetrics, updateInterval);
    }
    
    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
      }
    };
  }, [isPaused, updateInterval, selectedTenant]);
  
  const togglePause = () => {
    setIsPaused(!isPaused);
  };
  
  const changeUpdateInterval = (newInterval: number) => {
    setUpdateInterval(newInterval);
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
    }
    if (!isPaused) {
      intervalRef.current = setInterval(fetchMetrics, newInterval);
    }
  };
  
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
      <div className="flex-between section-header">
        <div>
          <h1 className="section-title flex items-center gap-2">
            <Activity className="icon-standard" />
            Real-time Metrics
          </h1>
          <p className="section-description">
            System performance updated every {updateInterval}ms
          </p>
        </div>
        <div className="flex gap-2">
          <Button
            variant={updateInterval === 100 ? "default" : "outline"}
            size="sm"
            onClick={() => changeUpdateInterval(100)}
          >
            100ms
          </Button>
          <Button
            variant={updateInterval === 500 ? "default" : "outline"}
            size="sm"
            onClick={() => changeUpdateInterval(500)}
          >
            500ms
          </Button>
          <Button
            variant={updateInterval === 1000 ? "default" : "outline"}
            size="sm"
            onClick={() => changeUpdateInterval(1000)}
          >
            1s
          </Button>
          <Button variant="outline" size="sm" onClick={togglePause}>
            {isPaused ? (
              <><Play className="icon-small mr-2" /> Resume</>
            ) : (
              <><Pause className="icon-small mr-2" /> Pause</>
            )}
          </Button>
        </div>
      </div>
      
      {/* System Resources */}
      <div className="grid grid-cols-4 gap-4">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium flex items-center gap-2">
              <Cpu className="icon-small text-blue-500" />
              CPU Usage
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {metrics?.cpu_usage?.toFixed(1) || 0}%
            </div>
            <Progress value={metrics?.cpu_usage || 0} className="mt-2" />
            <p className="text-xs text-muted-foreground mt-1">
              Load: {metrics?.load_average?.load_1min?.toFixed(2) || 0}
            </p>
          </CardContent>
        </Card>
        
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium flex items-center gap-2">
              <HardDrive className="icon-small text-green-500" />
              Memory
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {metrics?.memory_usage?.toFixed(1) || 0}%
            </div>
            <Progress value={metrics?.memory_usage || 0} className="mt-2" />
            <p className="text-xs text-muted-foreground mt-1">
              {((metrics?.memory_usage || 0) * 32 / 100).toFixed(1)}GB / 32GB
            </p>
          </CardContent>
        </Card>
        
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium flex items-center gap-2">
              <Zap className="icon-small text-purple-500" />
              GPU
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {metrics?.gpu_utilization?.toFixed(1) || 0}%
            </div>
            <Progress value={metrics?.gpu_utilization || 0} className="mt-2" />
            <p className="text-xs text-muted-foreground mt-1">
              M3 Max
            </p>
          </CardContent>
        </Card>
        
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium flex items-center gap-2">
              <Clock className="icon-small text-orange-500" />
              Latency
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {metrics?.avg_latency_ms?.toFixed(0) || 0}ms
            </div>
            <Progress value={Math.min(100, (metrics?.avg_latency_ms || 0) / 5)} className="mt-2" />
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
                <Area type="monotone" dataKey="CPU" stackId="1" stroke="#3b82f6" fill="#3b82f6" fillOpacity={0.6} />
                <Area type="monotone" dataKey="Memory" stackId="1" stroke="#10b981" fill="#10b981" fillOpacity={0.6} />
                <Area type="monotone" dataKey="GPU" stackId="1" stroke="#8b5cf6" fill="#8b5cf6" fillOpacity={0.6} />
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
                <Line yAxisId="left" type="monotone" dataKey="Tokens/s" stroke="#f59e0b" strokeWidth={2} dot={false} />
                <Line yAxisId="right" type="monotone" dataKey="Latency" stroke="#ef4444" strokeWidth={2} dot={false} />
              </LineChart>
            </ResponsiveContainer>
          </CardContent>
        </Card>
      </div>
      
      {/* Training Metrics */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <TrendingUp className="icon-small" />
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
            <Activity className="icon-small" />
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
            <Database className="icon-small" />
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
                  <RefreshCw className="icon-small animate-spin text-blue-500" />
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
              <span className="font-mono">{Math.floor((metrics?.uptime_seconds || 0) / 3600)}h {Math.floor(((metrics?.uptime_seconds || 0) % 3600) / 60)}m</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Process Count</span>
              <span className="font-mono">{metrics?.process_count || 0}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Network Bandwidth</span>
              <span className="font-mono">{((metrics?.network_bandwidth || 0) / 1024 / 1024).toFixed(1)} MB/s</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Disk Usage</span>
              <span className="font-mono">{metrics?.disk_usage?.toFixed(1) || 0}%</span>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

