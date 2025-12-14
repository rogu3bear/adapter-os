import React, { useState, useCallback, useMemo, useEffect } from 'react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Brain, Plus, RefreshCw } from 'lucide-react';
import { TrainingJobTable } from './TrainingJobTable';
import { StartTrainingForm } from './StartTrainingForm';
import { TrainingProgress } from './TrainingProgress';
import { useTraining } from '@/hooks/training';
import { useRBAC } from '@/hooks/security/useRBAC';
import { LastUpdated } from '@/components/ui/last-updated';
import { PageErrorsProvider, PageErrors, usePageErrors } from '@/components/ui/page-error-boundary';
import type { TrainingJob } from '@/api/training-types';
import { Badge } from '@/components/ui/badge';
import { parsePreselectParams, removeParams } from '@/utils/urlParams';
import { filterJobsByAdapter } from './TrainingJobsTab';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';

function TrainingJobsPageContent({ preselectedAdapterId, preselectedDatasetId }: { preselectedAdapterId?: string; preselectedDatasetId?: string }) {
  const { can } = useRBAC();
  const { errors, addError, clearError } = usePageErrors();
  const location = useLocation();
  const navigate = useNavigate();

  const [isStartDialogOpen, setIsStartDialogOpen] = useState(false);
  const [selectedJobId, setSelectedJobId] = useState<string | null>(null);
  const [adapterFilter, setAdapterFilter] = useState<string | undefined>(undefined);
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [backendFilter, setBackendFilter] = useState<string>('all');
  const [repoFilter, setRepoFilter] = useState<string>('all');
  const [coremlOnly, setCoremlOnly] = useState<boolean>(false);

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
  const adapterFilteredJobs = useMemo(() => filterJobsByAdapter(jobs, adapterFilter), [adapterFilter, jobs]);
  const backendOptions = useMemo(
    () => Array.from(new Set(jobs.map(j => (j.backend || '').toLowerCase()).filter(Boolean))),
    [jobs]
  );
  const repoOptions = useMemo(
    () => Array.from(new Set(jobs.map(j => j.repo_id).filter(Boolean))) as string[],
    [jobs]
  );
  const filteredJobs = useMemo(() => {
    return adapterFilteredJobs.filter(job => {
      if (statusFilter !== 'all' && job.status !== statusFilter) return false;
      if (backendFilter !== 'all' && (job.backend || '').toLowerCase() !== backendFilter) return false;
      if (repoFilter !== 'all' && job.repo_id !== repoFilter) return false;
      if (coremlOnly) {
        const coremlUsed = Boolean(
          (job.backend || '').toLowerCase().includes('coreml') ||
          (job.backend_device || '').toLowerCase().includes('ane') ||
          job.coreml_export_requested ||
          job.coreml_training_fallback === 'used'
        );
        if (!coremlUsed) return false;
      }
      return true;
    });
  }, [adapterFilteredJobs, statusFilter, backendFilter, repoFilter, coremlOnly]);
  const lastUpdated = new Date();
  const activeJobIds = new Set(jobs.filter(j => j.status === 'running' || j.status === 'pending').map(j => j.id));

  useEffect(() => {
    const parsed = parsePreselectParams(location.search, location.hash);
    if (parsed.adapterId || preselectedAdapterId) {
      setAdapterFilter(parsed.adapterId || preselectedAdapterId);
    }
  }, [location.hash, location.search, preselectedAdapterId]);

  const handleStartTraining = useCallback(() => {
    clearError('start-training');
    setIsStartDialogOpen(true);
  }, [clearError]);

  const handleTrainingStarted = useCallback((jobId: string) => {
    setIsStartDialogOpen(false);
    setSelectedJobId(jobId);
    refetch();
  }, [refetch]);

  const handleClearAdapterFilter = useCallback(() => {
    setAdapterFilter(undefined);
    const nextSearch = removeParams(location.search, ['adapterId']);
    navigate(`${location.pathname}${nextSearch}${location.hash}`, { replace: true });
  }, [location.hash, location.pathname, location.search, navigate]);

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
      <div className="flex items-center justify-between">
        {can('training:start') && (
          <Button onClick={handleStartTraining}>
            <Brain className="h-4 w-4 mr-2" />
            Start Training
          </Button>
        )}
        {!can('training:start') && (
          <Button
            disabled
            title="Requires training:start permission"
            className="opacity-50 cursor-not-allowed"
          >
            <Brain className="h-4 w-4 mr-2" />
            Start Training
          </Button>
        )}
        <div className="flex items-center gap-4">
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

      {adapterFilter && (
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant="secondary">Filtered by adapter {adapterFilter}</Badge>
          <Button variant="ghost" size="sm" onClick={handleClearAdapterFilter}>
            Clear
          </Button>
        </div>
      )}
      <div className="flex flex-wrap items-center gap-3 rounded-md border bg-card/50 p-3">
        <Select value={statusFilter} onValueChange={setStatusFilter}>
          <SelectTrigger className="w-[140px]">
            <SelectValue placeholder="Status" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All status</SelectItem>
            <SelectItem value="pending">Pending</SelectItem>
            <SelectItem value="running">Running</SelectItem>
            <SelectItem value="completed">Completed</SelectItem>
            <SelectItem value="failed">Failed</SelectItem>
            <SelectItem value="cancelled">Cancelled</SelectItem>
          </SelectContent>
        </Select>
        <Select value={backendFilter} onValueChange={setBackendFilter}>
          <SelectTrigger className="w-[170px]">
            <SelectValue placeholder="Backend" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All backends</SelectItem>
            {backendOptions.map(option => (
              <SelectItem key={option} value={option}>
                {option}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Select value={repoFilter} onValueChange={setRepoFilter}>
          <SelectTrigger className="w-[170px]">
            <SelectValue placeholder="Repository" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All repos</SelectItem>
            {repoOptions.map(option => (
              <SelectItem key={option} value={option}>
                {option}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          variant={coremlOnly ? 'default' : 'outline'}
          size="sm"
          onClick={() => setCoremlOnly(prev => !prev)}
        >
          CoreML only
        </Button>
      </div>

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
            jobs={filteredJobs}
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
            preselectedAdapterId={adapterFilter}
            preselectedDatasetId={preselectedDatasetId}
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

export default function TrainingJobsPage({
  preselectedAdapterId,
  preselectedDatasetId,
}: {
  preselectedAdapterId?: string;
  preselectedDatasetId?: string;
}) {
  return (
    <DensityProvider pageKey="training-jobs">
      <FeatureLayout title="Training Jobs" description="Manage training jobs">
        <PageErrorsProvider>
          <TrainingJobsPageContent
            preselectedAdapterId={preselectedAdapterId}
            preselectedDatasetId={preselectedDatasetId}
          />
        </PageErrorsProvider>
      </FeatureLayout>
    </DensityProvider>
  );
}
