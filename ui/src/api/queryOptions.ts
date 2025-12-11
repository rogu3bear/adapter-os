import type { DefaultOptions, QueryClientConfig } from '@tanstack/react-query';

const MINUTE_MS = 60_000;

type QueryPreset = Pick<NonNullable<DefaultOptions['queries']>, 'staleTime' | 'refetchInterval' | 'refetchOnWindowFocus' | 'retry' | 'gcTime'>;

// Query presets documented in ui/docs/query-policy.md
export const QUERY_FAST: QueryPreset = {
  staleTime: 15_000,
  refetchInterval: 30_000,
  refetchOnWindowFocus: true,
  retry: 1,
};

export const QUERY_STANDARD: QueryPreset = {
  staleTime: 5 * MINUTE_MS,
  refetchOnWindowFocus: false,
  retry: 1,
};

export const QUERY_RARE: QueryPreset = {
  staleTime: 60 * MINUTE_MS,
  gcTime: 120 * MINUTE_MS,
  refetchOnWindowFocus: false,
  retry: 0,
};

export const queryClientOptions: QueryClientConfig = {
  defaultOptions: {
    queries: {
      ...QUERY_STANDARD,
      refetchOnReconnect: true,
      refetchOnMount: false,
      retryOnMount: false,
    },
    mutations: {
      retry: 0,
    },
  },
};

