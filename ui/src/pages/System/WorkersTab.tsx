import { useState } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { useWorkers } from '@/hooks/useSystemMetrics';
import WorkerTable from './WorkerTable';
import WorkerLogsModal from './WorkerLogsModal';
import { Card, CardContent } from '@/components/ui/card';

export default function WorkersTab() {
  const { workers, isLoading, error, refetch } = useWorkers(undefined, undefined, 'normal');
  const [selectedWorkerId, setSelectedWorkerId] = useState<string | null>(null);

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
            isLoading={isLoading}
            onWorkerSelect={setSelectedWorkerId}
            onRefresh={refetch}
          />

          {selectedWorkerId && (
            <WorkerLogsModal
              workerId={selectedWorkerId}
              open={!!selectedWorkerId}
              onClose={() => setSelectedWorkerId(null)}
            />
          )}
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
