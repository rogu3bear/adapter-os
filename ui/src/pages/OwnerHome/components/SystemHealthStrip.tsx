/**
 * System Health Strip
 *
 * Top strip showing system name, version, environment, and health summary.
 *
 * Citations:
 * - crates/adapteros-server-api/src/handlers/system_overview.rs L19-L33
 */

import React from 'react';
import {
  Server,
  Cpu,
  MemoryStick,
  HardDrive,
  Activity,
  CheckCircle,
  AlertTriangle,
  XCircle,
  Database,
  Layers,
} from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { HelpTooltip } from '@/components/ui/help-tooltip';
import { SystemOverview } from '@/api/owner-types';
import type { BaseModelStatus, BackendStatus } from '@/api/api-types';
import type { AdapterStack } from '@/api/adapter-types';

interface SystemHealthStripProps {
  systemOverview?: SystemOverview;
  isLoading: boolean;
  error?: Error | null;
  baseModelStatus?: BaseModelStatus;
  backends?: BackendStatus[];
  activeStack?: AdapterStack | null;
  /** Whether real-time SSE updates are active */
  isLive?: boolean;
}

export default function SystemHealthStrip({
  systemOverview,
  isLoading,
  error,
  baseModelStatus,
  backends,
  activeStack,
  isLive = false,
}: SystemHealthStripProps) {
  if (isLoading) {
    return (
      <div className="bg-white rounded-lg border p-4 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <Skeleton className="h-10 w-10 rounded-full" />
          <Skeleton className="h-6 w-32" />
          <Skeleton className="h-5 w-24" />
        </div>
        <div className="flex items-center gap-6">
          <Skeleton className="h-5 w-20" />
          <Skeleton className="h-5 w-20" />
          <Skeleton className="h-5 w-20" />
          <Skeleton className="h-5 w-20" />
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-red-50 rounded-lg border border-red-200 p-4 flex items-center gap-3">
        <XCircle className="h-5 w-5 text-red-500" />
        <span className="text-red-700">Failed to load system status</span>
      </div>
    );
  }

  const healthyCount = systemOverview?.services?.filter(
    (s) => s.status === 'healthy'
  ).length || 0;
  const totalServices = systemOverview?.services?.length || 0;
  const allHealthy = healthyCount === totalServices && totalServices > 0;

  const formatUptime = (seconds?: number) => {
    if (!seconds) return 'Unknown';
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    if (days > 0) return `${days}d ${hours}h`;
    const minutes = Math.floor((seconds % 3600) / 60);
    return `${hours}h ${minutes}m`;
  };

  return (
    <div className="bg-white rounded-lg border p-4 flex flex-wrap items-center justify-between gap-4">
      {/* Left: System Identity */}
      <div className="flex items-center gap-4">
        <div className="h-10 w-10 bg-gradient-to-br from-blue-500 to-indigo-600 rounded-full flex items-center justify-center">
          <Server className="h-5 w-5 text-white" />
        </div>
        <div>
          <h2 className="text-lg font-semibold text-slate-900">AdapterOS</h2>
          <div className="flex items-center gap-2 text-sm text-slate-600">
            <span>v{systemOverview?.schema_version || '0.3-alpha'}</span>
            <span className="text-slate-300">|</span>
            <Badge variant="outline" className="text-xs">
              development
            </Badge>
            {isLive && (
              <Badge variant="outline" className="text-xs gap-1 text-green-600 border-green-300 bg-green-50">
                <span className="h-1.5 w-1.5 rounded-full bg-green-500 animate-pulse" />
                Live
              </Badge>
            )}
          </div>
        </div>
      </div>

      {/* Active Context - Model, Backend, Stack */}
      <div className="flex items-center gap-4 px-4 py-2 bg-slate-50 rounded-lg border border-slate-200">
        {/* Base Model */}
        <HelpTooltip content="Currently loaded base model">
          <div className="flex items-center gap-2">
            <Database className="h-4 w-4 text-indigo-500" />
            <span className="text-sm truncate max-w-[140px]">
              {baseModelStatus?.model_name || 'No model loaded'}
            </span>
            {baseModelStatus?.status === 'ready' || baseModelStatus?.status === 'loaded' ? (
              <Badge variant="default" className="text-xs">Ready</Badge>
            ) : baseModelStatus?.status === 'loading' ? (
              <Badge variant="secondary" className="text-xs">Loading</Badge>
            ) : null}
          </div>
        </HelpTooltip>

        <span className="text-slate-300">|</span>

        {/* Backend */}
        <HelpTooltip content="Active inference backend">
          <div className="flex items-center gap-2">
            <Cpu className="h-4 w-4 text-blue-500" />
            <span className="text-sm">
              {backends?.find(b => b.status === 'healthy')?.backend?.toUpperCase() || 'Auto'}
            </span>
            {backends?.some(b => b.status === 'healthy') && (
              <Badge variant="default" className="text-xs">OK</Badge>
            )}
          </div>
        </HelpTooltip>

        <span className="text-slate-300">|</span>

        {/* Active Stack */}
        <HelpTooltip content="Active adapter stack">
          <div className="flex items-center gap-2">
            <Layers className="h-4 w-4 text-purple-500" />
            <span className="text-sm truncate max-w-[120px]">
              {activeStack?.name || 'No active stack'}
            </span>
            {activeStack && (
              <Badge variant="secondary" className="text-xs">
                {activeStack.adapter_ids?.length || activeStack.adapters?.length || 0} adapters
              </Badge>
            )}
          </div>
        </HelpTooltip>
      </div>

      {/* Center: Health Indicators */}
      <div className="flex items-center gap-6">
        <HelpTooltip content="Overall service health status">
          <div className="flex items-center gap-2">
            {allHealthy ? (
              <CheckCircle className="h-5 w-5 text-green-500" />
            ) : (
              <AlertTriangle className="h-5 w-5 text-amber-500" />
            )}
            <span className="text-sm font-medium">
              {healthyCount}/{totalServices} Healthy
            </span>
          </div>
        </HelpTooltip>

        <HelpTooltip content="Number of running processes">
          <div className="flex items-center gap-2 text-sm">
            <Activity className="h-4 w-4 text-slate-500" />
            <span>{systemOverview?.process_count ?? 0} Processes</span>
          </div>
        </HelpTooltip>

        <HelpTooltip content="Number of active worker processes">
          <div className="flex items-center gap-2 text-sm">
            <Cpu className="h-4 w-4 text-slate-500" />
            <span>{systemOverview?.active_workers ?? 0} Workers</span>
          </div>
        </HelpTooltip>

        <HelpTooltip content="System uptime since last restart">
          <div className="flex items-center gap-2 text-sm">
            <span className="text-slate-400">Uptime:</span>
            <span className="font-medium">
              {formatUptime(systemOverview?.uptime_seconds)}
            </span>
          </div>
        </HelpTooltip>
      </div>

      {/* Right: Resource Usage */}
      <div className="flex items-center gap-4">
        <HelpTooltip content="Current CPU utilization">
          <div className="flex items-center gap-2 text-sm">
            <Cpu className="h-4 w-4 text-blue-500" />
            <span className="font-medium">
              {systemOverview?.resource_usage?.cpu_usage_percent?.toFixed(0) ?? '--'}%
            </span>
          </div>
        </HelpTooltip>

        <HelpTooltip content="Current memory utilization">
          <div className="flex items-center gap-2 text-sm">
            <MemoryStick className="h-4 w-4 text-purple-500" />
            <span className="font-medium">
              {systemOverview?.resource_usage?.memory_usage_percent?.toFixed(0) ?? '--'}%
            </span>
          </div>
        </HelpTooltip>

        <HelpTooltip content="Current GPU utilization">
          <div className="flex items-center gap-2 text-sm">
            <HardDrive className="h-4 w-4 text-green-500" />
            <span className="font-medium">
              {systemOverview?.resource_usage?.gpu_utilization_percent?.toFixed(0) ?? '--'}%
            </span>
          </div>
        </HelpTooltip>
      </div>
    </div>
  );
}
