/**
 * SystemKpiGrid - Consolidated system metrics display
 *
 * Replaces SystemOverviewCard, SystemStateCard, TenantsCard, and StacksAdaptersCard
 * with a compact 4-card KPI grid.
 *
 * Responsive: 4-col desktop, 2x2 tablet, 1-col mobile
 */

import React from 'react';
import {
  Cpu,
  MemoryStick,
  HardDrive,
  Layers,
  Users,
  Activity,
  Flame,
  Thermometer,
  Snowflake,
  Lock,
} from 'lucide-react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Skeleton } from '@/components/ui/skeleton';
import { KpiGrid } from '@/components/ui/grid';
import { cn } from '@/lib/utils';
import type { SystemOverview } from '@/api/owner-types';
import type { Tenant } from '@/api/api-types';
import type { Adapter } from '@/api/types';
import type { AdapterStack } from '@/api/adapter-types';

interface SystemStateData {
  memory?: {
    pressure_level?: 'low' | 'medium' | 'high' | 'critical';
    used_mb?: number;
    total_mb?: number;
    headroom_mb?: number;
  };
}

interface SystemKpiGridProps {
  systemOverview?: SystemOverview;
  systemState?: SystemStateData;
  adapters?: Adapter[];
  stacks?: AdapterStack[];
  tenants?: Tenant[];
  isLoading: boolean;
  className?: string;
}

const LIFECYCLE_ICONS = {
  hot: { icon: Flame, color: 'text-red-500', bg: 'bg-red-100' },
  warm: { icon: Thermometer, color: 'text-orange-500', bg: 'bg-orange-100' },
  cold: { icon: Snowflake, color: 'text-blue-500', bg: 'bg-blue-100' },
  resident: { icon: Lock, color: 'text-purple-500', bg: 'bg-purple-100' },
  unloaded: { icon: Activity, color: 'text-slate-400', bg: 'bg-slate-100' },
} as const;

function KpiCardSkeleton() {
  return (
    <Card>
      <CardHeader className="pb-2">
        <Skeleton className="h-4 w-20" />
      </CardHeader>
      <CardContent>
        <Skeleton className="h-8 w-16 mb-2" />
        <Skeleton className="h-2 w-full" />
      </CardContent>
    </Card>
  );
}

export function SystemKpiGrid({
  systemOverview,
  systemState,
  adapters,
  stacks,
  tenants,
  isLoading,
  className,
}: SystemKpiGridProps) {
  if (isLoading) {
    return (
      <KpiGrid className={className}>
        <KpiCardSkeleton />
        <KpiCardSkeleton />
        <KpiCardSkeleton />
        <KpiCardSkeleton />
      </KpiGrid>
    );
  }

  // Calculate resource metrics
  const cpuUsage = systemOverview?.resource_usage?.cpu_usage_percent ?? 0;
  const memoryUsage = systemOverview?.resource_usage?.memory_usage_percent ?? 0;
  const gpuUsage = systemOverview?.resource_usage?.gpu_utilization_percent;

  // Calculate adapter lifecycle stats
  const adapterArray = Array.isArray(adapters) ? adapters : [];
  const adapterStats = {
    total: adapterArray.length,
    hot: adapterArray.filter(a => a.lifecycle_state === 'hot').length,
    warm: adapterArray.filter(a => a.lifecycle_state === 'warm').length,
    cold: adapterArray.filter(a => a.lifecycle_state === 'cold').length,
    resident: adapterArray.filter(a => a.lifecycle_state === 'resident').length,
  };

  // Calculate stack stats
  const stackArray = Array.isArray(stacks) ? stacks : [];
  const activeStack = stackArray.find(s => s.is_default) || stackArray[0];

  // Calculate tenant stats
  const tenantArray = Array.isArray(tenants) ? tenants : [];
  const activeTenants = tenantArray.filter(t => t.status === 'active').length;
  const showTenants = tenantArray.length > 1;

  // Memory pressure from system state
  const memoryPressure = systemState?.memory?.pressure_level || 'low';

  return (
    <KpiGrid className={cn(showTenants ? '' : 'lg:grid-cols-3', className)}>
      {/* Resources Card */}
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-sm font-medium text-slate-600 flex items-center gap-2">
            <Activity className="h-4 w-4" />
            Resources
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {/* CPU */}
          <div className="space-y-1">
            <div className="flex items-center justify-between text-xs">
              <span className="flex items-center gap-1 text-slate-600">
                <Cpu className="h-3 w-3" /> CPU
              </span>
              <span className="font-medium">{cpuUsage.toFixed(0)}%</span>
            </div>
            <Progress value={cpuUsage} className="h-1.5" />
          </div>

          {/* Memory */}
          <div className="space-y-1">
            <div className="flex items-center justify-between text-xs">
              <span className="flex items-center gap-1 text-slate-600">
                <MemoryStick className="h-3 w-3" /> Memory
              </span>
              <span className="font-medium">{memoryUsage.toFixed(0)}%</span>
            </div>
            <Progress
              value={memoryUsage}
              className={cn(
                'h-1.5',
                memoryPressure === 'high' && '[&>div]:bg-orange-500',
                memoryPressure === 'critical' && '[&>div]:bg-red-500'
              )}
            />
          </div>

          {/* GPU (if available) */}
          {gpuUsage !== undefined && gpuUsage !== null && (
            <div className="space-y-1">
              <div className="flex items-center justify-between text-xs">
                <span className="flex items-center gap-1 text-slate-600">
                  <HardDrive className="h-3 w-3" /> GPU
                </span>
                <span className="font-medium">{gpuUsage.toFixed(0)}%</span>
              </div>
              <Progress value={gpuUsage} className="h-1.5" />
            </div>
          )}
        </CardContent>
      </Card>

      {/* Adapters Card */}
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-sm font-medium text-slate-600 flex items-center gap-2">
            <Layers className="h-4 w-4" />
            Adapters
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-baseline gap-1 mb-3">
            <span className="text-2xl font-bold text-slate-900">
              {adapterStats.total}
            </span>
            <span className="text-sm text-slate-500">total</span>
          </div>

          {/* Lifecycle badges */}
          <div className="flex flex-wrap gap-1.5">
            {Object.entries(LIFECYCLE_ICONS).map(([state, config]) => {
              const count = adapterStats[state as keyof typeof adapterStats];
              if (typeof count !== 'number' || count === 0) return null;

              const Icon = config.icon;
              return (
                <Badge
                  key={state}
                  variant="outline"
                  className={cn('text-xs gap-1', config.bg, 'border-transparent')}
                >
                  <Icon className={cn('h-3 w-3', config.color)} />
                  {count} {state}
                </Badge>
              );
            })}
            {adapterStats.total === 0 && (
              <span className="text-xs text-slate-400">No adapters</span>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Stacks Card */}
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-sm font-medium text-slate-600 flex items-center gap-2">
            <Layers className="h-4 w-4" />
            Stacks
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-baseline gap-1 mb-3">
            <span className="text-2xl font-bold text-slate-900">
              {stackArray.length}
            </span>
            <span className="text-sm text-slate-500">stacks</span>
          </div>

          {activeStack ? (
            <div className="flex items-center gap-2">
              <Badge variant="default" className="text-xs">
                Active
              </Badge>
              <span className="text-sm text-slate-700 truncate">
                {activeStack.name}
              </span>
              <Badge variant="secondary" className="text-xs">
                {activeStack.adapter_ids?.length || activeStack.adapters?.length || 0} adapters
              </Badge>
            </div>
          ) : (
            <span className="text-xs text-slate-400">No active stack</span>
          )}
        </CardContent>
      </Card>

      {/* Tenants Card - Only show if >1 tenant */}
      {showTenants && (
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-slate-600 flex items-center gap-2">
              <Users className="h-4 w-4" />
              Organizations
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex items-baseline gap-1 mb-3">
              <span className="text-2xl font-bold text-slate-900">
                {tenantArray.length}
              </span>
              <span className="text-sm text-slate-500">tenants</span>
            </div>

            <div className="flex items-center gap-2">
              <Badge
                variant="outline"
                className="text-xs bg-green-100 text-green-700 border-transparent"
              >
                {activeTenants} active
              </Badge>
              {tenantArray.length - activeTenants > 0 && (
                <Badge
                  variant="outline"
                  className="text-xs bg-slate-100 text-slate-600 border-transparent"
                >
                  {tenantArray.length - activeTenants} inactive
                </Badge>
              )}
            </div>
          </CardContent>
        </Card>
      )}
    </KpiGrid>
  );
}
