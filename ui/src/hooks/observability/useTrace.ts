import { useQuery } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import type { TraceResponseV1 } from '@/api/types';

export function useTrace(traceId?: string, tenantId?: string) {
  return useQuery({
    queryKey: ['trace-v1', traceId, tenantId],
    queryFn: async (): Promise<TraceResponseV1 | null> => {
      const res = await apiClient.getTrace(traceId!, tenantId);
      if (res && 'tokens' in res) {
        return res as TraceResponseV1;
      }
      return null;
    },
    enabled: Boolean(traceId),
    retry: false,
  });
}
