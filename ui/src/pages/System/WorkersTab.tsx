import { useState } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { useWorkers, useWorkersHealthSummary } from '@/hooks/system/useSystemMetrics';
import WorkerTable from './WorkerTable';
import WorkerLogsModal from './WorkerLogsModal';
import WorkerIncidentsModal from './WorkerIncidentsModal';
import { Card, CardContent } from '@/components/ui/card';

export default function WorkersTab() {
  const { workers, isLoading, error, refetch } = useWorkers(undefined, undefined, 'normal');
  const { data: healthSummaries } = useWorkersHealthSummary('normal', true);
  const [selectedWorkerId, setSelectedWorkerId] = useState<string | null>(null);
  const [incidentsWorkerId, setIncidentsWorkerId] = useState<string | null>(null);

  const healthSummariesData = healthSummaries ?? undefined;

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
