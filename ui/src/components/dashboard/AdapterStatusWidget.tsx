import React, { useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Layers, TrendingUp, Activity, Loader2, AlertCircle } from 'lucide-react';
import { Skeleton } from '@/components/ui/skeleton';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { useAdapters } from '@/pages/Adapters/useAdapters';
import { useMemoryUsage } from '@/hooks/useSystemMetrics';
import type { AdapterState } from '@/api/adapter-types';

interface AdapterStateCount {
  state: string;
  count: number;
  color: string;
}

const STATE_COLORS: Record<string, string> = {
  hot: 'bg-red-500',
  warm: 'bg-orange-500',
  cold: 'bg-blue-500',
  unloaded: 'bg-gray-400',
  resident: 'bg-purple-500',
  loading: 'bg-yellow-500'
};

export function AdapterStatusWidget() {
  // Fetch adapters data with auto-refresh (refetches every 60 seconds)
  const { data: adaptersData, isLoading: isLoadingAdapters, error: adaptersError, refetch } = useAdapters();

  // Fetch memory usage data with auto-refresh
  const { data: memoryData, isLoading: isLoadingMemory } = useMemoryUsage('normal', true);

  // Calculate state distribution from adapters
  const stateDistribution = useMemo(() => {
    if (!adaptersData?.adapters) {
      return [];
    }

    const stateCounts = new Map<string, number>();
    adaptersData.adapters.forEach((adapter) => {
      const state = adapter.current_state || adapter.lifecycle_state || 'unloaded';
      stateCounts.set(state, (stateCounts.get(state) || 0) + 1);
    });

    // Create distribution array - only include states that have adapters
    const states: AdapterState[] = ['hot', 'warm', 'cold', 'resident', 'unloaded'];
    return states
      .map(state => ({
        state,
        count: stateCounts.get(state) || 0,
        color: STATE_COLORS[state] || 'bg-gray-300'
      }))
      .filter(item => item.count > 0);
  }, [adaptersData]);

  // Calculate total and active adapters
  const totalAdapters = useMemo(() =>
    stateDistribution.reduce((sum, s) => sum + s.count, 0),
    [stateDistribution]
  );

  const activeAdapters = useMemo(() =>
    stateDistribution
      .filter(s => ['hot', 'warm', 'resident'].includes(s.state))
      .reduce((sum, s) => sum + s.count, 0),
    [stateDistribution]
  );

  // Calculate memory usage percentage
  const memoryUsage = useMemo(() => {
    if (!memoryData) return 0;
    const total = memoryData.total_memory_mb || 0;
    const available = memoryData.available_memory_mb || 0;
    if (total === 0) return 0;
    return Math.round(((total - available) / total) * 100);
  }, [memoryData]);

  // Calculate average activation rate (percentage of adapters that are active)
  const avgActivationRate = useMemo(() => {
    if (!adaptersData?.adapters || adaptersData.adapters.length === 0) return 0;

    const activeCount = adaptersData.adapters.filter(
      a => ['hot', 'warm', 'resident'].includes(a.current_state || a.lifecycle_state || '')
    ).length;

    return adaptersData.adapters.length > 0
      ? activeCount / adaptersData.adapters.length
      : 0;
  }, [adaptersData]);

  const isLoading = isLoadingAdapters || isLoadingMemory;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between">
          <span>Adapter Status</span>
          {!isLoading && !adaptersError && (
            <Badge variant="outline">
              {activeAdapters} Active
            </Badge>
          )}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {isLoading ? (
          <>
            <Skeleton className="h-20 w-full" />
            <Skeleton className="h-16 w-full" />
            <Skeleton className="h-16 w-full" />
          </>
        ) : adaptersError ? (
          <Alert variant="destructive">
            <AlertCircle className="h-4 w-4" />
            <AlertDescription className="flex items-center justify-between">
              <span>Failed to load adapter status</span>
              <Button
                variant="outline"
                size="sm"
                onClick={() => refetch()}
              >
                Retry
              </Button>
            </AlertDescription>
          </Alert>
        ) : totalAdapters === 0 ? (
          <div className="text-center py-8">
            <Layers className="h-12 w-12 text-muted-foreground mx-auto mb-2 opacity-20" />
            <p className="text-sm text-muted-foreground">No adapters found</p>
          </div>
        ) : (
          <>
            {/* State Distribution */}
            <div>
              <div className="flex items-center justify-between text-sm mb-2">
                <span className="text-muted-foreground">Lifecycle States</span>
                <span className="font-medium">{totalAdapters} total</span>
              </div>
              <div className="flex h-2 rounded-full overflow-hidden bg-gray-100">
                {stateDistribution.map((state) => (
                  state.count > 0 && (
                    <div
                      key={state.state}
                      className={state.color}
                      style={{ width: `${(state.count / totalAdapters) * 100}%` }}
                      title={`${state.state}: ${state.count}`}
                    />
                  )
                ))}
              </div>
              <div className="grid grid-cols-2 gap-2 mt-2">
                {stateDistribution.map((state) => (
                  <div key={state.state} className="flex items-center gap-2 text-xs">
                    <div className={`w-2 h-2 rounded-full ${state.color}`} />
                    <span className="text-muted-foreground capitalize">{state.state}:</span>
                    <span className="font-medium">{state.count}</span>
                  </div>
                ))}
              </div>
            </div>

            {/* Memory Usage */}
            <div>
              <div className="flex items-center gap-2 mb-2">
                <Layers className="h-4 w-4 text-muted-foreground" />
                <span className="text-sm text-muted-foreground">Memory Usage</span>
              </div>
              <Progress value={memoryUsage} className="h-2" />
              <p className="text-xs text-muted-foreground mt-1">
                {memoryUsage.toFixed(1)}% of adapter memory in use
              </p>
            </div>

            {/* Activation Rate */}
            <div className="flex items-center justify-between p-3 bg-muted rounded-lg">
              <div className="flex items-center gap-2">
                <Activity className="h-4 w-4 text-muted-foreground" />
                <span className="text-sm font-medium">Avg Activation</span>
              </div>
              <div className="flex items-center gap-1">
                <span className="text-lg font-semibold">{(avgActivationRate * 100).toFixed(1)}%</span>
                <TrendingUp className="h-4 w-4 text-gray-600" />
              </div>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
