/**
 * Dashboard Adapters Card Component
 *
 * Displays adapter and stack counts with quick actions.
 */

import React, { memo } from 'react';
import { Link } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { buildAdaptersListLink, buildAdminStacksLink } from '@/utils/navLinks';
import type { AdapterStack } from '@/api/types';

/**
 * Props for the DashboardAdaptersCard component
 */
export interface DashboardAdaptersCardProps {
  /** Total number of adapters */
  adapterTotal: number;
  /** Total number of stacks */
  stackTotal: number;
  /** Default stack for the current workspace, if set */
  defaultStack: AdapterStack | null;
  /** Whether adapters/stacks are loading */
  loading: boolean;
  /** Error from fetching adapters/stacks, if any */
  error: Error | null;
  /** Callback to refetch adapters and stacks */
  onRefetch: () => void;
}

/**
 * Adapters and stacks workflow card for the dashboard.
 *
 * Shows adapter and stack counts, default stack status,
 * and provides quick actions for managing adapters and stacks.
 */
export const DashboardAdaptersCard = memo(function DashboardAdaptersCard({
  adapterTotal,
  stackTotal,
  defaultStack,
  loading,
  error,
  onRefetch,
}: DashboardAdaptersCardProps) {
  return (
    <SectionErrorBoundary sectionName="Adapters & Stacks">
      <Card>
        <CardHeader>
          <CardTitle>Adapters & stacks</CardTitle>
          <p className="text-sm text-muted-foreground">See what is ready to serve.</p>
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
                  <p className="text-2xl font-bold">{adapterTotal}</p>
                  <p className="text-xs text-muted-foreground">Adapters</p>
                </div>
                <div>
                  <p className="text-2xl font-bold">{stackTotal}</p>
                  <p className="text-xs text-muted-foreground">Stacks</p>
                </div>
              </div>
              <p className="text-sm text-muted-foreground">
                {stackTotal === 0
                  ? 'No adapters or stacks yet. Complete a training job to register an adapter and auto-create a stack.'
                  : defaultStack
                    ? `Default stack for this workspace: ${defaultStack.name}`
                    : 'No default stack configured. Training will auto-create one; you can also set it under Stacks.'}
              </p>
              <div className="flex flex-wrap gap-2">
                <Button variant="outline" asChild>
                  <Link to={buildAdaptersListLink()}>Manage adapters</Link>
                </Button>
                <Button asChild variant="secondary">
                  <Link to={buildAdminStacksLink()}>Manage stacks</Link>
                </Button>
              </div>
            </>
          )}
        </CardContent>
      </Card>
    </SectionErrorBoundary>
  );
});
