import { useState, useCallback, useMemo, useEffect } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Brain, RefreshCw } from 'lucide-react';
import { TrainingJobTable } from './TrainingJobTable';
import { TrainingJobFilters, type TrainingJobFilterKey } from './TrainingJobFilters';
import { StartTrainingForm } from './StartTrainingForm';
import { TrainingProgress } from './TrainingProgress';
import { useTraining } from '@/hooks/training';
import { useRBAC } from '@/hooks/security/useRBAC';
import { useFilter, type FilterValue } from '@/hooks/ui/useFilter';
import { LastUpdated } from '@/components/ui/last-updated';
import { PageErrors, usePageErrors } from '@/components/ui/page-error-boundary';
import { withErrorBoundary } from '@/components/WithErrorBoundary';
import type { TrainingJob } from '@/api/training-types';
import { Badge } from '@/components/ui/badge';
import { parsePreselectParams, removeParams } from '@/utils/urlParams';

// Custom filter function to search across multiple fields
function searchJobFields<T>(item: T, searchValue: FilterValue): boolean {
  if (!searchValue || typeof searchValue !== 'string') return true;
  const job = item as TrainingJob;
  const search = searchValue.toLowerCase();
  const adapterName = (job.adapter_name || '').toLowerCase();
  const jobId = job.id.toLowerCase();
  return adapterName.includes(search) || jobId.includes(search);
}

// Custom filter function for date range on created_at
function filterByDateRange<T>(item: T, dateRange: FilterValue): boolean {
  if (!dateRange || typeof dateRange !== 'object' || !('start' in dateRange)) return true;
  const job = item as TrainingJob;
  const { start, end } = dateRange as { start: string; end: string };
  const jobDate = job.created_at || job.started_at;
  if (!jobDate) return true;

  const jobTime = new Date(jobDate).getTime();

  if (start) {
    const startTime = new Date(start).setHours(0, 0, 0, 0);
    if (jobTime < startTime) return false;
  }
  if (end) {
    const endTime = new Date(end).setHours(23, 59, 59, 999);
    if (jobTime > endTime) return false;
  }
  return true;
}

export function filterJobsByAdapter(jobs: TrainingJob[], adapterId?: string): TrainingJob[] {
  if (!adapterId) return jobs;
  const normalized = adapterId.toLowerCase();
  return jobs.filter((job) => (job.adapter_id || '').toLowerCase() === normalized || (job.adapter_name || '').toLowerCase() === normalized);
}

export function TrainingJobsTab({
  preselectedAdapterId,
  preselectedDatasetId,
}: {
  preselectedAdapterId?: string;
  preselectedDatasetId?: string;
}) {
  const { can } = useRBAC();
  const { errors, addError, clearError } = usePageErrors();
  const location = useLocation();
  const navigate = useNavigate();

  const [isStartDialogOpen, setIsStartDialogOpen] = useState(false);
  const [selectedJobId, setSelectedJobId] = useState<string | null>(null);
  const [adapterFilter, setAdapterFilter] = useState<string | undefined>(undefined);

  const {
    data: jobsData,
    isLoading,
    error,
    refetch,
  } = useTraining.useTrainingJobs();

  const jobs = useMemo(() => jobsData?.jobs || [], [jobsData]);

  useEffect(() => {
    const parsed = parsePreselectParams(location.search, location.hash);
    if (parsed.adapterId || preselectedAdapterId) {
      setAdapterFilter(parsed.adapterId || preselectedAdapterId);
    }
  }, [location.hash, location.search, preselectedAdapterId]);

  // Filter configuration with URL sync
  const adapterFilteredJobs = useMemo(() => filterJobsByAdapter(jobs, adapterFilter), [adapterFilter, jobs]);

  const {
    filters,
    filteredData: filteredJobs,
    setFilter,
    clearFilters,
    activeFilterCount,
  } = useFilter<TrainingJob, TrainingJobFilterKey>({
    data: adapterFilteredJobs,
    filterConfigs: {
      search: {
        type: 'search',
        placeholder: 'Search by adapter name or job ID...',
        customFilter: searchJobFields,
      },
      status: {
        type: 'select',
        options: ['pending', 'running', 'completed', 'failed', 'cancelled'],
      },
      dateRange: {
        type: 'dateRange',
        customFilter: filterByDateRange,
      },
    },
    syncToUrl: true,
    urlPrefix: 'job_',
  });

  // Handle errors outside of query options (React Query v5 compatibility)
  if (error) {
    addError('fetch-jobs', error.message, () => refetch());
  }

  const { mutateAsync: cancelJob, isPending: isCancelling } = useTraining.useCancelJob({
    onSuccess: () => {
      refetch();
    },
    onError: (err) => {
      addError('cancel-job', err.message);
    },
  });

  const lastUpdated = new Date();
  const activeJobsCount = filteredJobs.filter(j => j.status === 'running' || j.status === 'pending').length;

  const handleFilterChange = useCallback((key: TrainingJobFilterKey, value: FilterValue) => {
    setFilter(key, value);
  }, [setFilter]);

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
      {/* Action Bar */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-4">
          {can('training:start') && (
            <Button onClick={handleStartTraining} data-cy="new-training-job-btn">
              <Brain className="h-4 w-4 mr-2" />
              Start Training
            </Button>
          )}
          {!can('training:start') && (
            <Button
              disabled
              title="Requires training:start permission"
              className="opacity-50 cursor-not-allowed"
              data-cy="new-training-job-btn"
            >
              <Brain className="h-4 w-4 mr-2" />
              Start Training
            </Button>
          )}
        </div>

        <div className="flex items-center gap-4">
          <LastUpdated timestamp={lastUpdated} className="text-sm" />
          <Button
            variant="outline"
            size="sm"
            data-cy="job-history-tab"
            onClick={() => setFilter('status', 'completed')}
            disabled={isLoading}
          >
            History
          </Button>
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

      {/* Filter Bar */}
      <TrainingJobFilters
        filters={filters}
        onFilterChange={handleFilterChange}
        onClearFilters={clearFilters}
        activeFilterCount={activeFilterCount}
      />

      {/* Jobs Table */}
      <Card data-cy="training-jobs-list">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Brain className="h-5 w-5" />
            Training Jobs
            {activeJobsCount > 0 && (
              <span className="text-sm font-normal text-muted-foreground">
                ({activeJobsCount} active)
              </span>
            )}
            {activeFilterCount > 0 && (
              <span className="text-sm font-normal text-muted-foreground">
                - Showing {filteredJobs.length} of {jobs.length}
              </span>
            )}
          </CardTitle>
        </CardHeader>
        <CardContent data-cy="completed-jobs-list">
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

export default withErrorBoundary(TrainingJobsTab, 'Failed to load training jobs tab');
