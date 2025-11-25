import React from 'react';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { AlertTriangle, CheckCircle2, XCircle, HardDrive, Cpu, Activity } from 'lucide-react';
import apiClient from '@/api/client';
// Format bytes to human-readable string
function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(2)} ${sizes[i]}`;
}

interface CapacityResponse {
  total_ram_bytes: number;
  total_vram_bytes: number;
  limits: {
    models_per_worker?: number;
    models_per_tenant?: number;
    concurrent_requests?: number;
  };
  usage: {
    models_loaded: number;
    adapters_loaded: number;
    active_requests: number;
    ram_used_bytes: number;
    vram_used_bytes: number;
    ram_headroom_pct: number;
    vram_headroom_pct: number;
  };
  node_health: 'ok' | 'warning' | 'critical';
}

export function CapacityTab() {
  const { data, isLoading, error } = useQuery<CapacityResponse>({
    queryKey: ['system-capacity'],
    queryFn: () => apiClient.getCapacity(),
    refetchInterval: 10000, // Refresh every 10 seconds
  });

  if (isLoading) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-32 w-full" />
        <Skeleton className="h-48 w-full" />
        <Skeleton className="h-48 w-full" />
      </div>
    );
  }

  if (error) {
    return (
      <Alert variant="destructive">
        <XCircle className="h-4 w-4" />
        <AlertTitle>Error</AlertTitle>
        <AlertDescription>Failed to load capacity information: {error.message}</AlertDescription>
      </Alert>
    );
  }

  if (!data) {
    return null;
  }

  const getHealthBadge = (health: string) => {
    switch (health) {
      case 'critical':
        return <Badge variant="destructive" className="flex items-center gap-1"><XCircle className="h-3 w-3" /> Critical</Badge>;
      case 'warning':
        return <Badge variant="outline" className="flex items-center gap-1 bg-yellow-50 text-yellow-800 border-yellow-300"><AlertTriangle className="h-3 w-3" /> Warning</Badge>;
      default:
        return <Badge variant="outline" className="flex items-center gap-1 bg-green-50 text-green-800 border-green-300"><CheckCircle2 className="h-3 w-3" /> OK</Badge>;
    }
  };

  const getHeadroomColor = (pct: number) => {
    if (pct < 10) return 'text-red-600';
    if (pct < 20) return 'text-yellow-600';
    return 'text-green-600';
  };

  return (
    <div className="space-y-6">
      {/* Node Health Status */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center justify-between">
            <span className="flex items-center gap-2">
              <Activity className="h-5 w-5" />
              Node Health Status
            </span>
            {getHealthBadge(data.node_health)}
          </CardTitle>
        </CardHeader>
        <CardContent>
          {data.node_health === 'critical' && (
            <Alert variant="destructive" className="mb-4">
              <AlertTriangle className="h-4 w-4" />
              <AlertTitle>Critical Memory Pressure</AlertTitle>
              <AlertDescription>
                System memory is critically low. New training jobs will be blocked until memory pressure is resolved.
              </AlertDescription>
            </Alert>
          )}
          {data.node_health === 'warning' && (
            <Alert className="mb-4 border-yellow-300 bg-yellow-50">
              <AlertTriangle className="h-4 w-4 text-yellow-600" />
              <AlertTitle className="text-yellow-800">Memory Pressure Warning</AlertTitle>
              <AlertDescription className="text-yellow-700">
                System memory is below recommended thresholds. Consider reducing concurrent operations.
              </AlertDescription>
            </Alert>
          )}
          {data.node_health === 'ok' && (
            <Alert className="mb-4 border-green-300 bg-green-50">
              <CheckCircle2 className="h-4 w-4 text-green-600" />
              <AlertTitle className="text-green-800">System Operating Normally</AlertTitle>
              <AlertDescription className="text-green-700">
                All systems are operating within normal capacity limits.
              </AlertDescription>
            </Alert>
          )}
        </CardContent>
      </Card>

      {/* Memory Usage */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <HardDrive className="h-5 w-5" />
              System RAM
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div>
              <div className="flex justify-between text-sm mb-1">
                <span>Total</span>
                <span className="font-mono">{formatBytes(data.total_ram_bytes)}</span>
              </div>
              <div className="flex justify-between text-sm mb-1">
                <span>Used</span>
                <span className="font-mono">{formatBytes(data.usage.ram_used_bytes)}</span>
              </div>
              <div className="flex justify-between text-sm mb-2">
                <span>Available</span>
                <span className="font-mono">{formatBytes(data.total_ram_bytes - data.usage.ram_used_bytes)}</span>
              </div>
              <div className="w-full bg-gray-200 rounded-full h-2">
                <div
                  className={`h-2 rounded-full ${getHeadroomColor(data.usage.ram_headroom_pct)}`}
                  style={{
                    width: `${100 - (data.usage.ram_used_bytes / data.total_ram_bytes) * 100}%`,
                    backgroundColor: data.usage.ram_headroom_pct < 10 ? '#dc2626' : data.usage.ram_headroom_pct < 20 ? '#d97706' : '#16a34a',
                  }}
                />
              </div>
              <div className="text-sm text-muted-foreground mt-1">
                Headroom: <span className={getHeadroomColor(data.usage.ram_headroom_pct)}>{data.usage.ram_headroom_pct.toFixed(1)}%</span>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Cpu className="h-5 w-5" />
              VRAM (GPU Memory)
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div>
              <div className="flex justify-between text-sm mb-1">
                <span>Total</span>
                <span className="font-mono">{formatBytes(data.total_vram_bytes)}</span>
              </div>
              <div className="flex justify-between text-sm mb-1">
                <span>Used</span>
                <span className="font-mono">{formatBytes(data.usage.vram_used_bytes)}</span>
              </div>
              <div className="flex justify-between text-sm mb-2">
                <span>Available</span>
                <span className="font-mono">{formatBytes(data.total_vram_bytes - data.usage.vram_used_bytes)}</span>
              </div>
              <div className="w-full bg-gray-200 rounded-full h-2">
                <div
                  className={`h-2 rounded-full ${getHeadroomColor(data.usage.vram_headroom_pct)}`}
                  style={{
                    width: `${100 - (data.usage.vram_used_bytes / data.total_vram_bytes) * 100}%`,
                    backgroundColor: data.usage.vram_headroom_pct < 10 ? '#dc2626' : data.usage.vram_headroom_pct < 20 ? '#d97706' : '#16a34a',
                  }}
                />
              </div>
              <div className="text-sm text-muted-foreground mt-1">
                Headroom: <span className={getHeadroomColor(data.usage.vram_headroom_pct)}>{data.usage.vram_headroom_pct.toFixed(1)}%</span>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Current Usage */}
      <Card>
        <CardHeader>
          <CardTitle>Current Usage</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div>
              <div className="text-sm text-muted-foreground">Models Loaded</div>
              <div className="text-2xl font-bold">{data.usage.models_loaded}</div>
              {data.limits.models_per_worker && (
                <div className="text-xs text-muted-foreground">Limit: {data.limits.models_per_worker} per worker</div>
              )}
            </div>
            <div>
              <div className="text-sm text-muted-foreground">Adapters Loaded</div>
              <div className="text-2xl font-bold">{data.usage.adapters_loaded}</div>
              {data.limits.models_per_tenant && (
                <div className="text-xs text-muted-foreground">Limit: {data.limits.models_per_tenant} per tenant</div>
              )}
            </div>
            <div>
              <div className="text-sm text-muted-foreground">Active Requests</div>
              <div className="text-2xl font-bold">{data.usage.active_requests}</div>
              {data.limits.concurrent_requests && (
                <div className="text-xs text-muted-foreground">Limit: {data.limits.concurrent_requests} concurrent</div>
              )}
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Configured Limits */}
      <Card>
        <CardHeader>
          <CardTitle>Configured Limits</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div>
              <div className="text-sm font-medium">Models per Worker</div>
              <div className="text-lg">{data.limits.models_per_worker ?? 'Not configured'}</div>
            </div>
            <div>
              <div className="text-sm font-medium">Models per Tenant</div>
              <div className="text-lg">{data.limits.models_per_tenant ?? 'Not configured'}</div>
            </div>
            <div>
              <div className="text-sm font-medium">Concurrent Requests</div>
              <div className="text-lg">{data.limits.concurrent_requests ?? 'Not configured'}</div>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

