/**
 * React Query hooks for Settings API
 *
 * Provides hooks for fetching and updating system settings with optimistic updates.
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/api/client';
import type {
  SystemSettings,
  UpdateSettingsRequest,
  SettingsUpdateResponse,
} from '@/api/document-types';
import { useToast } from '@/hooks/use-toast';

// Query keys for cache management
export const settingsKeys = {
  all: ['settings'] as const,
  current: () => [...settingsKeys.all, 'current'] as const,
};

/**
 * Hook for fetching current system settings
 */
export function useSettings() {
  return useQuery({
    queryKey: settingsKeys.current(),
    queryFn: async (): Promise<SystemSettings> => {
      // GET /v1/settings endpoint (requires Admin role)
      return apiClient.request<SystemSettings>('/v1/settings');
    },
    staleTime: 5 * 60 * 1000, // 5 minutes
  });
}

/**
 * Hook for updating system settings with optimistic updates
 */
export function useUpdateSettings() {
  const queryClient = useQueryClient();
  const { toast } = useToast();

  return useMutation({
    mutationFn: async (request: UpdateSettingsRequest): Promise<SettingsUpdateResponse> => {
      // PUT /v1/settings endpoint (requires Admin role)
      return apiClient.request<SettingsUpdateResponse>('/v1/settings', {
        method: 'PUT',
        body: JSON.stringify(request),
      });
    },
    onMutate: async (newSettings) => {
      // Cancel outgoing refetches
      await queryClient.cancelQueries({ queryKey: settingsKeys.current() });

      // Snapshot previous value
      const previousSettings = queryClient.getQueryData<SystemSettings>(
        settingsKeys.current()
      );

      // Optimistically update
      if (previousSettings) {
        queryClient.setQueryData<SystemSettings>(settingsKeys.current(), {
          ...previousSettings,
          general: newSettings.general
            ? { ...previousSettings.general, ...newSettings.general }
            : previousSettings.general,
          server: newSettings.server
            ? { ...previousSettings.server, ...newSettings.server }
            : previousSettings.server,
          security: newSettings.security
            ? { ...previousSettings.security, ...newSettings.security }
            : previousSettings.security,
          performance: newSettings.performance
            ? { ...previousSettings.performance, ...newSettings.performance }
            : previousSettings.performance,
        });
      }

      return { previousSettings };
    },
    onError: (_error, _newSettings, context) => {
      // Rollback on error
      if (context?.previousSettings) {
        queryClient.setQueryData(settingsKeys.current(), context.previousSettings);
      }
      toast({
        title: 'Failed to update settings',
        description: 'Your changes could not be saved. Please try again.',
        variant: 'destructive',
      });
    },
    onSuccess: (response) => {
      queryClient.invalidateQueries({ queryKey: settingsKeys.current() });

      if (response.restart_required) {
        toast({
          title: 'Settings saved',
          description: 'A server restart is required for some changes to take effect.',
          variant: 'default',
        });
      } else {
        toast({
          title: 'Settings saved',
          description: response.message || 'Your settings have been updated.',
        });
      }
    },
  });
}

export default useSettings;
