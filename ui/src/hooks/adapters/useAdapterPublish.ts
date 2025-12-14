/**
 * useAdapterPublish - Hooks for adapter publishing workflow
 *
 * Provides mutations for publishing adapters after training,
 * archiving adapters, and managing attach mode configuration.
 */

import { useMutation, useQueryClient } from '@tanstack/react-query';
import apiClient from '@/api/client';
import type {
  PublishAdapterRequest,
  PublishAdapterResponse,
  ArchiveAdapterResponse,
} from '@/api/adapter-types';
import { toast } from 'sonner';
import { logger } from '@/utils/logger';

// Query keys for adapter publish operations
export const adapterPublishKeys = {
  all: ['adapter-publish'] as const,
  detail: (versionId: string) => ['adapter-publish', versionId] as const,
};

/**
 * Hook to publish an adapter version after training.
 * Configures attach mode and makes the adapter available for use in stacks.
 */
export function usePublishAdapter() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      repoId,
      versionId,
      data,
    }: {
      repoId: string;
      versionId: string;
      data: PublishAdapterRequest;
    }) => apiClient.publishAdapterVersion(repoId, versionId, data),
    onSuccess: (response: PublishAdapterResponse) => {
      // Invalidate related queries to refetch fresh data
      queryClient.invalidateQueries({ queryKey: ['adapters'] });
      queryClient.invalidateQueries({ queryKey: ['adapter-versions'] });
      queryClient.invalidateQueries({ queryKey: ['training-jobs'] });
      queryClient.invalidateQueries({ queryKey: ['repos'] });

      toast.success('Adapter published successfully');
      logger.info('Adapter published', {
        component: 'useAdapterPublish',
        operation: 'publishAdapter',
        versionId: response.version_id,
        repoId: response.repo_id,
        attachMode: response.attach_mode,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to publish adapter: ${error.message}`);
      logger.error(
        'Failed to publish adapter',
        {
          component: 'useAdapterPublish',
          operation: 'publishAdapter',
        },
        error
      );
    },
  });
}

/**
 * Hook to archive an adapter version.
 * Archived versions are hidden from normal use but retained for audit.
 */
export function useArchiveAdapter() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ versionId, reason }: { versionId: string; reason?: string }) =>
      apiClient.archiveAdapterVersion(versionId, reason),
    onSuccess: (response: ArchiveAdapterResponse) => {
      queryClient.invalidateQueries({ queryKey: ['adapters'] });
      queryClient.invalidateQueries({ queryKey: ['adapter-versions'] });
      queryClient.invalidateQueries({ queryKey: ['adapter-stacks'] });

      toast.success('Adapter archived');
      logger.info('Adapter archived', {
        component: 'useAdapterPublish',
        operation: 'archiveAdapter',
        versionId: response.version_id,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to archive adapter: ${error.message}`);
      logger.error(
        'Failed to archive adapter',
        {
          component: 'useAdapterPublish',
          operation: 'archiveAdapter',
        },
        error
      );
    },
  });
}

/**
 * Hook to unarchive (restore) an adapter version.
 * Makes the adapter visible again for normal use.
 */
export function useUnarchiveAdapter() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (versionId: string) => apiClient.unarchiveAdapterVersion(versionId),
    onSuccess: (response: ArchiveAdapterResponse) => {
      queryClient.invalidateQueries({ queryKey: ['adapters'] });
      queryClient.invalidateQueries({ queryKey: ['adapter-versions'] });
      queryClient.invalidateQueries({ queryKey: ['adapter-stacks'] });

      toast.success('Adapter restored');
      logger.info('Adapter unarchived', {
        component: 'useAdapterPublish',
        operation: 'unarchiveAdapter',
        versionId: response.version_id,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to restore adapter: ${error.message}`);
      logger.error(
        'Failed to unarchive adapter',
        {
          component: 'useAdapterPublish',
          operation: 'unarchiveAdapter',
        },
        error
      );
    },
  });
}
