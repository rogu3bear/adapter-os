/**
 * Dashboard Workflow Section Component
 *
 * Container for the "Using AdapterOS" workflow cards including
 * datasets, training, adapters/stacks, and chat.
 */

import React, { memo } from 'react';
import { Badge } from '@/components/ui/badge';
import { ContentGrid } from '@/components/ui/grid';
import { DashboardDatasetCard } from './DashboardDatasetCard';
import { DashboardTrainingCard } from './DashboardTrainingCard';
import { DashboardTrainingWizardCard } from './DashboardTrainingWizardCard';
import { DashboardAdaptersCard } from './DashboardAdaptersCard';
import { DashboardChatCard } from './DashboardChatCard';
import type { DatasetStats } from '@/hooks/dashboard';
import type { TrainingJob, AdapterStack } from '@/api/types';

/**
 * Props for the DashboardWorkflowSection component
 */
export interface DashboardWorkflowSectionProps {
  /** Current tenant/workspace name */
  effectiveTenant: string;
  /** Label describing the default stack status */
  defaultStackLabel: string;

  // Dataset props
  datasetStats: DatasetStats;
  datasetsLoading: boolean;
  datasetsError: Error | null;
  onRefetchDatasets: () => void;

  // Training props
  runningJobs: number;
  completedLast7Days: number;
  recentTrainingJob: TrainingJob | null;
  trainingJobsLoading: boolean;
  trainingJobsError: Error | null;
  onRefetchTrainingJobs: () => void;

  // Adapter/Stack props
  adapterTotal: number;
  stackTotal: number;
  stackNameLookup: Map<string, string>;
  defaultStack: AdapterStack | null;
  adaptersStacksLoading: boolean;
  adapterStackError: Error | null;
  onRefetchAdaptersStacks: () => void;

  // Chat props
  recentCompletedJobWithStack: TrainingJob | null;
  defaultStackLoading: boolean;
  defaultStackError: Error | null;
  onRefetchDefaultStack: () => void;
}

/**
 * Workflow section displaying the main AdapterOS usage flow.
 *
 * Shows workflow cards in a grid: Upload data -> Train adapter -> Pick stack -> Chat.
 */
export const DashboardWorkflowSection = memo(function DashboardWorkflowSection({
  effectiveTenant,
  defaultStackLabel,
  datasetStats,
  datasetsLoading,
  datasetsError,
  onRefetchDatasets,
  runningJobs,
  completedLast7Days,
  recentTrainingJob,
  trainingJobsLoading,
  trainingJobsError,
  onRefetchTrainingJobs,
  adapterTotal,
  stackTotal,
  stackNameLookup,
  defaultStack,
  adaptersStacksLoading,
  adapterStackError,
  onRefetchAdaptersStacks,
  recentCompletedJobWithStack,
  defaultStackLoading,
  defaultStackError,
  onRefetchDefaultStack,
}: DashboardWorkflowSectionProps) {
  return (
    <div className="space-y-4">
      {/* Section Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold">Using AdapterOS</h2>
          <p className="text-sm text-muted-foreground">
            Upload data, validate, train adapters, manage stacks, and chat with your model.
          </p>
          <p className="text-xs text-muted-foreground mt-1">
            1) Upload data 2) Train adapter 3) Pick stack 4) Chat
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant="outline">Workspace: {effectiveTenant}</Badge>
          <Badge variant="secondary">{defaultStackLabel}</Badge>
        </div>
      </div>

      {/* Workflow Cards Grid */}
      <ContentGrid className="gap-4">
        {/* Datasets */}
        <DashboardDatasetCard
          datasetStats={datasetStats}
          loading={datasetsLoading}
          error={datasetsError}
          onRefetch={onRefetchDatasets}
        />

        {/* Training Jobs */}
        <DashboardTrainingCard
          runningJobs={runningJobs}
          completedLast7Days={completedLast7Days}
          recentTrainingJob={recentTrainingJob}
          stackNameLookup={stackNameLookup}
          loading={trainingJobsLoading}
          error={trainingJobsError}
          onRefetch={onRefetchTrainingJobs}
        />

        {/* Training Wizard */}
        <DashboardTrainingWizardCard />

        {/* Adapters & Stacks */}
        <DashboardAdaptersCard
          adapterTotal={adapterTotal}
          stackTotal={stackTotal}
          defaultStack={defaultStack}
          loading={adaptersStacksLoading}
          error={adapterStackError}
          onRefetch={onRefetchAdaptersStacks}
        />

        {/* Chat */}
        <DashboardChatCard
          defaultStack={defaultStack}
          recentCompletedJobWithStack={recentCompletedJobWithStack}
          stackNameLookup={stackNameLookup}
          loading={defaultStackLoading}
          error={defaultStackError}
          onRefetch={onRefetchDefaultStack}
        />
      </ContentGrid>
    </div>
  );
});
