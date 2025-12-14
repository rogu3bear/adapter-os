// 【ui/src/components/ResourceMonitor.tsx§94-178】 - Replace manual polling with standardized hook
import React, { useState, useEffect, useRef } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Progress } from './ui/progress';
import { Alert, AlertDescription } from './ui/alert';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import {
  Cpu,
  MemoryStick,
  HardDrive,
  Zap,
  Activity,
  AlertTriangle,
  CheckCircle,
  TrendingUp,
  TrendingDown,
  BarChart3,
  Thermometer,
  Wifi,
  Database,
  Monitor,
  Server
} from 'lucide-react';
import apiClient from '@/api/client';
import { logger, toError } from '@/utils/logger';
import { usePolling } from '@/hooks/realtime/usePolling';
import { LastUpdated } from './ui/last-updated';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { formatBytes, formatNumber } from '@/utils/format';

interface ResourceMonitorProps {
  jobId?: string;
  nodeId?: string;
}

interface ResourceMetrics {
  timestamp: string;
  cpu: {
    usage: number;
    cores: number;
    temperature?: number;
  };
  memory: {
    used: number;
    total: number;
    usage_percent: number;
  };
  gpu: {
    utilization: number;
    memory_used: number;
    memory_total: number;
    temperature?: number;
    power_draw?: number;
  };
  disk: {
    used: number;
    total: number;
    usage_percent: number;
    io_read: number;
    io_write: number;
  };
  network: {
    bytes_in: number;
    bytes_out: number;
    packets_in: number;
    packets_out: number;
  };
  training?: {
    tokens_per_second: number;
    loss: number;
    learning_rate: number;
    current_epoch: number;
    total_epochs: number;
  };
}

interface NodeInfo {
  id: string;
  hostname: string;
  metal_family: string;
  memory_gb: number;
  gpu_count: number;
  gpu_type: string;
  status: string;
  last_heartbeat: string;
}

export function ResourceMonitor({ jobId, nodeId }: ResourceMonitorProps) {
  const [metrics, setMetrics] = useState<ResourceMetrics[]>([]);
  const [nodeInfo, setNodeInfo] = useState<NodeInfo | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [isMonitoring, setIsMonitoring] = useState(true); // Always monitoring with usePolling hook

  // Fetch node info once on mount
  useEffect(() => {
    const fetchNodeInfo = async () => {
      if (!nodeId) return;
      try {
        const node = await apiClient.getNodeDetails(nodeId);
        setNodeInfo({
          id: node.id,
          hostname: node.hostname || nodeId,
          metal_family: node.metal_family || 'Unknown',
          memory_gb: node.memory_gb || 0,
          gpu_count: node.gpu_count || 0,
          gpu_type: node.gpu_type || 'Unknown',
          status: node.status,
          last_heartbeat: node.last_heartbeat || new Date().toISOString()
        });
      } catch (err) {
        logger.error('Failed to fetch node info', {
          component: 'ResourceMonitor',
          operation: 'fetchNodeInfo',
          nodeId
        }, toError(err));
      }
    };
    fetchNodeInfo();
  }, [nodeId]);

  // 【ui/src/hooks/usePolling.ts】 - Standardized polling hook for resource metrics
  const fetchResourceMetrics = async () => {
    const systemMetrics = await apiClient.getSystemMetrics();


    // Convert system metrics to resource metrics format
    const newMetrics: ResourceMetrics = {
      timestamp: new Date().toISOString(),
      cpu: {
        usage: systemMetrics.cpu_usage_percent || 0,
        cores: systemMetrics.cpu_cores || 0,
        temperature: systemMetrics.cpu_temp_celsius || 0
      },
      memory: {
        used: systemMetrics.memory_used_gb || 0,
        total: systemMetrics.memory_total_gb || 0,
        usage_percent: systemMetrics.memory_usage_percent || 0
      },
      gpu: {
        utilization: systemMetrics.gpu_utilization_percent || 0,
        memory_used: systemMetrics.gpu_memory_used_mb || 0,
        memory_total: systemMetrics.gpu_memory_total_mb || 0,
        temperature: systemMetrics.gpu_temp_celsius || 0,
        power_draw: systemMetrics.gpu_power_watts || 0
      },
      disk: {
        used: systemMetrics.disk_used_gb || 0,
        total: systemMetrics.disk_total_gb || 0,
        usage_percent: systemMetrics.disk_usage_percent || 0,
        io_read: systemMetrics.disk_read_mbps || 0,
        io_write: systemMetrics.disk_write_mbps || 0
      },
      network: {
        bytes_in: systemMetrics.network_rx_bytes || 0,
        bytes_out: systemMetrics.network_tx_bytes || 0,
        packets_in: systemMetrics.network_rx_packets || 0,
        packets_out: systemMetrics.network_tx_packets || 0
      },
      training: jobId ? {
        tokens_per_second: systemMetrics.tokens_per_second || 0,
        loss: systemMetrics.current_loss || 0,
        learning_rate: systemMetrics.learning_rate || 0,
        current_epoch: systemMetrics.current_epoch || 0,
        total_epochs: systemMetrics.total_epochs || 0
      } : undefined
    };

    return newMetrics;
  };

  const {
    data: polledMetrics,
    isLoading: loading,
    lastUpdated,
    error: pollingError,
    refetch: refreshMetrics
  } = usePolling(
    fetchResourceMetrics,
    'fast', // Real-time updates for resource monitoring
    {
      showLoadingIndicator: true,
      onError: (err) => {
        const error = err instanceof Error ? err : new Error('Failed to fetch resource metrics');
        setError(error);
        logger.error('Failed to fetch resource metrics', {
          component: 'ResourceMonitor',
          operation: 'polling',
          jobId,
          nodeId
        }, err);
      }
    }
  );

  // Update metrics when polling data arrives
  useEffect(() => {
    if (!polledMetrics) return;
    setMetrics(prev => [...prev.slice(-59), polledMetrics]); // Keep last 60 data points
    setError(null);
  }, [polledMetrics]);

  const getStatusColor = (usage: number) => {
    if (usage > 90) return 'text-red-600';
    if (usage > 75) return 'text-yellow-600';
    return 'text-green-600';
  };

  const getStatusIcon = (usage: number) => {
    if (usage > 90) return <AlertTriangle className="h-4 w-4 text-red-600" />;
    if (usage > 75) return <AlertTriangle className="h-4 w-4 text-yellow-600" />;
    return <CheckCircle className="h-4 w-4 text-green-600" />;
  };

  if (loading) {
    return <div className="text-center p-8">Loading resource metrics...</div>;
  }

  if (error) {
    return errorRecoveryTemplates.genericError(
      error.message,
      () => refreshMetrics()
    );
  }

  const currentMetrics = metrics[metrics.length - 1];
  if (!currentMetrics || !nodeInfo) {
    return <div className="text-center p-8">No metrics available</div>;
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-semibold">Resource Monitor</h2>
          <p className="text-muted-foreground">
            {nodeInfo.hostname} • {nodeInfo.metal_family} • {nodeInfo.gpu_count} GPUs
          </p>
          {lastUpdated && <LastUpdated timestamp={lastUpdated} className="mt-1" />}
        </div>
        <div className="flex items-center space-x-2">
          <Badge variant={isMonitoring ? "default" : "outline"}>
            {isMonitoring ? "Monitoring" : "Paused"}
          </Badge>
          <Button 
            variant="outline" 
            size="sm" 
            onClick={() => setIsMonitoring(!isMonitoring)}
          >
            {isMonitoring ? "Pause" : "Resume"}
          </Button>
        </div>
      </div>

      {/* Quick Stats */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <Card>
          <CardContent className="pt-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">CPU Usage</p>
                <p className="text-2xl font-bold">{currentMetrics.cpu.usage.toFixed(1)}%</p>
              </div>
              <Cpu className="h-8 w-8 text-muted-foreground" />
            </div>
            <Progress value={currentMetrics.cpu.usage} className="mt-2" />
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Memory Usage</p>
                <p className="text-2xl font-bold">{currentMetrics.memory.usage_percent.toFixed(1)}%</p>
              </div>
              <MemoryStick className="h-8 w-8 text-muted-foreground" />
            </div>
            <Progress value={currentMetrics.memory.usage_percent} className="mt-2" />
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">GPU Usage</p>
                <p className="text-2xl font-bold">{currentMetrics.gpu.utilization.toFixed(1)}%</p>
              </div>
              <Monitor className="h-8 w-8 text-muted-foreground" />
            </div>
            <Progress value={currentMetrics.gpu.utilization} className="mt-2" />
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Disk Usage</p>
                <p className="text-2xl font-bold">{currentMetrics.disk.usage_percent.toFixed(1)}%</p>
              </div>
              <HardDrive className="h-8 w-8 text-muted-foreground" />
            </div>
            <Progress value={currentMetrics.disk.usage_percent} className="mt-2" />
          </CardContent>
        </Card>
      </div>

      <Tabs defaultValue="overview" className="space-y-4">
        <TabsList>
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="gpu">GPU Details</TabsTrigger>
          <TabsTrigger value="network">Network</TabsTrigger>
          <TabsTrigger value="training">Training</TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center">
                  <Cpu className="mr-2 h-5 w-5" />
                  CPU Details
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-3">
                <div className="flex justify-between">
                  <span className="text-sm text-muted-foreground">Usage</span>
                  <span className="font-medium">{currentMetrics.cpu.usage.toFixed(1)}%</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-sm text-muted-foreground">Cores</span>
                  <span className="font-medium">{currentMetrics.cpu.cores}</span>
                </div>
                {currentMetrics.cpu.temperature && (
                  <div className="flex justify-between">
                    <span className="text-sm text-muted-foreground">Temperature</span>
                    <span className="font-medium">{currentMetrics.cpu.temperature.toFixed(1)}°C</span>
                  </div>
                )}
                <Progress value={currentMetrics.cpu.usage} className="h-2" />
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle className="flex items-center">
                  <MemoryStick className="mr-2 h-5 w-5" />
                  Memory Details
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-3">
                <div className="flex justify-between">
                  <span className="text-sm text-muted-foreground">Used</span>
                  <span className="font-medium">{formatBytes(currentMetrics.memory.used * 1024 * 1024 * 1024)}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-sm text-muted-foreground">Total</span>
                  <span className="font-medium">{formatBytes(currentMetrics.memory.total * 1024 * 1024 * 1024)}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-sm text-muted-foreground">Usage</span>
                  <span className="font-medium">{currentMetrics.memory.usage_percent.toFixed(1)}%</span>
                </div>
                <Progress value={currentMetrics.memory.usage_percent} className="h-2" />
              </CardContent>
            </Card>
          </div>
        </TabsContent>

        <TabsContent value="gpu" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center">
                <Monitor className="mr-2 h-5 w-5" />
                GPU Details
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div className="text-center">
                  <div className="text-2xl font-bold">{currentMetrics.gpu.utilization.toFixed(1)}%</div>
                  <div className="text-xs text-muted-foreground">Utilization</div>
                </div>
                <div className="text-center">
                  <div className="text-2xl font-bold">{formatBytes(currentMetrics.gpu.memory_used * 1024 * 1024 * 1024)}</div>
                  <div className="text-xs text-muted-foreground">Memory Used</div>
                </div>
                <div className="text-center">
                  <div className="text-2xl font-bold">{formatBytes(currentMetrics.gpu.memory_total * 1024 * 1024 * 1024)}</div>
                  <div className="text-xs text-muted-foreground">Memory Total</div>
                </div>
                {currentMetrics.gpu.temperature && (
                  <div className="text-center">
                    <div className="text-2xl font-bold">{currentMetrics.gpu.temperature.toFixed(1)}°C</div>
                    <div className="text-xs text-muted-foreground">Temperature</div>
                  </div>
                )}
              </div>
              
              <div className="space-y-2">
                <div className="flex justify-between text-sm">
                  <span>GPU Utilization</span>
                  <span>{currentMetrics.gpu.utilization.toFixed(1)}%</span>
                </div>
                <Progress value={currentMetrics.gpu.utilization} className="h-2" />
              </div>

              <div className="space-y-2">
                <div className="flex justify-between text-sm">
                  <span>GPU Memory</span>
                  <span>{((currentMetrics.gpu.memory_used / currentMetrics.gpu.memory_total) * 100).toFixed(1)}%</span>
                </div>
                <Progress value={(currentMetrics.gpu.memory_used / currentMetrics.gpu.memory_total) * 100} className="h-2" />
              </div>

              {currentMetrics.gpu.power_draw && (
                <div className="flex justify-between text-sm">
                  <span>Power Draw</span>
                  <span>{currentMetrics.gpu.power_draw.toFixed(1)}W</span>
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="network" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center">
                <Wifi className="mr-2 h-5 w-5" />
                Network Activity
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div className="text-center">
                  <div className="text-2xl font-bold">{formatBytes(currentMetrics.network.bytes_in)}</div>
                  <div className="text-xs text-muted-foreground">Bytes In</div>
                </div>
                <div className="text-center">
                  <div className="text-2xl font-bold">{formatBytes(currentMetrics.network.bytes_out)}</div>
                  <div className="text-xs text-muted-foreground">Bytes Out</div>
                </div>
                <div className="text-center">
                  <div className="text-2xl font-bold">{formatNumber(currentMetrics.network.packets_in)}</div>
                  <div className="text-xs text-muted-foreground">Packets In</div>
                </div>
                <div className="text-center">
                  <div className="text-2xl font-bold">{formatNumber(currentMetrics.network.packets_out)}</div>
                  <div className="text-xs text-muted-foreground">Packets Out</div>
                </div>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="training" className="space-y-4">
          {currentMetrics.training ? (
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center">
                  <Activity className="mr-2 h-5 w-5" />
                  Training Metrics
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                  <div className="text-center">
                    <div className="text-2xl font-bold">{currentMetrics.training.tokens_per_second.toFixed(0)}</div>
                    <div className="text-xs text-muted-foreground">Tokens/sec</div>
                  </div>
                  <div className="text-center">
                    <div className="text-2xl font-bold">{currentMetrics.training.loss.toFixed(4)}</div>
                    <div className="text-xs text-muted-foreground">Loss</div>
                  </div>
                  <div className="text-center">
                    <div className="text-2xl font-bold">{currentMetrics.training.current_epoch}</div>
                    <div className="text-xs text-muted-foreground">Current Epoch</div>
                  </div>
                  <div className="text-center">
                    <div className="text-2xl font-bold">{currentMetrics.training.total_epochs}</div>
                    <div className="text-xs text-muted-foreground">Total Epochs</div>
                  </div>
                </div>
              </CardContent>
            </Card>
          ) : (
            <Card>
              <CardContent className="text-center py-12">
                <Activity className="h-12 w-12 text-muted-foreground mx-auto mb-4" />
                <h3 className="text-lg font-medium mb-2">No Training Activity</h3>
                <p className="text-muted-foreground">
                  Start a training job to see detailed metrics here.
                </p>
              </CardContent>
            </Card>
          )}
        </TabsContent>
      </Tabs>

      {/* Charts Placeholder */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <BarChart3 className="mr-2 h-5 w-5" />
            Resource Trends
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="h-64 flex items-center justify-center text-muted-foreground">
            Resource usage charts would go here
            <br />
            <small className="text-xs">Real-time CPU, Memory, GPU, and Network usage over time</small>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
