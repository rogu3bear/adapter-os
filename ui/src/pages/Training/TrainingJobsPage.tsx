import React, { useState, useCallback } from 'react';
import { Link } from 'react-router-dom';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Brain, Plus, RefreshCw } from 'lucide-react';
import { TrainingJobTable } from './TrainingJobTable';
import { StartTrainingForm } from './StartTrainingForm';
import { TrainingProgress } from './TrainingProgress';
import { useTraining } from '@/hooks/useTraining';
import { useRBAC } from '@/hooks/useRBAC';
import { LastUpdated } from '@/components/ui/last-updated';
import { ConfigPageHeader } from '@/components/ui/page-headers/ConfigPageHeader';
import { PageErrorsProvider, PageErrors, usePageErrors } from '@/components/ui/page-error-boundary';
import type { TrainingJob } from '@/api/training-types';

function TrainingJobsPageContent() {
  const { can } = useRBAC();
  const { errors, addError, clearError } = usePageErrors();

  const [isStartDialogOpen, setIsStartDialogOpen] = useState(false);
  const [selectedJobId, setSelectedJobId] = useState<string | null>(null);

  const {
    data: jobsData,
    isLoading,
    error,
    refetch,
  } = useTraining.useTrainingJobs();

  const { mutateAsync: cancelJob, isPending: isCancelling } = useTraining.useCancelJob({
    onSuccess: () => {
      refetch();
    },
    onError: (err) => {
      addError('cancel-job', err.message);
    },
  });

  const jobs = jobsData?.jobs || [];
  const lastUpdated = new Date();
  const activeJobIds = new Set(jobs.filter(j => j.status === 'running' || j.status === 'pending').map(j => j.id));

  const handleStartTraining = useCallback(() => {
    clearError('start-training');
    setIsStartDialogOpen(true);
  }, [clearError]);

  const handleTrainingStarted = useCallback((jobId: string) => {
    setIsStartDialogOpen(false);
    setSelectedJobId(jobId);
    refetch();
  }, [refetch]);

  const handleCancelJob = useCallback(async (jobId: string) => {
    clearError('cancel-job');
    try {
      await cancelJob(jobId);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to cancel job');
      addError('cancel-job', error.message, () => handleCancelJob(jobId));
    }
  }, [cancelJob, clearError, addError]);

  const handleViewJob = useCallback((job: TrainingJob) => {
    setSelectedJobId(job.id);
  }, []);

  return (
    <div className="space-y-6">
      <ConfigPageHeader
        title="Training Jobs"
        description="Manage LoRA adapter training jobs and monitor progress"
        primaryAction={can('training:start') ? {
          label: 'Start Training',
          icon: Brain,
          onClick: handleStartTraining,
        } : undefined}
      />

      {!can('training:start') && (
        <div className="flex justify-end -mt-4">
          <Button
            disabled
            title="Requires training:start permission"
            className="opacity-50 cursor-not-allowed"
          >
            <Brain className="h-4 w-4 mr-2" />
            Start Training
          </Button>
        </div>
      )}

      <div className="flex items-center justify-between">
        <LastUpdated timestamp={lastUpdated} className="text-sm" />
        <Button
          variant="outline"
          size="sm"
          onClick={() => refetch()}
          disabled={isLoading}
        >
          <RefreshCw className={`h-4 w-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
          Refresh
        </Button>
      </div>

      <PageErrors errors={errors} />

      {error && (
        <Card className="border-destructive">
          <CardContent className="pt-6">
            <p className="text-destructive">Failed to load training jobs: {error.message}</p>
            <Button variant="outline" onClick={() => refetch()} className="mt-2">
              Retry
            </Button>
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Brain className="h-5 w-5" />
            Training Jobs
            {activeJobIds.size > 0 && (
              <span className="text-sm font-normal text-muted-foreground">
                ({activeJobIds.size} active)
              </span>
            )}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <TrainingJobTable
            jobs={jobs}
            isLoading={isLoading}
            onViewJob={handleViewJob}
            onCancelJob={handleCancelJob}
            isCancelling={new Set(isCancelling ? [selectedJobId || ''] : [])}
            canCancel={can('training:cancel')}
          />
        </CardContent>
      </Card>

      {/* Start Training Dialog */}
      <Dialog open={isStartDialogOpen} onOpenChange={setIsStartDialogOpen}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Start New Training Job</DialogTitle>
          </DialogHeader>
          <StartTrainingForm
            onSuccess={handleTrainingStarted}
            onCancel={() => setIsStartDialogOpen(false)}
          />
        </DialogContent>
      </Dialog>

      {/* Job Progress Dialog */}
      <Dialog open={!!selectedJobId} onOpenChange={() => setSelectedJobId(null)}>
        <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
          {selectedJobId && (
            <TrainingProgress
              jobId={selectedJobId}
              onClose={() => setSelectedJobId(null)}
            />
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}

export default function TrainingJobsPage() {
  return (
    <DensityProvider pageKey="training-jobs">
      <FeatureLayout title="Training Jobs" description="Manage training jobs">
        <PageErrorsProvider>
          <TrainingJobsPageContent />
        </PageErrorsProvider>
      </FeatureLayout>
    </DensityProvider>
  );
}
