import { useState, useMemo } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { useWorkers, useWorkersHealthSummary } from '@/hooks/system/useSystemMetrics';
import WorkerTable from './WorkerTable';
import WorkerLogsModal from './WorkerLogsModal';
import WorkerIncidentsModal from './WorkerIncidentsModal';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Skeleton } from '@/components/ui/skeleton';
import { useWorkerCacheHealth } from '@/hooks/workers/useWorkerCacheHealth';
import { Server, Activity, AlertTriangle } from 'lucide-react';

export default function WorkersTab() {
  const { workers, isLoading, error, refetch } = useWorkers(undefined, undefined, 'normal');
  const { data: healthSummaries } = useWorkersHealthSummary('normal', true);
  const [selectedWorkerId, setSelectedWorkerId] = useState<string | null>(null);
  const [incidentsWorkerId, setIncidentsWorkerId] = useState<string | null>(null);

  const healthSummariesData = healthSummaries ?? undefined;
  const { summary: cacheSummary } = useWorkerCacheHealth(workers);

  const workerStats = useMemo(() => {
    const running = workers.filter(w => w.status === 'running').length;
    const stopped = workers.filter(w => w.status === 'stopped').length;
    const errored = workers.filter(w => w.status === 'error').length;
    const starting = workers.filter(w => w.status === 'starting').length;
    const stopping = workers.filter(w => w.status === 'stopping').length;
    return { running, stopped, errored, starting, stopping, total: workers.length };
  }, [workers]);

  const healthStats = useMemo(() => {
    if (!healthSummaries || healthSummaries.length === 0) {
      return { healthy: 0, degraded: 0, crashed: 0, unknown: 0, total: 0 };
    }
    const healthy = healthSummaries.filter(h => h.health_status === 'healthy').length;
    const degraded = healthSummaries.filter(h => h.health_status === 'degraded').length;
    const crashed = healthSummaries.filter(h => h.health_status === 'crashed').length;
    const unknown = healthSummaries.filter(h => h.health_status === 'unknown').length;
    return { healthy, degraded, crashed, unknown, total: healthSummaries.length };
  }, [healthSummaries]);

  const totalIncidents = useMemo(() => {
    if (!healthSummaries) return 0;
    return healthSummaries.reduce((sum, h) => sum + h.total_failures, 0);
  }, [healthSummaries]);

  if (error) {
    return (
      <DensityProvider pageKey="system-workers">
        <FeatureLayout
          title="Workers"
          description="Monitor and manage worker processes"
          maxWidth="xl"
        >
          <Card className="border-destructive bg-destructive/10">
            <CardContent className="pt-6">
              <p className="text-destructive">Failed to load workers: {error.message}</p>
            </CardContent>
          </Card>
        </FeatureLayout>
      </DensityProvider>
    );
  }

  return (
    <DensityProvider pageKey="system-workers">
      <FeatureLayout
        title="Workers"
        description="Monitor and manage worker processes"
        maxWidth="xl"
      >
        <div className="space-y-6">
          {/* Worker Capacity Overview */}
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle className="flex items-center gap-2">
                    <Server className="h-5 w-5" />
                    Worker Status
                  </CardTitle>
                  <Badge variant="secondary">{workerStats.total}</Badge>
                </div>
                <CardDescription>Process state distribution</CardDescription>
              </CardHeader>
              <CardContent>
                {isLoading ? (
                  <div className="space-y-2">
                    <Skeleton className="h-4 w-full" />
                    <Skeleton className="h-4 w-3/4" />
                  </div>
                ) : (
                  <div className="space-y-3">
                    <div className="flex items-center justify-between">
                      <span className="text-sm text-muted-foreground">Running</span>
                      <div className="flex items-center gap-2">
                        <Progress value={(workerStats.running / workerStats.total) * 100} className="w-24 h-2" />
                        <Badge variant="success" className="min-w-[3rem] justify-center">{workerStats.running}</Badge>
                      </div>
                    </div>
                    <div className="flex items-center justify-between">
                      <span className="text-sm text-muted-foreground">Stopped</span>
                      <div className="flex items-center gap-2">
                        <Progress value={(workerStats.stopped / workerStats.total) * 100} className="w-24 h-2" />
                        <Badge variant="secondary" className="min-w-[3rem] justify-center">{workerStats.stopped}</Badge>
                      </div>
                    </div>
                    {workerStats.errored > 0 && (
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">Error</span>
                        <div className="flex items-center gap-2">
                          <Progress value={(workerStats.errored / workerStats.total) * 100} className="w-24 h-2" />
                          <Badge variant="destructive" className="min-w-[3rem] justify-center">{workerStats.errored}</Badge>
                        </div>
                      </div>
                    )}
                    {(workerStats.starting > 0 || workerStats.stopping > 0) && (
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">Transitioning</span>
                        <div className="flex items-center gap-2">
                          <Progress value={((workerStats.starting + workerStats.stopping) / workerStats.total) * 100} className="w-24 h-2" />
                          <Badge variant="warning" className="min-w-[3rem] justify-center">{workerStats.starting + workerStats.stopping}</Badge>
                        </div>
                      </div>
                    )}
                  </div>
                )}
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle className="flex items-center gap-2">
                    <Activity className="h-5 w-5" />
                    Health Status
                  </CardTitle>
                  <Badge variant="secondary">{healthStats.total}</Badge>
                </div>
                <CardDescription>Worker health distribution</CardDescription>
              </CardHeader>
              <CardContent>
                {isLoading ? (
                  <div className="space-y-2">
                    <Skeleton className="h-4 w-full" />
                    <Skeleton className="h-4 w-3/4" />
                  </div>
                ) : (
                  <div className="space-y-3">
                    <div className="flex items-center justify-between">
                      <span className="text-sm text-muted-foreground">Healthy</span>
                      <div className="flex items-center gap-2">
                        <Progress value={healthStats.total > 0 ? (healthStats.healthy / healthStats.total) * 100 : 0} className="w-24 h-2" />
                        <Badge variant="success" className="min-w-[3rem] justify-center">{healthStats.healthy}</Badge>
                      </div>
                    </div>
                    {healthStats.degraded > 0 && (
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">Degraded</span>
                        <div className="flex items-center gap-2">
                          <Progress value={(healthStats.degraded / healthStats.total) * 100} className="w-24 h-2" />
                          <Badge variant="warning" className="min-w-[3rem] justify-center">{healthStats.degraded}</Badge>
                        </div>
                      </div>
                    )}
                    {healthStats.crashed > 0 && (
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">Crashed</span>
                        <div className="flex items-center gap-2">
                          <Progress value={(healthStats.crashed / healthStats.total) * 100} className="w-24 h-2" />
                          <Badge variant="destructive" className="min-w-[3rem] justify-center">{healthStats.crashed}</Badge>
                        </div>
                      </div>
                    )}
                    {totalIncidents > 0 && (
                      <div className="flex items-center justify-between pt-2 border-t">
                        <span className="text-sm text-muted-foreground">Total Incidents</span>
                        <Badge variant="destructive" className="min-w-[3rem] justify-center">{totalIncidents}</Badge>
                      </div>
                    )}
                  </div>
                )}
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle className="flex items-center gap-2">
                    <AlertTriangle className="h-5 w-5" />
                    Cache Health
                  </CardTitle>
                  <Badge variant="secondary">{cacheSummary.total}</Badge>
                </div>
                <CardDescription>Model cache utilization</CardDescription>
              </CardHeader>
              <CardContent>
                {isLoading ? (
                  <div className="space-y-2">
                    <Skeleton className="h-4 w-full" />
                    <Skeleton className="h-4 w-3/4" />
                  </div>
                ) : (
                  <div className="space-y-3">
                    <div className="flex items-center justify-between">
                      <span className="text-sm text-muted-foreground">Healthy (&lt;75%)</span>
                      <div className="flex items-center gap-2">
                        <Progress value={cacheSummary.total > 0 ? (cacheSummary.healthy / cacheSummary.total) * 100 : 0} className="w-24 h-2" />
                        <Badge variant="success" className="min-w-[3rem] justify-center">{cacheSummary.healthy}</Badge>
                      </div>
                    </div>
                    {cacheSummary.warning > 0 && (
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">Warning (75-89%)</span>
                        <div className="flex items-center gap-2">
                          <Progress value={(cacheSummary.warning / cacheSummary.total) * 100} className="w-24 h-2" />
                          <Badge variant="warning" className="min-w-[3rem] justify-center">{cacheSummary.warning}</Badge>
                        </div>
                      </div>
                    )}
                    {cacheSummary.critical > 0 && (
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">Critical (90%+)</span>
                        <div className="flex items-center gap-2">
                          <Progress value={(cacheSummary.critical / cacheSummary.total) * 100} className="w-24 h-2" />
                          <Badge variant="destructive" className="min-w-[3rem] justify-center">{cacheSummary.critical}</Badge>
                        </div>
                      </div>
                    )}
                    {cacheSummary.unknown > 0 && (
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">Unknown</span>
                        <div className="flex items-center gap-2">
                          <Progress value={(cacheSummary.unknown / cacheSummary.total) * 100} className="w-24 h-2" />
                          <Badge variant="secondary" className="min-w-[3rem] justify-center">{cacheSummary.unknown}</Badge>
                        </div>
                      </div>
                    )}
                  </div>
                )}
              </CardContent>
            </Card>
          </div>

          <WorkerTable
            workers={workers}
            healthSummaries={healthSummariesData}
            isLoading={isLoading}
            onWorkerSelect={setSelectedWorkerId}
            onIncidentsSelect={setIncidentsWorkerId}
            onRefresh={refetch}
          />

          {selectedWorkerId && (
            <WorkerLogsModal
              workerId={selectedWorkerId}
              open={!!selectedWorkerId}
              onClose={() => setSelectedWorkerId(null)}
            />
          )}

          {incidentsWorkerId && (
            <WorkerIncidentsModal
              workerId={incidentsWorkerId}
              open={!!incidentsWorkerId}
              onClose={() => setIncidentsWorkerId(null)}
            />
          )}
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
