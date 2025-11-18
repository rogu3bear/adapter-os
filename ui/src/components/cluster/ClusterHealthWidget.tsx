import React from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import {
  Server,
  Activity,
  AlertTriangle,
  CheckCircle,
  XCircle,
  Cpu,
  MemoryStick,
  HardDrive,
  Network,
} from 'lucide-react';
import type { ClusterStatus, Node, WorkerResponse } from '@/api/types';

interface ClusterHealthWidgetProps {
  clusterStatus: ClusterStatus | null;
  nodes: Node[];
  workers: WorkerResponse[];
  onRefresh: () => void;
}

export function ClusterHealthWidget({ clusterStatus, nodes, workers }: ClusterHealthWidgetProps) {
  if (!clusterStatus) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Cluster Health</CardTitle>
          <CardDescription>Unable to load cluster status</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex items-center gap-2 text-yellow-600">
            <AlertTriangle className="h-5 w-5" />
            <span>Cluster status unavailable</span>
          </div>
        </CardContent>
      </Card>
    );
  }

  const healthPercentage = clusterStatus.total_nodes > 0
    ? (clusterStatus.healthy_nodes / clusterStatus.total_nodes) * 100
    : 0;

  const getHealthStatus = () => {
    if (healthPercentage >= 90) return { color: 'text-green-600', icon: CheckCircle, label: 'Healthy' };
    if (healthPercentage >= 70) return { color: 'text-yellow-600', icon: AlertTriangle, label: 'Degraded' };
    return { color: 'text-red-600', icon: XCircle, label: 'Critical' };
  };

  const healthStatus = getHealthStatus();
  const HealthIcon = healthStatus.icon;

  return (
    <div className="space-y-6">
      {/* Overall Cluster Status */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Network className="h-5 w-5" />
            Cluster Health Overview
          </CardTitle>
          <CardDescription>
            Cluster ID: {clusterStatus.cluster_id}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex items-center gap-4 mb-6">
            <HealthIcon className={`h-8 w-8 ${healthStatus.color}`} />
            <div>
              <div className={`text-2xl font-bold ${healthStatus.color}`}>
                {healthStatus.label}
              </div>
              <div className="text-sm text-muted-foreground">
                {Math.round(healthPercentage)}% nodes healthy
              </div>
            </div>
          </div>

          {/* Cluster Metrics Grid */}
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <MetricCard
              icon={Server}
              label="Total Nodes"
              value={clusterStatus.total_nodes}
              color="text-blue-600"
            />
            <MetricCard
              icon={CheckCircle}
              label="Healthy Nodes"
              value={clusterStatus.healthy_nodes}
              color="text-green-600"
            />
            <MetricCard
              icon={XCircle}
              label="Offline Nodes"
              value={clusterStatus.offline_nodes}
              color="text-red-600"
            />
            <MetricCard
              icon={AlertTriangle}
              label="Maintenance"
              value={clusterStatus.maintenance_nodes}
              color="text-yellow-600"
            />
          </div>
        </CardContent>
      </Card>

      {/* Worker Status */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Activity className="h-5 w-5" />
            Worker Status
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
            <MetricCard
              icon={Activity}
              label="Total Workers"
              value={clusterStatus.total_workers}
              color="text-blue-600"
            />
            <MetricCard
              icon={CheckCircle}
              label="Active Workers"
              value={clusterStatus.active_workers}
              color="text-green-600"
            />
            <MetricCard
              icon={XCircle}
              label="Inactive Workers"
              value={clusterStatus.total_workers - clusterStatus.active_workers}
              color="text-gray-600"
            />
          </div>
        </CardContent>
      </Card>

      {/* Resource Summary */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <HardDrive className="h-5 w-5" />
            Cluster Resources
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
            <MetricCard
              icon={MemoryStick}
              label="Total Memory"
              value={`${clusterStatus.aggregate_memory_gb} GB`}
              color="text-purple-600"
            />
            <MetricCard
              icon={Cpu}
              label="GPU Count"
              value={clusterStatus.aggregate_gpu_count}
              color="text-orange-600"
            />
            <MetricCard
              icon={Server}
              label="Avg Memory/Node"
              value={clusterStatus.total_nodes > 0
                ? `${Math.round(clusterStatus.aggregate_memory_gb / clusterStatus.total_nodes)} GB`
                : '0 GB'}
              color="text-teal-600"
            />
          </div>
        </CardContent>
      </Card>

      {/* Node Status Breakdown */}
      <Card>
        <CardHeader>
          <CardTitle>Node Status Distribution</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-2">
            {nodes.map((node) => (
              <div
                key={node.id}
                className="flex items-center justify-between p-3 rounded-lg border hover:bg-accent/50 transition-colors"
              >
                <div className="flex items-center gap-3">
                  <Server className="h-4 w-4 text-muted-foreground" />
                  <div>
                    <div className="font-medium">{node.hostname}</div>
                    <div className="text-sm text-muted-foreground">
                      {node.metal_family} • {node.memory_gb} GB
                    </div>
                  </div>
                </div>
                <Badge
                  variant={
                    node.status === 'active' ? 'default' :
                    node.status === 'offline' ? 'destructive' :
                    'secondary'
                  }
                >
                  {node.status}
                </Badge>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

interface MetricCardProps {
  icon: React.ElementType;
  label: string;
  value: string | number;
  color: string;
}

function MetricCard({ icon: Icon, label, value, color }: MetricCardProps) {
  return (
    <div className="flex items-center gap-3 p-4 rounded-lg border bg-card">
      <Icon className={`h-6 w-6 ${color}`} />
      <div>
        <div className="text-2xl font-bold">{value}</div>
        <div className="text-sm text-muted-foreground">{label}</div>
      </div>
    </div>
  );
}
