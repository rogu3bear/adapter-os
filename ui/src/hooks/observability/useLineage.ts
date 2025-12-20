import { useMemo } from 'react';
import { useQuery, UseQueryOptions } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import type { LineageEntityKind, LineageGraphResponse, LineageQueryParams } from '@/api/types';

const lineageKeys = {
  all: ['lineage'] as const,
  graph: (kind: LineageEntityKind, id: string, params?: LineageQueryParams) =>
    [...lineageKeys.all, kind, id, params] as const,
};

export interface UseLineageOptions
  extends Omit<UseQueryOptions<LineageGraphResponse, Error>, 'queryKey' | 'queryFn'> {
  params?: LineageQueryParams;
}

export function useLineage(
  kind: LineageEntityKind,
  id: string | undefined,
  { params, ...options }: UseLineageOptions = {},
) {
  const queryKey = useMemo(() => lineageKeys.graph(kind, id || '', params), [kind, id, params]);

  return useQuery({
    queryKey,
    enabled: Boolean(id),
    queryFn: async (): Promise<LineageGraphResponse> => {
      if (!id) {
        throw new Error('lineage id is required');
      }
      if (kind === 'dataset_version') {
        return apiClient.getDatasetVersionLineage(id, params);
      }
      return apiClient.getAdapterVersionLineage(id, params);
    },
    staleTime: 60_000,
    ...options,
  });
}

export const lineageQueryKeys = lineageKeys;
