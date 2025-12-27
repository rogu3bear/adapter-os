import { useEffect, useMemo, useState } from 'react';
import { AlertTriangle, ListChecks, Orbit, RefreshCw, Satellite, Sparkles } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { cn } from '@/lib/utils';
import { GalaxyCanvas } from './GalaxyCanvas';
import type { ReasoningSwapEvent, RouterEventStep, RouterRealtimeState, TopologyGraph } from '@/types/topology';

interface ThoughtTopologyPanelProps {
  topology?: TopologyGraph;
  isLoading?: boolean;
  routerState: RouterRealtimeState;
  routerSteps: RouterEventStep[];
  connection: {
    connected: boolean;
    circuitOpen: boolean;
    reconnectAttempts: number;
    error: Error | null;
    reconnect: () => void;
  };
  highlightClusterId?: string | null;
  onRefresh?: () => void;
  onForceCluster?: (clusterId: string) => Promise<void>;
  onDriftChange?: (warning: boolean, distance: number | null) => void;
  reasoningSwaps?: ReasoningSwapEvent[];
}

interface LayoutSnapshot {
  nodes: Record<string, { x: number; y: number; type: 'cluster' | 'adapter'; clusterId?: string }>;
  viewport: { width: number; height: number };
}

const formatScore = (score: number | null) => {
  if (score === null || Number.isNaN(score)) return '—';
  return `${(score * 100).toFixed(1)}%`;
};

export function ThoughtTopologyPanel({
  topology,
  isLoading,
  routerState,
  routerSteps,
  connection,
  highlightClusterId,
  onRefresh,
  onForceCluster,
  onDriftChange,
  reasoningSwaps,
}: ThoughtTopologyPanelProps) {
  const [viewMode, setViewMode] = useState<'galaxy' | 'steps'>('galaxy');
  const [layout, setLayout] = useState<LayoutSnapshot | null>(null);

  const adapterLookup = useMemo(() => {
    const map = new Map<string, string>();
    topology?.adapters.forEach((adapter) => {
      map.set(adapter.id, adapter.name ?? adapter.id);
    });
    return map;
  }, [topology?.adapters]);

  const clusterLookup = useMemo(() => {
    const map = new Map<string, string>();
    topology?.clusters.forEach((cluster) => {
      map.set(cluster.id, cluster.name ?? cluster.id);
    });
    return map;
  }, [topology?.clusters]);

  const ghostTrail = useMemo(
    () => (topology?.predictedPath ?? [])
      .map((node) => node.adapterId ?? node.id)
      .filter((id): id is string => Boolean(id)),
    [topology?.predictedPath]
  );

  const driftDistance = useMemo(() => {
    if (routerState.driftDistance !== null && routerState.driftDistance !== undefined) {
      return routerState.driftDistance;
    }
    if (!layout || !routerState.activeClusterId || !routerState.startingClusterId) return null;
    const start = layout.nodes[routerState.startingClusterId];
    const active = layout.nodes[routerState.activeClusterId];
    if (!start || !active) return null;
    return Math.hypot(active.x - start.x, active.y - start.y);
  }, [layout, routerState.activeClusterId, routerState.driftDistance, routerState.startingClusterId]);

  const driftWarning = useMemo(() => {
    if (driftDistance === null) return false;
    if (!layout) {
      return driftDistance > 80;
    }
    const threshold = Math.min(layout.viewport.width, layout.viewport.height) * 0.35;
    return driftDistance > threshold;
  }, [driftDistance, layout]);

  useEffect(() => {
    if (!onDriftChange) return;
    onDriftChange(driftWarning, driftDistance);
  }, [driftDistance, driftWarning, onDriftChange]);

  const activeClusterLabel = routerState.activeClusterId
    ? clusterLookup.get(routerState.activeClusterId) ?? routerState.activeClusterId
    : 'Idle';
  const startingClusterLabel = routerState.startingClusterId
    ? clusterLookup.get(routerState.startingClusterId) ?? routerState.startingClusterId
    : 'Not set';
  const activeAdapterLabel = routerState.activeAdapterId
    ? adapterLookup.get(routerState.activeAdapterId) ?? routerState.activeAdapterId
    : 'Pending';

  const steps = useMemo(() => routerSteps.slice(-20).reverse(), [routerSteps]);

  return (
    <div className="flex flex-col gap-3 rounded-xl border border-border/60 bg-background/60 p-4 shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div className="space-y-1">
          <div className="flex items-center gap-2 text-sm font-semibold">
            <Sparkles className="h-4 w-4 text-primary" />
            Thought Topology
            {connection.connected ? (
              <Badge variant="secondary" className="gap-1">
                <Satellite className="h-3 w-3" />
                Live
              </Badge>
            ) : (
              <Badge variant="destructive" className="gap-1">
                <AlertTriangle className="h-3 w-3" />
                Offline
              </Badge>
            )}
            {driftWarning && (
              <Badge variant="destructive" className="gap-1 animate-pulse">
                <Orbit className="h-3 w-3" />
                Drift
              </Badge>
            )}
          </div>
          <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
            <span>Cluster: {activeClusterLabel}</span>
            <span className="text-muted-foreground/50">•</span>
            <span>Adapter: {activeAdapterLabel}</span>
            <span className="text-muted-foreground/50">•</span>
            <span>Score: {formatScore(routerState.reasoningScore)}</span>
            <span className="text-muted-foreground/50">•</span>
            <span>Start: {startingClusterLabel}</span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <div className="rounded-full border border-border/60 bg-muted/50 p-1">
            <div className="grid grid-cols-2 gap-1">
              <Button
                variant={viewMode === 'galaxy' ? 'secondary' : 'ghost'}
                size="sm"
                className="h-7 px-2 text-xs"
                onClick={() => setViewMode('galaxy')}
              >
                Galaxy
              </Button>
              <Button
                variant={viewMode === 'steps' ? 'secondary' : 'ghost'}
                size="sm"
                className="h-7 px-2 text-xs"
                onClick={() => setViewMode('steps')}
              >
                Steps
              </Button>
            </div>
          </div>
          <Button variant="ghost" size="icon" onClick={onRefresh} disabled={isLoading}>
            <RefreshCw className={cn('h-4 w-4', isLoading && 'animate-spin')} />
            <span className="sr-only">Refresh topology</span>
          </Button>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2 text-xs">
        <Badge variant="outline" className="gap-1">
          <Satellite className="h-3 w-3" />
          {connection.circuitOpen ? 'Paused (circuit open)' : connection.connected ? 'Streaming' : 'Reconnecting'}
        </Badge>
        <Badge variant="outline" className="gap-1">
          <Orbit className="h-3 w-3" />
          Drift: {driftDistance ? Math.round(driftDistance) : 0}
        </Badge>
        {ghostTrail.length > 0 && (
          <Badge variant="outline" className="gap-1">
            <Sparkles className="h-3 w-3" />
            Ghost path
          </Badge>
        )}
        {connection.error && (
          <Badge variant="destructive" className="gap-1">
            <AlertTriangle className="h-3 w-3" />
            {connection.error.message}
          </Badge>
        )}
        <div className="flex-1" />
        <Button
          variant="outline"
          size="sm"
          disabled={!routerState.activeClusterId || !onForceCluster}
          onClick={() => routerState.activeClusterId && onForceCluster?.(routerState.activeClusterId)}
        >
          Lock cluster
        </Button>
      </div>

      {isLoading ? (
        <Skeleton className="h-[280px] w-full" />
      ) : viewMode === 'galaxy' ? (
        <GalaxyCanvas
          clusters={topology?.clusters ?? []}
          adapters={topology?.adapters ?? []}
          links={topology?.links ?? []}
          activeClusterId={routerState.activeClusterId}
          activeAdapterId={routerState.activeAdapterId}
          highlightClusterId={highlightClusterId}
          driftWarning={driftWarning}
          ghostPath={ghostTrail}
          trail={routerState.trail}
          reasoningSwaps={reasoningSwaps}
          onClusterClick={(clusterId) => {
            void onForceCluster?.(clusterId);
          }}
          onPositionsUpdate={(payload) => setLayout(payload)}
        />
      ) : (
        <div className="max-h-[280px] overflow-y-auto rounded-lg border border-border/60 bg-muted/40 p-3">
          <div className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
            Reasoning Steps
          </div>
          {steps.length === 0 ? (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <ListChecks className="h-4 w-4" />
              Waiting for router events...
            </div>
          ) : (
            <ol className="space-y-2">
              {steps.map((step) => (
                <li
                  key={step.id}
                  className="flex items-start justify-between gap-3 rounded-lg border border-border/70 bg-background/80 p-2 text-sm"
                >
                  <div className="space-y-1">
                    <div className="flex flex-wrap items-center gap-2 text-xs">
                      <Badge variant="outline" className="gap-1">
                        <Sparkles className="h-3 w-3" />
                        {step.adapterId ? adapterLookup.get(step.adapterId) ?? step.adapterId : 'Adapter'}
                      </Badge>
                      {step.clusterId && (
                        <Badge variant="secondary" className="gap-1">
                          <Orbit className="h-3 w-3" />
                          {clusterLookup.get(step.clusterId) ?? step.clusterId}
                        </Badge>
                      )}
                      <span className="text-muted-foreground">
                        {new Date(step.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })}
                      </span>
                    </div>
                    {step.reason && <div className="text-xs text-muted-foreground">{step.reason}</div>}
                  </div>
                  <Badge variant="outline">{formatScore(step.score ?? null)}</Badge>
                </li>
              ))}
            </ol>
          )}
        </div>
      )}
    </div>
  );
}
