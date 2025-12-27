import { useMemo, useState, useCallback } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Skeleton } from '@/components/ui/skeleton';
import { Button } from '@/components/ui/button';
import { DataTable } from '@/components/shared/DataTable/DataTable';
import { type Column } from '@/components/shared/DataTable/types';
import { useMemoryUsage, useMemoryOperations } from '@/hooks/system/useSystemMetrics';
import { useToast } from '@/hooks/use-toast';
import { Trash2, Wifi, WifiOff, Activity } from 'lucide-react';
import { useMetricsStream } from '@/hooks/streaming/useStreamingEndpoints';
import type { MetricsSnapshotEvent } from '@/api/streaming-types';
import { useUiMode } from '@/hooks/ui/useUiMode';
import { UiMode } from '@/config/ui-mode';
import { useAuth } from '@/providers/CoreProviders';

interface AdapterMemoryInfo {
  id: string;
  name: string;
  memory_usage_mb: number;
  state: string;
  pinned: boolean;
  category: string;
}

export default function MemoryTab() {
  const [useSSE, setUseSSE] = useState(true);
  const [liveMemoryPercent, setLiveMemoryPercent] = useState<number | null>(null);
  const { uiMode } = useUiMode();
  const { user } = useAuth();
  const isKernelMode = uiMode === UiMode.Kernel && user?.role?.toLowerCase() === 'developer';

  // SSE stream for live memory percentage
  const { error: sseError, connected: sseConnected, reconnect } = useMetricsStream({
    enabled: useSSE,
    onMessage: useCallback((event: any) => {
      if ('system' in event) {
        const metricsEvent = event as MetricsSnapshotEvent;
        if (metricsEvent.system?.memory_percent !== undefined) {
          setLiveMemoryPercent(metricsEvent.system.memory_percent);
        }
      }
    }, []),
  });

  const { data: memoryData, isLoading, refetch } = useMemoryUsage('normal');
  const { evictAdapter } = useMemoryOperations();
  const { toast } = useToast();

  const handleEvict = useCallback(async (adapterId: string) => {
    if (!confirm(`Are you sure you want to remove adapter ${adapterId} from memory?`)) {
      return;
    }

    try {
      await evictAdapter.execute(adapterId);
      toast({
        title: 'Adapter Removed',
        description: `Adapter ${adapterId} has been removed from memory`,
      });
      refetch();
    } catch (error) {
      toast({
        title: 'Removal Failed',
        description: error instanceof Error ? error.message : 'Unknown error',
        variant: 'destructive',
      });
    }
  }, [evictAdapter, toast, refetch]);

  const memoryUsagePercent = useMemo(() => {
    // Use live SSE data if available and connected
    if (useSSE && sseConnected && liveMemoryPercent !== null) {
      return liveMemoryPercent;
    }
    // Otherwise calculate from memoryData
    if (!memoryData) return 0;
    const used = memoryData.total_memory_mb - memoryData.available_memory_mb;
    return (used / memoryData.total_memory_mb) * 100;
  }, [memoryData, useSSE, sseConnected, liveMemoryPercent]);

  const pressureVariant = useMemo(() => {
    if (!memoryData) return 'secondary';
    switch (memoryData.memory_pressure_level) {
      case 'low':
        return 'success';
      case 'medium':
        return 'warning';
      case 'high':
      case 'critical':
        return 'destructive';
      default:
        return 'secondary';
    }
  }, [memoryData]);

  const memoryBlocks = useMemo(() => {
    if (!memoryData) return [];
    const total = memoryData.total_memory_mb || 1;
    return memoryData.adapters.map((adapter, idx) => {
      const usage = adapter.memory_usage_mb || 0;
      const percent = Math.max(2, Math.min(100, (usage / total) * 100));
      return {
        id: adapter.id || `adapter-${idx}`,
        label: adapter.name || adapter.id,
        memoryMb: usage,
        percent,
        rank: (adapter as { rank?: number }).rank,
        pinned: adapter.pinned,
        state: adapter.state,
      };
    });
  }, [memoryData]);

  const columns = useMemo<Column<AdapterMemoryInfo>[]>(
    () => [
      {
        id: 'id',
        accessorKey: 'id',
        header: 'Adapter ID',
        cell: ({ row }) => <span className="font-mono text-sm">{row.id}</span>,
      },
      {
        id: 'name',
        accessorKey: 'name',
        header: 'Name',
        cell: ({ row }) => <span className="text-sm">{row.name}</span>,
      },
      {
        id: 'memory_usage_mb',
        accessorKey: 'memory_usage_mb',
        header: 'Memory Usage',
        cell: ({ row }) => `${row.memory_usage_mb.toFixed(2)} MB`,
      },
      {
        id: 'state',
        accessorKey: 'state',
        header: 'State',
        cell: ({ row }) => (
          <Badge variant="outline" className="capitalize">
            {row.state}
          </Badge>
        ),
      },
      {
        id: 'pinned',
        accessorKey: 'pinned',
        header: 'Protected',
        cell: ({ row }) => (
          <Badge variant={row.pinned ? 'default' : 'secondary'}>
            {row.pinned ? 'Protected' : '-'}
          </Badge>
        ),
      },
      {
        id: 'actions',
        header: 'Actions',
        cell: ({ row }) => {
          return (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => handleEvict(row.id)}
              disabled={evictAdapter.isLoading || row.pinned}
              title={row.pinned ? 'Cannot remove protected adapter' : 'Remove adapter from memory'}
            >
              <Trash2 className="h-4 w-4 mr-2" />
              Remove
            </Button>
          );
        },
      },
    ],
    [evictAdapter.isLoading, handleEvict]
  );

  if (isLoading) {
    return (
      <DensityProvider pageKey="system-memory">
        <FeatureLayout
          title="Memory"
          description="Monitor memory usage and manage adapters"
          maxWidth="xl"
        >
          <div className="space-y-6">
            <Skeleton className="h-48 w-full" />
            <Skeleton className="h-64 w-full" />
          </div>
        </FeatureLayout>
      </DensityProvider>
    );
  }

  if (!memoryData) {
    return (
      <DensityProvider pageKey="system-memory">
        <FeatureLayout
          title="Memory"
          description="Monitor memory usage and manage adapters"
          maxWidth="xl"
        >
          <Card className="border-destructive bg-destructive/10">
            <CardContent className="pt-6">
              <p className="text-destructive">Failed to load memory information</p>
            </CardContent>
          </Card>
        </FeatureLayout>
      </DensityProvider>
    );
  }

  return (
    <DensityProvider pageKey="system-memory">
      <FeatureLayout
        title="Memory"
        description="Monitor memory usage and manage adapters"
        maxWidth="xl"
        headerActions={
          <div className="flex items-center gap-2">
            {useSSE && sseError && (
              <Button variant="outline" size="sm" onClick={reconnect}>
                Reconnect
              </Button>
            )}
            <Button
              variant="outline"
              size="sm"
              onClick={() => setUseSSE(!useSSE)}
            >
              {useSSE ? 'Switch to Polling' : 'Switch to Live'}
            </Button>
          </div>
        }
      >
        <div className="space-y-6">
      {/* Live Connection Status */}
      <Card>
        <CardContent className="pt-6">
          <div className="flex items-center gap-3">
            {useSSE && (
              <>
                {sseConnected ? (
                  <>
                    <Wifi className="h-5 w-5 text-green-500" />
                    <div>
                      <div className="font-medium">Live Updates Active</div>
                      <div className="text-sm text-muted-foreground">
                        Memory metrics updating in real-time
                      </div>
                    </div>
                  </>
                ) : sseError ? (
                  <>
                    <WifiOff className="h-5 w-5 text-destructive" />
                    <div>
                      <div className="font-medium text-destructive">Connection Lost</div>
                      <div className="text-sm text-muted-foreground">
                        {sseError?.message}
                      </div>
                    </div>
                  </>
                ) : (
                  <>
                    <Activity className="h-5 w-5 animate-pulse text-yellow-500" />
                    <div>
                      <div className="font-medium">Connecting...</div>
                      <div className="text-sm text-muted-foreground">
                        Establishing live connection
                      </div>
                    </div>
                  </>
                )}
              </>
            )}
            {!useSSE && (
              <div>
                <div className="font-medium">Polling Mode</div>
                <div className="text-sm text-muted-foreground">
                  Updates every few seconds
                </div>
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Memory Overview */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card>
          <CardHeader>
            <CardDescription>Total Memory</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold">
              {(memoryData.total_memory_mb / 1024).toFixed(2)} GB
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardDescription>Available Memory</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold">
              {(memoryData.available_memory_mb / 1024).toFixed(2)} GB
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardDescription>Pressure Level</CardDescription>
          </CardHeader>
          <CardContent>
            <Badge variant={pressureVariant} className="text-lg px-4 py-2">
              {memoryData.memory_pressure_level.toUpperCase()}
            </Badge>
          </CardContent>
        </Card>
      </div>

      {/* Memory Usage Chart */}
      <Card>
        <CardHeader>
          <CardTitle>Memory Usage</CardTitle>
          <CardDescription>
            {memoryUsagePercent.toFixed(1)}% of total memory in use
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Progress value={memoryUsagePercent} className="h-4" />
          <div className="mt-4 grid grid-cols-2 gap-4 text-sm">
            <div>
              <span className="text-muted-foreground">Used:</span>{' '}
              <span className="font-semibold">
                {((memoryData.total_memory_mb - memoryData.available_memory_mb) / 1024).toFixed(2)} GB
              </span>
            </div>
            <div>
              <span className="text-muted-foreground">Free:</span>{' '}
              <span className="font-semibold">
                {(memoryData.available_memory_mb / 1024).toFixed(2)} GB
              </span>
            </div>
          </div>
        </CardContent>
      </Card>

      {isKernelMode && (
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle>VRAM Map</CardTitle>
                <CardDescription>Hot-Swap visualization of adapter blocks</CardDescription>
              </div>
              <Badge variant="secondary" className="uppercase text-[11px]">Hot-Swap</Badge>
            </div>
          </CardHeader>
          <CardContent>
            <div className="flex flex-wrap gap-2">
              {memoryBlocks.length === 0 ? (
                <div className="text-sm text-muted-foreground">No adapters in VRAM.</div>
              ) : (
                memoryBlocks.map((block) => (
                  <div
                    key={block.id}
                    className="min-w-[180px] flex-1 rounded-md border border-border/60 bg-slate-950 text-slate-50 p-3 shadow-sm"
                    style={{ flexBasis: `${Math.min(block.percent * 1.5, 100)}%` }}
                  >
                    <div className="flex items-center justify-between text-[11px] uppercase tracking-wide">
                      <span className="truncate" title={block.label}>{block.label}</span>
                      <span className="text-slate-300">{block.percent.toFixed(1)}%</span>
                    </div>
                    <div className="mt-2 h-2 rounded-sm bg-slate-800">
                      <div className="h-full rounded-sm bg-emerald-400" style={{ width: `${Math.min(100, block.percent)}%` }} />
                    </div>
                    <div className="mt-2 text-[11px] text-slate-200">
                      {block.memoryMb.toFixed(2)} MB • Rank {block.rank ?? 'n/a'} {block.pinned ? '• pinned' : ''}
                    </div>
                    <div className="text-[11px] text-slate-400">
                      state: {block.state}
                    </div>
                  </div>
                ))
              )}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Adapter Memory Table */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle>Adapters in Memory</CardTitle>
              <CardDescription>
                {memoryData.adapters.length} adapter(s) currently loaded
              </CardDescription>
            </div>
            <Button variant="outline" onClick={() => refetch()}>
              Refresh
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          <DataTable
            columns={columns}
            data={memoryData.adapters}
            getRowId={(row) => row.id}
            isLoading={isLoading}
            globalFilter=""
          />
        </CardContent>
      </Card>
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
