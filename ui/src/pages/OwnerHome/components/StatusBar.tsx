/**
 * StatusBar - Simplified system status display
 *
 * Streamlined horizontal bar showing essential system health indicators.
 * Replaces the more verbose SystemHealthStrip with focused metrics.
 *
 * Mobile: horizontal scroll with icon-only mode
 * Desktop: full labels visible
 */

import React from 'react';
import {
  Server,
  Database,
  MemoryStick,
  CheckCircle,
  AlertTriangle,
  XCircle,
  Layers,
  Activity,
} from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { cn } from '@/components/ui/utils';
import type { SystemOverview } from '@/api/owner-types';
import type { BaseModelStatus } from '@/api/api-types';
import type { Adapter } from '@/api/types';

interface SystemStateData {
  memory?: {
    pressure_level?: 'low' | 'medium' | 'high' | 'critical';
    used_mb?: number;
    total_mb?: number;
  };
}

interface StatusBarProps {
  systemOverview?: SystemOverview;
  baseModelStatus?: BaseModelStatus;
  adapters?: Adapter[];
  systemState?: SystemStateData;
  isLoading: boolean;
  error?: Error | null;
  isLive?: boolean;
}

const MEMORY_PRESSURE_COLORS = {
  low: 'bg-success-surface text-success border-success/30',
  medium: 'bg-warning-surface text-warning border-warning/30',
  high: 'bg-warning-surface text-warning border-warning/50',
  critical: 'bg-destructive-surface text-destructive border-destructive/30',
} as const;

export function StatusBar({
  systemOverview,
  baseModelStatus,
  adapters,
  systemState,
  isLoading,
  error,
  isLive = false,
}: StatusBarProps) {
  if (isLoading) {
    return (
      <div className="bg-white rounded-lg border p-3 flex items-center gap-4 overflow-x-auto">
        <Skeleton className="h-8 w-8 rounded-full flex-shrink-0" />
        <Skeleton className="h-5 w-24 flex-shrink-0" />
        <Skeleton className="h-5 w-32 flex-shrink-0" />
        <Skeleton className="h-5 w-20 flex-shrink-0" />
        <Skeleton className="h-5 w-24 flex-shrink-0" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-destructive-surface rounded-lg border border-destructive/20 p-3 flex items-center gap-3">
        <XCircle className="h-5 w-5 text-destructive flex-shrink-0" />
        <span className="text-destructive text-sm">Failed to load system status</span>
      </div>
    );
  }

  const healthyCount = systemOverview?.services?.filter(
    (s) => s.status === 'healthy'
  ).length || 0;
  const totalServices = systemOverview?.services?.length || 0;
  const allHealthy = healthyCount === totalServices && totalServices > 0;

  const memoryPressure = systemState?.memory?.pressure_level || 'low';
  const adapterCount = Array.isArray(adapters) ? adapters.length : 0;
  const hotAdapters = Array.isArray(adapters)
    ? adapters.filter(a => a.lifecycle_state === 'hot').length
    : 0;

  return (
    <div className="bg-white rounded-lg border p-3 flex items-center gap-3 sm:gap-6 overflow-x-auto scrollbar-thin">
      {/* System Identity - Always visible */}
      <div className="flex items-center gap-2 flex-shrink-0">
        <div className="h-8 w-8 bg-gradient-to-br from-primary to-primary/80 rounded-full flex items-center justify-center">
          <Server className="h-4 w-4 text-primary-foreground" />
        </div>
        <div className="hidden sm:block">
          <span className="text-sm font-semibold text-slate-900">AdapterOS</span>
          {isLive && (
            <span className="ml-2 inline-flex items-center gap-1 text-xs text-success">
              <span className="h-1.5 w-1.5 rounded-full bg-success animate-pulse" />
              Live
            </span>
          )}
        </div>
      </div>

      <div className="h-6 w-px bg-slate-200 hidden sm:block flex-shrink-0" />

      {/* Health Status */}
      <GlossaryTooltip brief={`${healthyCount} of ${totalServices} services healthy`}>
        <div className="flex items-center gap-2 flex-shrink-0">
          {allHealthy ? (
            <CheckCircle className="h-5 w-5 text-success" />
          ) : totalServices > 0 ? (
            <AlertTriangle className="h-5 w-5 text-warning" />
          ) : (
            <Activity className="h-5 w-5 text-slate-400" />
          )}
          <span className="text-sm font-medium hidden sm:inline">
            {allHealthy ? 'Healthy' : `${healthyCount}/${totalServices}`}
          </span>
        </div>
      </GlossaryTooltip>

      <div className="h-6 w-px bg-slate-200 flex-shrink-0" />

      {/* Base Model */}
      <GlossaryTooltip brief={baseModelStatus?.model_name || 'No model loaded'}>
        <div className="flex items-center gap-2 flex-shrink-0">
          <Database className="h-4 w-4 text-primary" />
          <span className="text-sm truncate max-w-[100px] sm:max-w-[160px]">
            {baseModelStatus?.model_name || (
              <span className="text-slate-400">No model</span>
            )}
          </span>
          {(baseModelStatus?.status === 'ready' || baseModelStatus?.status === 'loaded') && (
            <Badge variant="default" className="text-xs hidden sm:inline-flex">
              Ready
            </Badge>
          )}
        </div>
      </GlossaryTooltip>

      <div className="h-6 w-px bg-slate-200 flex-shrink-0" />

      {/* Memory Pressure */}
      <GlossaryTooltip brief={`Memory pressure: ${memoryPressure.toUpperCase()}`}>
        <div className="flex items-center gap-2 flex-shrink-0">
          <MemoryStick className="h-4 w-4 text-primary" />
          <Badge
            variant="outline"
            className={cn(
              'text-xs capitalize',
              MEMORY_PRESSURE_COLORS[memoryPressure]
            )}
          >
            {memoryPressure}
          </Badge>
        </div>
      </GlossaryTooltip>

      <div className="h-6 w-px bg-slate-200 flex-shrink-0" />

      {/* Adapter Count */}
      <GlossaryTooltip brief={`${adapterCount} total adapters, ${hotAdapters} hot`}>
        <div className="flex items-center gap-2 flex-shrink-0">
          <Layers className="h-4 w-4 text-primary" />
          <span className="text-sm">
            <span className="font-medium">{adapterCount}</span>
            <span className="text-slate-500 hidden sm:inline"> adapters</span>
          </span>
          {hotAdapters > 0 && (
            <Badge variant="secondary" className="text-xs">
              {hotAdapters} hot
            </Badge>
          )}
        </div>
      </GlossaryTooltip>
    </div>
  );
}
