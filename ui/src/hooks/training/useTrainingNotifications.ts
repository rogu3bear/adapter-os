import { useEffect, useRef, useCallback } from 'react';
import { toast } from 'sonner';
import { useQuery } from '@tanstack/react-query';
import apiClient from '@/api/client';
import { logger } from '@/utils/logger';
import type { TrainingJob } from '@/api/types';
import { usePersistentNotifications, usePersistentNotificationsAvailable } from '@/components/PersistentNotifications';

// Global tracking to prevent duplicate notifications across hook instances
// Use LRU cache with max size to prevent memory leaks
class LRUCache<K> {
  private cache: Map<K, number>;
  private maxSize: number;

  constructor(maxSize: number = 1000) {
    this.cache = new Map();
    this.maxSize = maxSize;
  }

  has(key: K): boolean {
    return this.cache.has(key);
  }

  add(key: K): void {
    // Remove oldest entries if at capacity
    if (this.cache.size >= this.maxSize) {
      const firstKey = this.cache.keys().next().value as K | undefined;
      if (firstKey !== undefined) {
        this.cache.delete(firstKey);
      }
    }
    this.cache.set(key, Date.now());
  }

  clear(): void {
    this.cache.clear();
  }

  // Clean up entries older than 24 hours
  cleanup(maxAge: number = 24 * 60 * 60 * 1000): void {
    const now = Date.now();
    for (const [key, timestamp] of this.cache.entries()) {
      if (now - timestamp > maxAge) {
        this.cache.delete(key);
      }
    }
  }
}

export const globalNotifiedJobs = new LRUCache<string>(1000);
export const globalNotifiedAdapters = new LRUCache<string>(1000);

// Cleanup old entries every hour
if (typeof window !== 'undefined') {
  setInterval(() => {
    globalNotifiedJobs.cleanup();
    globalNotifiedAdapters.cleanup();
  }, 60 * 60 * 1000);
}

interface UseTrainingNotificationsOptions {
  jobId?: string;
  enabled?: boolean;
  onTrainingComplete?: (job: TrainingJob) => void;
  onAdapterCreated?: (adapterId: string) => void;
}

/**
 * Hook to monitor training jobs and show notifications for async flows
 * Provides proactive notifications for:
 * - Training started (persistent notification with progress)
 * - Training completion (with link to adapter)
 * - Adapter creation
 */
export function useTrainingNotifications({
  jobId,
  enabled = true,
  onTrainingComplete,
  onAdapterCreated,
}: UseTrainingNotificationsOptions = {}) {
  const notifiedJobsRef = useRef<Set<string>>(new Set());
  const notifiedAdaptersRef = useRef<Set<string>>(new Set());
  const persistentNotificationIdRef = useRef<string | null>(null);

  // Use persistent notifications (returns no-op if outside provider)
  const persistentNotifications = usePersistentNotifications();
  const hasPersistentNotifications = usePersistentNotificationsAvailable();

  // Poll for job updates if jobId is provided
  const { data: job } = useQuery({
    queryKey: ['training-job', jobId],
    queryFn: () => (jobId ? apiClient.getTrainingJob(jobId) : null),
    enabled: enabled && !!jobId,
    refetchInterval: (query) => {
      const job = query.state.data as TrainingJob | null;
      // Poll every 3 seconds if job is running or pending, otherwise stop
      return job?.status === 'running' || job?.status === 'pending' ? 3000 : false;
    },
  });

  useEffect(() => {
    if (!job || !jobId) return;

    const globalJobKey = `global-${jobId}-${job.status}`;

    // Notify when training starts - use persistent notification
    if (job.status === 'running' && !notifiedJobsRef.current.has(`${jobId}-started`) && !globalNotifiedJobs.has(globalJobKey)) {
      notifiedJobsRef.current.add(`${jobId}-started`);
      globalNotifiedJobs.add(globalJobKey);

      if (hasPersistentNotifications) {
        // Create persistent notification with progress
        const notificationId = persistentNotifications.addNotification({
          title: `Training: ${job.adapter_name || 'Adapter'}`,
          description: 'Initializing training...',
          status: 'in_progress',
          progress: 0,
          resourceType: 'training_job',
          resourceId: jobId,
          resourceName: job.adapter_name,
          linkPath: `/training?job=${jobId}`,
          metadata: {
            adapter_name: job.adapter_name,
            started_at: job.started_at,
          },
          persistent: true,
        });
        persistentNotificationIdRef.current = notificationId;
      } else {
        // Fallback to toast
        toast.success('Training started', {
          description: `Job "${job.adapter_name || jobId}" is now running.`,
          duration: 5000,
        });
      }

      logger.info('Training job started', {
        component: 'useTrainingNotifications',
        jobId,
        adapterName: job.adapter_name,
      });
    }

    // Update progress for running jobs
    if (job.status === 'running' && persistentNotificationIdRef.current && hasPersistentNotifications) {
      const progress = job.progress ?? 0;
      const currentEpoch = job.current_epoch ?? 0;
      const totalEpochs = job.total_epochs ?? 1;

      persistentNotifications.updateNotification(persistentNotificationIdRef.current, {
        progress,
        description: `Epoch ${currentEpoch}/${totalEpochs}`,
        metadata: {
          adapter_name: job.adapter_name,
          epoch: currentEpoch,
          total_epochs: totalEpochs,
          loss: job.current_loss,
          started_at: job.started_at,
        },
      });
    }

    // Notify when training completes
    if (job.status === 'completed' && !notifiedJobsRef.current.has(`${jobId}-completed`) && !globalNotifiedJobs.has(`global-${jobId}-completed`)) {
      notifiedJobsRef.current.add(`${jobId}-completed`);
      globalNotifiedJobs.add(`global-${jobId}-completed`);

      const adapterId = job.adapter_id;
      const stackId = job.stack_id;

      if (persistentNotificationIdRef.current && hasPersistentNotifications) {
        // Update persistent notification to completed state
        // Calculate duration if possible
        let durationMs: number | undefined;
        if (job.started_at && job.completed_at) {
          durationMs = new Date(job.completed_at).getTime() - new Date(job.started_at).getTime();
        }

        persistentNotifications.updateNotification(persistentNotificationIdRef.current, {
          status: 'completed',
          title: `Training Complete: ${job.adapter_name || 'Adapter'}`,
          description: stackId ? 'Click to open result chat' : (adapterId ? 'Adapter is ready to use' : 'Training finished successfully'),
          progress: 100,
          resourceType: stackId ? 'training_job' : (adapterId ? 'adapter' : 'training_job'),
          resourceId: stackId ? jobId : (adapterId || jobId),
          resourceName: job.adapter_name,
          linkPath: stackId ? `/training/jobs/${jobId}/chat` : (adapterId ? `/adapters/${adapterId}` : `/training?job=${jobId}`),
          metadata: {
            adapter_name: job.adapter_name,
            adapter_id: adapterId,
            duration_ms: durationMs,
            loss: job.loss || job.current_loss,
          },
          persistent: false,
          autoCloseDelay: 15000, // Keep visible for 15 seconds
        });
      } else if (stackId) {
        // If job has a stack_id, show "Open Result Chat" action
        toast.success('Training completed!', {
          description: `Adapter "${adapterId || job.adapter_name || 'training output'}" is ready.`,
          duration: 10000,
          action: {
            label: 'Open Result Chat',
            onClick: () => {
              // Navigate to result chat page (handles session creation internally)
              window.location.href = `/training/jobs/${jobId}/chat`;
            },
          },
        });
      } else if (adapterId) {
        toast.success('Training completed!', {
          description: `Adapter "${adapterId}" is ready.`,
          duration: 8000,
          action: {
            label: 'View Adapter',
            onClick: () => {
              window.location.href = `/adapters/${adapterId}`;
            },
          },
        });
      } else {
        toast.success('Training completed', {
          description: `Job "${job.adapter_name || jobId}" finished successfully.`,
          duration: 5000,
        });
      }

      // Notify adapter creation
      if (adapterId && !notifiedAdaptersRef.current.has(adapterId) && !globalNotifiedAdapters.has(adapterId)) {
        notifiedAdaptersRef.current.add(adapterId);
        globalNotifiedAdapters.add(adapterId);
        onAdapterCreated?.(adapterId);
      }

      onTrainingComplete?.(job);
      logger.info('Training job completed', {
        component: 'useTrainingNotifications',
        jobId,
        adapterId: job.adapter_id,
      });
    }

    // Notify when training fails
    if (job.status === 'failed' && !notifiedJobsRef.current.has(`${jobId}-failed`) && !globalNotifiedJobs.has(`global-${jobId}-failed`)) {
      notifiedJobsRef.current.add(`${jobId}-failed`);
      globalNotifiedJobs.add(`global-${jobId}-failed`);

      if (persistentNotificationIdRef.current && hasPersistentNotifications) {
        persistentNotifications.updateNotification(persistentNotificationIdRef.current, {
          status: 'failed',
          title: `Training Failed: ${job.adapter_name || 'Adapter'}`,
          description: job.error_message || 'An error occurred during training',
          linkPath: `/training?job=${jobId}`,
          persistent: false,
          autoCloseDelay: 20000,
        });
      } else {
        toast.error('Training failed', {
          description: `Job "${job.adapter_name || jobId}" encountered an error.`,
          duration: 8000,
        });
      }

      logger.error('Training job failed', {
        component: 'useTrainingNotifications',
        jobId,
      }, new Error('Training job failed'));
    }
  }, [job, jobId, onTrainingComplete, onAdapterCreated, hasPersistentNotifications, persistentNotifications]);

  // Cleanup: remove from global tracking when component unmounts or job completes/fails
  useEffect(() => {
    const jobs = notifiedJobsRef.current;
    return () => {
      if (jobId && job) {
        if (job.status === 'completed' || job.status === 'failed' || job.status === 'cancelled') {
          // Keep completed/failed notifications in global set to prevent re-notification
          // but clean up local refs
          jobs.clear();
          persistentNotificationIdRef.current = null;
        }
      }
    };
  }, [jobId, job]);

  return { job };
}

/**
 * Hook to monitor adapter creation from training jobs
 */
export function useAdapterCreationNotifications(adapterId?: string) {
  const notifiedRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    if (!adapterId || notifiedRef.current.has(adapterId)) return;

    notifiedRef.current.add(adapterId);
    toast.success('Adapter created', {
      description: `Adapter "${adapterId}" is ready to use.`,
      duration: 8000,
      action: {
        label: 'View Adapter',
        onClick: () => {
          window.location.href = `/adapters/${adapterId}`;
        },
      },
    });
    logger.info('Adapter created notification', {
      component: 'useAdapterCreationNotifications',
      adapterId,
    });
  }, [adapterId]);
}

/**
 * Hook to monitor stack updates
 */
export function useStackUpdateNotifications(stackId?: string, stackName?: string) {
  const notifiedRef = useRef<Set<string>>(new Set());

  const notifyStackUpdate = (id: string, name?: string) => {
    if (notifiedRef.current.has(id)) return;
    notifiedRef.current.add(id);
    
    toast.success('Stack updated', {
      description: `Stack "${name || id}" has been updated successfully.`,
      duration: 5000,
      action: {
        label: 'View Stack',
        onClick: () => {
          window.location.href = `/admin/stacks`;
        },
      },
    });
    logger.info('Stack updated notification', {
      component: 'useStackUpdateNotifications',
      stackId: id,
      stackName: name,
    });
  };

  useEffect(() => {
    if (stackId) {
      notifyStackUpdate(stackId, stackName);
    }
  }, [stackId, stackName]);

  return { notifyStackUpdate };
}

