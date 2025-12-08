# Query Policy and Cache Helpers

This note documents the shared query presets and cache invalidation helpers used in the UI. The goal is to keep React Query behavior predictable across dashboards, adapters, telemetry, and models while avoiding accidental stale views.

## Presets

Presets live in `src/api/queryOptions.ts` and can be spread into any `useQuery` call.

- `QUERY_FAST` — `staleTime: 15s`, `refetchInterval: 30s`, window refetch enabled. Use for live-ish dashboards and telemetry.
- `QUERY_STANDARD` — `staleTime: 5m`, window refetch off, retry once. Default for most lists/detail views.
- `QUERY_RARE` — `staleTime: 60m`, `gcTime: 2h`, no retries. Use for static assets (models, manifests, docs).

## Presets usage rules

- `QUERY_FAST`: user-facing status or state that should feel live (adapter lifecycle, telemetry streams, service health, fast-changing dashboards).
- `QUERY_STANDARD`: default for most lists and detail views where a 10–60s view lag is acceptable.
- `QUERY_RARE`: static or rarely changing metadata (models, tenants, static config) with explicit overrides when a page needs a longer cache.

## Invalidation helpers

- `invalidateAdapters(queryClient)` from `useAdaptersApi` — clears adapter list/detail + related metrics.
- `invalidateDashboard(queryClient)` from `api/queryInvalidation` — clears dashboard/metrics-prefixed queries.
- `invalidateTelemetry(queryClient)` — clears session telemetry (sessions, steps, metrics series).
- `invalidateModels(queryClient)` — clears owner/base model lists and status.
- `invalidateTrainingCaches(queryClient)` — shared training list/dataset invalidation.

All helpers accept an optional `QueryClient`; omit to use the app singleton via `getQueryClient`.
Call `invalidateDashboard` after mutations that change what the dashboard shows (adapter lifecycle changes, health-affecting ops, metrics refresh triggers). Use telemetry/models/training helpers immediately after mutations that impact their respective resources. Rule: always use these invalidation helpers instead of ad-hoc `invalidateQueries` calls.

## Usage pattern

```ts
const { data } = useQuery({
  queryKey: ['adapters', tenantId],
  queryFn: () => apiClient.listAdapters({ tenantId }),
  ...QUERY_FAST,
});

const { mutateAsync: promote } = usePromoteAdapter({
  onSuccess: async () => {
    await invalidateAdapters(queryClient);
    await invalidateDashboard(queryClient);
  },
});
```

## Examples

- Fast: hook for live adapter state
```ts
const { data: adaptersLive } = useQuery({
  queryKey: ['adapters', 'live'],
  queryFn: () => apiClient.listAdapters(),
  ...QUERY_FAST,
});
```

- Standard: typical list/detail view
```ts
const { data: trainingJobs } = useQuery({
  queryKey: ['training', 'jobs'],
  queryFn: () => apiClient.listTrainingJobs(),
  ...QUERY_STANDARD,
});
```

- Rare + page-specific override
```ts
// In a page component: static catalog that can stay warm longer
const { data: modelCatalog } = useQuery({
  queryKey: ['models', 'catalog'],
  queryFn: () => apiClient.listOwnerModels(),
  ...QUERY_RARE,
  staleTime: 12 * 60 * 60 * 1000, // explicit override
});
```

## Guidance

- Prefer presets over one-off numbers for new queries.
- Choose the smallest preset that keeps UI fresh; avoid refetchOnWindowFocus for heavy lists unless using `QUERY_FAST`.
- Pair mutations with the narrowest invalidation helper; avoid `queryClient.clear()` or global wipes.
- Keep query keys structured (`['feature', 'scope', params]`) so helper prefixes work reliably.

MLNavigator Inc 2025-12-08.

