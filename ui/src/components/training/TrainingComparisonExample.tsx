/**
 * TrainingComparisonExample.tsx
 * 
 * Example usage of the TrainingComparison component.
 * Demonstrates integration with API client and state management.
 */

import React, { useState, useEffect } from 'react';
import { TrainingComparison } from './TrainingComparison';
import { TrainingJob } from '@/api/types';
import apiClient from '@/api/client';
import { logger } from '@/utils/logger';
import { toast } from 'sonner';

export function TrainingComparisonExample() {
  const [jobs, setJobs] = useState<TrainingJob[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  useEffect(() => {
    fetchTrainingJobs();
  }, []);

  const fetchTrainingJobs = async () => {
    try {
      setLoading(true);
      setError(null);

      // Fetch all training jobs from API
      const response = await apiClient.listTrainingJobs();
      const jobList = response.jobs || [];
      setJobs(jobList);

      logger.info('Loaded training jobs for comparison', { count: jobList.length });
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to load training jobs');
      setError(error);
      logger.error('Failed to fetch training jobs', { error: error.message });
      toast.error('Failed to load training jobs');
    } finally {
      setLoading(false);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center p-12">
        <div className="text-center">
          <div className="animate-spin size-8 border-4 border-primary border-t-transparent rounded-full mx-auto mb-4" />
          <p className="text-muted-foreground">Loading training jobs...</p>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-12 text-center">
        <p className="text-destructive mb-4">Error: {error.message}</p>
        <button
          onClick={fetchTrainingJobs}
          className="px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90"
        >
          Retry
        </button>
      </div>
    );
  }

  return (
    <TrainingComparison
      jobs={jobs}
      onClose={() => {
        // Handle close action (e.g., navigate back)
      }}
    />
  );
}

/**
 * Example with specific job IDs (alternative approach)
 */
export function TrainingComparisonWithIds({ jobIds }: { jobIds: string[] }) {
  const [jobs, setJobs] = useState<TrainingJob[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetchSpecificJobs();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- fetchSpecificJobs is not stable, only run when jobIds changes
  }, [jobIds]);

  const fetchSpecificJobs = async () => {
    try {
      setLoading(true);
      
      // Fetch specific jobs by ID
      const jobPromises = jobIds.map(id => apiClient.getTrainingJob(id));
      const fetchedJobs = await Promise.all(jobPromises);
      
      setJobs(fetchedJobs);
    } catch (err) {
      logger.error('Failed to fetch specific jobs', { error: String(err) });
      toast.error('Failed to load some training jobs');
    } finally {
      setLoading(false);
    }
  };

  if (loading) {
    return <div>Loading...</div>;
  }

  return <TrainingComparison jobs={jobs} />;
}
