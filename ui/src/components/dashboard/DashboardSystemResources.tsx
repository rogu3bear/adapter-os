/**
 * Dashboard System Resources Component
 *
 * Displays system resource usage including CPU, memory, disk, and network.
 * Shows real-time connection status and live metrics.
 */

import React, { memo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { Cpu, HardDrive, Network } from 'lucide-react';

/**
 * Props for the DashboardSystemResources component
 */
export interface DashboardSystemResourcesProps {
  /** CPU usage percentage (0-100) */
  cpuUsage: number;
  /** Memory usage percentage (0-100) */
  memoryUsage: number;
  /** Disk usage percentage (0-100) */
  diskUsage: number;
  /** Network bandwidth in MB/s as string */
  networkBandwidth: string;
  /** Whether real-time SSE connection is active */
  connected: boolean;
  /** Whether system metrics are available */
  hasMetrics: boolean;
}

/**
 * System resources card showing CPU, memory, disk, and network usage.
 *
 * Displays a "Live" badge when connected to real-time updates via SSE.
 */
export const DashboardSystemResources = memo(function DashboardSystemResources({
  cpuUsage,
  memoryUsage,
  diskUsage,
  networkBandwidth,
  connected,
  hasMetrics,
}: DashboardSystemResourcesProps) {
  return (
    <SectionErrorBoundary sectionName="System Resources">
      <Card className="card-standard">
        <CardHeader>
          <CardTitle>System Resources</CardTitle>
        </CardHeader>
        <CardContent className="space-y-6">
          {/* CPU Usage */}
          <div className="space-y-2">
            <div className="flex justify-between items-center mb-2">
              <div className="flex items-center gap-2">
                <Cpu className="h-5 w-5 text-muted-foreground" />
                <GlossaryTooltip termId="cpu-usage">
                  <span className="text-sm font-medium cursor-help">CPU Usage</span>
                </GlossaryTooltip>
                {connected && (
                  <Badge variant="outline" className="text-xs px-2 py-0 h-5">
                    <span className="relative flex h-2 w-2 mr-1">
                      <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                      <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
                    </span>
                    Live
                  </Badge>
                )}
              </div>
              <span className="text-sm font-semibold">
                {hasMetrics ? `${cpuUsage.toFixed(1)}%` : '--'}
              </span>
            </div>
            <Progress value={cpuUsage} className="h-3 transition-all duration-500" />
          </div>

          {/* Memory Usage */}
          <div className="space-y-2">
            <div className="flex justify-between items-center mb-2">
              <div className="flex items-center gap-2">
                <HardDrive className="h-5 w-5 text-muted-foreground" />
                <GlossaryTooltip termId="memory-usage">
                  <span className="text-sm font-medium cursor-help">Memory Usage</span>
                </GlossaryTooltip>
              </div>
              <span className="text-sm font-semibold">
                {hasMetrics ? `${memoryUsage.toFixed(1)}%` : '--'}
              </span>
            </div>
            <Progress value={memoryUsage} className="h-3 transition-all duration-500" />
          </div>

          {/* Disk Usage */}
          <div className="space-y-2">
            <div className="flex justify-between items-center mb-2">
              <div className="flex items-center gap-2">
                <HardDrive className="h-5 w-5 text-muted-foreground" />
                <GlossaryTooltip termId="disk-usage">
                  <span className="text-sm font-medium cursor-help">Disk Usage</span>
                </GlossaryTooltip>
              </div>
              <span className="text-sm font-semibold">
                {hasMetrics ? `${diskUsage.toFixed(1)}%` : '--'}
              </span>
            </div>
            <Progress value={diskUsage} className="h-3 transition-all duration-500" />
          </div>

          {/* Network Bandwidth */}
          <div className="space-y-2">
            <div className="flex justify-between items-center mb-2">
              <div className="flex items-center gap-2">
                <Network className="h-5 w-5 text-muted-foreground" />
                <GlossaryTooltip termId="network-bandwidth">
                  <span className="text-sm font-medium cursor-help">Network Bandwidth</span>
                </GlossaryTooltip>
              </div>
              <span className="text-sm font-semibold">
                {hasMetrics ? `${networkBandwidth} MB/s` : '--'}
              </span>
            </div>
            <Progress
              value={Math.min(parseFloat(networkBandwidth), 100)}
              className="h-3 transition-all duration-500"
            />
          </div>
        </CardContent>
      </Card>
    </SectionErrorBoundary>
  );
});
