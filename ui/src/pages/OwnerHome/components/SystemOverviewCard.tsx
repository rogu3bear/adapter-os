/**
 * System Overview Card
 *
 * Card component displaying system overview data including uptime, process count,
 * worker count, active sessions, adapters, and service health status.
 *
 * Used in Owner Home page dashboard.
 *
 * Citations:
 * - crates/adapteros-server-api/src/handlers/system_overview.rs L19-L33
 */

import React from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Server,
  CheckCircle,
  AlertTriangle,
  XCircle,
  Clock,
  ExternalLink,
  Cpu,
  HardDrive,
  Activity,
} from 'lucide-react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { SystemOverview } from '@/api/owner-types';

interface SystemOverviewCardProps {
  systemOverview?: SystemOverview;
  isLoading: boolean;
}

export default function SystemOverviewCard({
  systemOverview,
  isLoading,
}: SystemOverviewCardProps) {
  const navigate = useNavigate();

  const formatUptime = (seconds?: number): string => {
    if (!seconds) return 'Unknown';
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);

    if (days > 0) {
      return `${days}d ${hours}h ${minutes}m`;
    }
    if (hours > 0) {
      return `${hours}h ${minutes}m`;
    }
    return `${minutes}m`;
  };

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'healthy':
        return <CheckCircle className="h-4 w-4 text-green-500" />;
      case 'degraded':
        return <AlertTriangle className="h-4 w-4 text-amber-500" />;
      case 'unhealthy':
        return <XCircle className="h-4 w-4 text-red-500" />;
      default:
        return <AlertTriangle className="h-4 w-4 text-slate-400" />;
    }
  };

  const getStatusBadgeVariant = (status: string) => {
    switch (status) {
      case 'healthy':
        return 'default' as const;
      case 'degraded':
        return 'secondary' as const;
      case 'unhealthy':
        return 'destructive' as const;
      default:
        return 'outline' as const;
    }
  };

  if (isLoading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Server className="h-5 w-5" />
            System Overview
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <Skeleton className="h-20 w-full" />
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-10 w-32" />
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Server className="h-5 w-5" />
          System Overview
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-6">
        {/* System Metrics */}
        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-1">
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Clock className="h-4 w-4" />
              <span>Uptime</span>
            </div>
            <p className="text-2xl font-semibold">
              {formatUptime(systemOverview?.uptime_seconds)}
            </p>
          </div>

          <div className="space-y-1">
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Activity className="h-4 w-4" />
              <span>Processes</span>
            </div>
            <p className="text-2xl font-semibold">
              {systemOverview?.process_count ?? 0}
            </p>
          </div>

          <div className="space-y-1">
            <p className="text-sm text-muted-foreground">Workers</p>
            <p className="text-2xl font-semibold">
              {systemOverview?.active_workers ?? 0}
            </p>
          </div>

          <div className="space-y-1">
            <p className="text-sm text-muted-foreground">Active Sessions</p>
            <p className="text-2xl font-semibold">
              {systemOverview?.active_sessions ?? 0}
            </p>
          </div>

          <div className="space-y-1">
            <p className="text-sm text-muted-foreground">Adapters</p>
            <p className="text-2xl font-semibold">
              {systemOverview?.adapter_count ?? 0}
            </p>
          </div>

          <div className="space-y-1">
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Cpu className="h-4 w-4" />
              <span>CPU</span>
            </div>
            <p className="text-2xl font-semibold">
              {systemOverview?.resource_usage?.cpu_usage_percent?.toFixed(1) ?? 0}%
            </p>
          </div>
        </div>

        {/* Resource Usage */}
        {systemOverview?.resource_usage && (
          <div className="space-y-2">
            <h4 className="text-sm font-medium text-slate-700">Resource Usage</h4>
            <div className="grid grid-cols-3 gap-2 text-sm">
              <div className="p-2 bg-slate-50 rounded">
                <span className="text-muted-foreground">Memory</span>
                <p className="font-medium">{systemOverview.resource_usage.memory_usage_percent.toFixed(1)}%</p>
              </div>
              <div className="p-2 bg-slate-50 rounded">
                <span className="text-muted-foreground">Disk</span>
                <p className="font-medium">{systemOverview.resource_usage.disk_usage_percent.toFixed(1)}%</p>
              </div>
              {systemOverview.resource_usage.gpu_utilization_percent !== undefined && (
                <div className="p-2 bg-slate-50 rounded">
                  <span className="text-muted-foreground">GPU</span>
                  <p className="font-medium">{systemOverview.resource_usage.gpu_utilization_percent.toFixed(1)}%</p>
                </div>
              )}
            </div>
          </div>
        )}

        {/* Service Health Status */}
        {systemOverview?.services && systemOverview.services.length > 0 && (
          <div className="space-y-3">
            <h4 className="text-sm font-medium text-slate-700">
              Service Health
            </h4>
            <div className="space-y-2">
              {systemOverview.services.map((service, index) => (
                <div
                  key={index}
                  className="flex items-center justify-between p-2 rounded-md bg-slate-50 hover:bg-slate-100 transition-colors"
                >
                  <div className="flex items-center gap-2">
                    {getStatusIcon(service.status)}
                    <span className="text-sm font-medium">{service.name}</span>
                  </div>
                  <Badge variant={getStatusBadgeVariant(service.status)}>
                    {service.status}
                  </Badge>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* View Details Button */}
        <Button
          variant="outline"
          className="w-full"
          onClick={() => navigate('/system')}
        >
          <span>View Details</span>
          <ExternalLink className="h-4 w-4 ml-2" />
        </Button>
      </CardContent>
    </Card>
  );
}
