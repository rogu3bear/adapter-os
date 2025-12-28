/**
 * Dashboard Dataset Card Component
 *
 * Displays dataset upload and validation status with quick actions.
 */

import React, { memo } from 'react';
import { Link } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { buildTrainingDatasetsLink } from '@/utils/navLinks';
import type { DatasetStats } from '@/hooks/dashboard';

/**
 * Props for the DashboardDatasetCard component
 */
export interface DashboardDatasetCardProps {
  /** Dataset validation statistics */
  datasetStats: DatasetStats;
  /** Whether datasets are loading */
  loading: boolean;
  /** Error from fetching datasets, if any */
  error: Error | null;
  /** Callback to refetch datasets */
  onRefetch: () => void;
}

/**
 * Dataset workflow card for the dashboard.
 *
 * Shows dataset counts by validation status and provides
 * quick actions for uploading and managing datasets.
 */
export const DashboardDatasetCard = memo(function DashboardDatasetCard({
  datasetStats,
  loading,
  error,
  onRefetch,
}: DashboardDatasetCardProps) {
  return (
    <SectionErrorBoundary sectionName="Datasets">
      <Card>
        <CardHeader>
          <CardTitle>Get started with your data</CardTitle>
          <p className="text-sm text-muted-foreground">
            Upload and validate datasets before training.
          </p>
        </CardHeader>
        <CardContent className="space-y-4">
          {loading ? (
            <Skeleton className="h-20 w-full" />
          ) : error ? (
            errorRecoveryTemplates.genericError(error, onRefetch)
          ) : (
            <>
              <div className="flex items-center justify-between gap-4">
                <div>
                  <p className="text-2xl font-bold">{datasetStats.total}</p>
                  <p className="text-xs text-muted-foreground">Total datasets</p>
                </div>
                <div className="flex flex-wrap gap-2 text-xs">
                  <Badge variant="outline">Valid {datasetStats.valid}</Badge>
                  <Badge variant="outline">Draft {datasetStats.draft}</Badge>
                  <Badge variant="outline">Invalid {datasetStats.invalid}</Badge>
                </div>
              </div>
              <p className="text-sm text-muted-foreground">
                {datasetStats.total === 0
                  ? 'No datasets yet. Upload one to begin training.'
                  : 'Validation overview for your datasets.'}
              </p>
              <div className="flex flex-wrap gap-2">
                <Button asChild>
                  <Link to={buildTrainingDatasetsLink()} state={{ openUpload: true }}>
                    Upload dataset
                  </Link>
                </Button>
                <Button variant="outline" asChild>
                  <Link to={buildTrainingDatasetsLink()}>View datasets</Link>
                </Button>
              </div>
            </>
          )}
        </CardContent>
      </Card>
    </SectionErrorBoundary>
  );
});
