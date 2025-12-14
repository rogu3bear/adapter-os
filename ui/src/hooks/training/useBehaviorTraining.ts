// React Query hooks for behavior training data
//
// Provides hooks for fetching and exporting adapter behavior training data.

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/api/client';
import type {
  BehaviorEvent,
  BehaviorEventFilters,
  BehaviorExportRequest,
  BehaviorStats,
} from '@/api/adapter-types';

export const behaviorKeys = {
  all: ['behavior'] as const,
  events: (filters?: BehaviorEventFilters) => [...behaviorKeys.all, 'events', filters] as const,
  stats: (tenantId?: string) => [...behaviorKeys.all, 'stats', tenantId] as const,
};

/**
 * Hook to fetch behavior events with optional filtering
 *
 * @param filters - Optional filters for events
 * @param enabled - Whether the query should execute
 * @returns React Query result with behavior events
 */
export function useBehaviorEvents(filters?: BehaviorEventFilters, enabled = true) {
  return useQuery({
    queryKey: behaviorKeys.events(filters),
    queryFn: () => apiClient.getBehaviorEvents(filters),
    enabled,
  });
}

/**
 * Hook to fetch behavior event statistics
 *
 * @param tenantId - Optional tenant ID filter
 * @param enabled - Whether the query should execute
 * @returns React Query result with behavior statistics
 */
export function useBehaviorStats(tenantId?: string, enabled = true) {
  return useQuery({
    queryKey: behaviorKeys.stats(tenantId),
    queryFn: () => apiClient.getBehaviorStats(tenantId),
    enabled,
  });
}

/**
 * Hook to export behavior data
 *
 * @returns Mutation for exporting behavior data
 */
export function useExportBehaviorData() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (request: BehaviorExportRequest) => {
      const blob = await apiClient.exportBehaviorData(request);
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `behavior_training_${new Date().toISOString().split('T')[0]}.jsonl`;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      URL.revokeObjectURL(url);
      return { success: true };
    },
    onSuccess: () => {
      // Invalidate events query to reflect any changes
      queryClient.invalidateQueries({ queryKey: behaviorKeys.all });
    },
  });
}

