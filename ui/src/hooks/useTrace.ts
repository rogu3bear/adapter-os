import { useQuery } from '@tanstack/react-query';
import apiClient from '@/api/client';
import type { TraceResponseV1 } from '@/api/types';

export function useTrace(traceId?: string, tenantId?: string) {
  return useQuery<TraceResponseV1 | null>({
    queryKey: ['trace-v1', traceId, tenantId],
    queryFn: () =>
      apiClient.getTrace(traceId!, tenantId).then((res) => {
        if (res && 'tokens' in res) {
          return res as TraceResponseV1;
        }
        return null;
      }),
    enabled: Boolean(traceId),
    retry: false,
  });
}
