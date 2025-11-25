import { useEffect, useRef } from 'react';
import { toast } from 'sonner';
import { useQuery } from '@tanstack/react-query';
import apiClient from '../api/client';
import { logger, toError } from '../utils/logger';
import type { TrainingJob } from '../api/types';

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
      const firstKey = this.cache.keys().next().value;
      this.cache.delete(firstKey);
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

const globalNotifiedJobs = new LRUCache<string>(1000);
const globalNotifiedAdapters = new LRUCache<string>(1000);

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
 * - Training started
 * - Training completion
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

  // Poll for job updates if jobId is provided
  const { data: job } = useQuery({
    queryKey: ['training-job', jobId],
    queryFn: () => (jobId ? apiClient.getTrainingJob(jobId) : null),
    enabled: enabled && !!jobId,
    refetchInterval: (query) => {
      const job = query.state.data as TrainingJob | null;
      // Poll every 5 seconds if job is running, otherwise stop
      return job?.status === 'running' || job?.status === 'queued' ? 5000 : false;
    },
  });

  useEffect(() => {
    if (!job || !jobId) return;

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
        component: 'useTrainingNotifications',
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
          onAdapterCreated?.(adapterId);
        }
      } else {
        toast.success('Training completed', {
          description: `Job "${job.adapter_name || jobId}" finished successfully.`,
          duration: 5000,
        });
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
      toast.error('Training failed', {
        description: `Job "${job.adapter_name || jobId}" encountered an error.`,
        duration: 8000,
      });
      logger.error('Training job failed', {
        component: 'useTrainingNotifications',
        jobId,
      }, new Error('Training job failed'));
    }
  }, [job, jobId, onTrainingComplete, onAdapterCreated]);

  // Cleanup: remove from global tracking when component unmounts or job completes/fails
  useEffect(() => {
    return () => {
      if (jobId && job) {
        if (job.status === 'completed' || job.status === 'failed' || job.status === 'cancelled') {
          // Keep completed/failed notifications in global set to prevent re-notification
          // but clean up local refs
          notifiedJobsRef.current.clear();
        }
      }
    };
  }, [jobId, job]);
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
          window.location.href = `/adapter-stacks/${id}`;
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

