/**
 * Dashboard Training Card Component
 *
 * Displays training job status with recent job details and quick actions.
 */

import React, { memo } from 'react';
import { Link } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { buildTrainingJobsLink, buildTrainingOverviewLink } from '@/utils/navLinks';
import type { TrainingJob } from '@/api/types';

/**
 * Props for the DashboardTrainingCard component
 */
export interface DashboardTrainingCardProps {
  /** Number of running or pending jobs */
  runningJobs: number;
  /** Number of jobs completed in the last 7 days */
  completedLast7Days: number;
  /** Most recent training job, if any */
  recentTrainingJob: TrainingJob | null;
  /** Map of stack IDs to stack names for display */
  stackNameLookup: Map<string, string>;
  /** Whether training jobs are loading */
  loading: boolean;
  /** Error from fetching training jobs, if any */
  error: Error | null;
  /** Callback to refetch training jobs */
  onRefetch: () => void;
}

/**
 * Training jobs workflow card for the dashboard.
 *
 * Shows running and completed job counts, recent job details,
 * and provides quick actions for viewing and starting training.
 */
export const DashboardTrainingCard = memo(function DashboardTrainingCard({
  runningJobs,
  completedLast7Days,
  recentTrainingJob,
  stackNameLookup,
  loading,
  error,
  onRefetch,
}: DashboardTrainingCardProps) {
  return (
    <SectionErrorBoundary sectionName="Training Jobs">
      <Card>
        <CardHeader>
          <CardTitle>Training jobs</CardTitle>
          <p className="text-sm text-muted-foreground">
            Track running jobs or start a new training.
          </p>
        </CardHeader>
        <CardContent className="space-y-4">
          {loading ? (
            <Skeleton className="h-20 w-full" />
          ) : error ? (
            errorRecoveryTemplates.genericError(error, onRefetch)
          ) : (
            <>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <p className="text-2xl font-bold">{runningJobs}</p>
                  <p className="text-xs text-muted-foreground">Running jobs</p>
                </div>
                <div>
                  <p className="text-2xl font-bold">{completedLast7Days}</p>
                  <p className="text-xs text-muted-foreground">Completed last 7 days</p>
                </div>
              </div>
              {recentTrainingJob ? (
                <div className="rounded-lg border bg-muted/40 p-3 space-y-1">
                  <div className="flex items-center justify-between gap-2">
                    <p className="text-sm font-medium truncate">
                      {recentTrainingJob.adapter_name || recentTrainingJob.id}
                    </p>
                    <Badge variant="outline">{recentTrainingJob.status}</Badge>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    Dataset: {recentTrainingJob.dataset_id || '---'}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    Stack:{' '}
                    {recentTrainingJob.stack_id
                      ? stackNameLookup.get(recentTrainingJob.stack_id) ||
                        recentTrainingJob.stack_id
                      : 'Not set'}
                  </p>
                </div>
              ) : (
                <p className="text-sm text-muted-foreground">
                  No training jobs yet. Start training after you have a validated dataset.
                </p>
              )}
              <div className="flex flex-wrap gap-2">
                <Button variant="outline" asChild>
                  <Link to={buildTrainingJobsLink()}>View training jobs</Link>
                </Button>
                <Button asChild>
                  <Link
                    to={buildTrainingOverviewLink()}
                    state={{ openTrainingWizard: true }}
                  >
                    Start new training
                  </Link>
                </Button>
              </div>
            </>
          )}
        </CardContent>
      </Card>
    </SectionErrorBoundary>
  );
});
