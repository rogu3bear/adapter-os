/**
 * Dashboard KPI Cards Component
 *
 * Displays key performance indicators in a grid layout including
 * node count, tenant count, adapter count, sessions, and performance metrics.
 */

import React, { memo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { KpiGrid } from '@/components/ui/grid';
import { Server, Users, Code, Zap } from 'lucide-react';

/**
 * Props for the DashboardKpiCards component
 */
export interface DashboardKpiCardsProps {
  /** Number of online inference nodes */
  nodeCount: number;
  /** Number of active tenants/workspaces */
  tenantCount: number;
  /** Number of registered LoRA adapters */
  adapterCount: number;
  /** Number of active inference sessions */
  activeSessions: number;
  /** Tokens processed per second */
  tokensPerSecond: number;
  /** 95th percentile latency in milliseconds */
  latencyP95: number;
  /** Whether data is currently loading */
  loading: boolean;
}

/**
 * KPI cards grid displaying key system metrics.
 *
 * Shows inference nodes, active workspaces, LoRA adapters,
 * and performance metrics in a responsive grid layout.
 */
export const DashboardKpiCards = memo(function DashboardKpiCards({
  nodeCount,
  tenantCount,
  adapterCount,
  activeSessions,
  tokensPerSecond,
  latencyP95,
  loading,
}: DashboardKpiCardsProps) {
  return (
    <KpiGrid>
      {/* Inference Nodes */}
      <Card className="card-standard">
        <CardHeader className="flex-between pb-2">
          <GlossaryTooltip termId="compute-nodes">
            <CardTitle className="text-sm font-medium cursor-help">
              Inference Nodes
            </CardTitle>
          </GlossaryTooltip>
          <Server className="icon-standard text-muted-foreground" />
        </CardHeader>
        <CardContent>
          <div className="text-2xl font-bold text-green-600">
            {loading ? <Skeleton className="h-6 w-16" /> : nodeCount}
          </div>
          <p className="text-xs text-muted-foreground">
            {loading ? 'Loading nodes...' : `${nodeCount} nodes online`}
          </p>
        </CardContent>
      </Card>

      {/* Active Workspaces */}
      <Card className="card-standard">
        <CardHeader className="flex-between pb-2">
          <GlossaryTooltip termId="active-tenants">
            <CardTitle className="text-sm font-medium cursor-help">
              Active Workspaces
            </CardTitle>
          </GlossaryTooltip>
          <Users className="icon-standard text-muted-foreground" />
        </CardHeader>
        <CardContent>
          <div className="text-2xl font-bold text-blue-600">
            {loading ? <Skeleton className="h-6 w-16" /> : tenantCount}
          </div>
          <p className="text-xs text-muted-foreground">
            {loading ? 'Loading workspaces...' : 'All workspaces operational'}
          </p>
        </CardContent>
      </Card>

      {/* LoRA Adapters */}
      <Card className="card-standard">
        <CardHeader className="flex-between pb-2">
          <GlossaryTooltip termId="adapter-count">
            <CardTitle className="text-sm font-medium cursor-help">
              LoRA Adapters
            </CardTitle>
          </GlossaryTooltip>
          <Code className="icon-standard text-muted-foreground" />
        </CardHeader>
        <CardContent>
          <div className="text-2xl font-bold text-purple-600">{adapterCount}</div>
          <GlossaryTooltip termId="active-sessions">
            <p className="text-xs text-muted-foreground cursor-help">
              {activeSessions} active sessions
            </p>
          </GlossaryTooltip>
        </CardContent>
      </Card>

      {/* Performance */}
      <Card className="card-standard">
        <CardHeader className="flex-between pb-2">
          <GlossaryTooltip termId="tokens-per-second">
            <CardTitle className="text-sm font-medium cursor-help">
              Performance
            </CardTitle>
          </GlossaryTooltip>
          <Zap className="icon-standard text-muted-foreground" />
        </CardHeader>
        <CardContent>
          <div className="text-2xl font-bold text-green-600">
            {tokensPerSecond.toFixed(0)}
          </div>
          <GlossaryTooltip termId="latency-p95">
            <p className="text-xs text-muted-foreground cursor-help">
              tokens/sec (p95: {latencyP95.toFixed(0)}ms)
            </p>
          </GlossaryTooltip>
        </CardContent>
      </Card>
    </KpiGrid>
  );
});
