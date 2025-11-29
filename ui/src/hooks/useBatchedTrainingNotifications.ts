/**
 * Batched training notifications hook
 * Monitors all active training jobs efficiently using a single query
 * instead of creating multiple hook instances
 */
import { useEffect, useRef } from 'react';
import { toast } from 'sonner';
import { useQuery } from '@tanstack/react-query';
import apiClient from '../api/client';
import { logger } from '../utils/logger';
import type { TrainingJob } from '../api/types';
import { globalNotifiedJobs, globalNotifiedAdapters } from './useTrainingNotifications';

interface UseBatchedTrainingNotificationsOptions {
  enabled?: boolean;
  onTrainingComplete?: (job: TrainingJob) => void;
  onAdapterCreated?: (adapterId: string, jobId: string) => void;
  refetchInterval?: number;
}

/**
 * Hook to monitor all active training jobs in a single batch query
 * More efficient than creating multiple hook instances
 */
export function useBatchedTrainingNotifications({
  enabled = true,
  onTrainingComplete,
  onAdapterCreated,
  refetchInterval = 5000,
}: UseBatchedTrainingNotificationsOptions = {}) {
  const notifiedJobsRef = useRef<Set<string>>(new Set());
  const notifiedAdaptersRef = useRef<Set<string>>(new Set());

  // Batch query for all active jobs (running or queued)
  const { data: activeJobs } = useQuery({
    queryKey: ['training-jobs', 'active'],
    queryFn: async () => {
      const runningResponse = await apiClient.listTrainingJobs({ status: 'running' });
      const pendingResponse = await apiClient.listTrainingJobs({ status: 'pending' });
      return [...(runningResponse.jobs || []), ...(pendingResponse.jobs || [])];
    },
    enabled,
    refetchInterval: enabled ? refetchInterval : false,
  });

  // Monitor each active job for status changes
  useEffect(() => {
    if (!activeJobs || activeJobs.length === 0) return;

    activeJobs.forEach((job) => {
      const jobId = job.id;
      const jobKey = `${jobId}-${job.status}`;
      const globalJobKey = `global-${jobId}-${job.status}`;

      // Notify when training starts
      if (job.status === 'running' && !notifiedJobsRef.current.has(`${jobId}-started`) && !globalNotifiedJobs.has(globalJobKey)) {
        notifiedJobsRef.current.add(`${jobId}-started`);
        globalNotifiedJobs.add(globalJobKey);
        toast.success('Training started', {
          description: `Job "${job.adapter_name || jobId}" is now running.`,
          duration: 5000,
        });
        logger.info('Training job started', {
          component: 'useBatchedTrainingNotifications',
          jobId,
          adapterName: job.adapter_name,
        });
      }

      // Notify when training completes
      if (job.status === 'completed' && !notifiedJobsRef.current.has(`${jobId}-completed`) && !globalNotifiedJobs.has(`global-${jobId}-completed`)) {
        notifiedJobsRef.current.add(`${jobId}-completed`);
        globalNotifiedJobs.add(`global-${jobId}-completed`);
        
        const adapterId = job.adapter_id;
        if (adapterId) {
          toast.success('Training completed!', {
            description: `Adapter "${adapterId}" is ready.`,
            duration: 8000,
            action: adapterId ? {
              label: 'View Adapter',
              onClick: () => {
                window.location.href = `/adapters/${adapterId}`;
              },
            } : undefined,
          });
          
          // Notify adapter creation
          if (!notifiedAdaptersRef.current.has(adapterId) && !globalNotifiedAdapters.has(adapterId)) {
            notifiedAdaptersRef.current.add(adapterId);
            globalNotifiedAdapters.add(adapterId);
            onAdapterCreated?.(adapterId, jobId);
          }
        } else {
          toast.success('Training completed', {
            description: `Job "${job.adapter_name || jobId}" finished successfully.`,
            duration: 5000,
          });
        }
        
        onTrainingComplete?.(job);
        logger.info('Training job completed', {
          component: 'useBatchedTrainingNotifications',
          jobId,
          adapterId: job.adapter_id,
        });
      }

      // Notify when training fails
      if (job.status === 'failed' && !notifiedJobsRef.current.has(`${jobId}-failed`) && !globalNotifiedJobs.has(`global-${jobId}-failed`)) {
        notifiedJobsRef.current.add(`${jobId}-failed`);
        globalNotifiedJobs.add(`global-${jobId}-failed`);
        toast.error('Training failed', {
          description: `Job "${job.adapter_name || jobId}" encountered an error.`,
          duration: 8000,
        });
        logger.error('Training job failed', {
          component: 'useBatchedTrainingNotifications',
          jobId,
        }, new Error('Training job failed'));
      }
    });
  }, [activeJobs, onTrainingComplete, onAdapterCreated]);

  // Cleanup: remove completed/failed jobs from local tracking
  useEffect(() => {
    return () => {
      // Clean up local refs on unmount
      notifiedJobsRef.current.clear();
      notifiedAdaptersRef.current.clear();
    };
  }, []);
}

