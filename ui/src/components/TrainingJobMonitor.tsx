/**
 * Component to monitor multiple training jobs simultaneously
 * Uses batched monitoring for efficiency
 */
import React from 'react';
import { useBatchedTrainingNotifications } from '@/hooks/useBatchedTrainingNotifications';
import type { TrainingJob } from '@/api/types';

interface TrainingJobMonitorProps {
  jobs: TrainingJob[];
  onAdapterCreated?: (adapterId: string, jobId: string) => void;
}

export function TrainingJobMonitor({ jobs, onAdapterCreated }: TrainingJobMonitorProps) {
  // Use batched monitoring instead of individual hooks
  // This is more efficient and handles cleanup automatically
  useBatchedTrainingNotifications({
    enabled: true,
    onAdapterCreated,
  });

  // Component doesn't render anything - it's just a hook container
  return null;
}

