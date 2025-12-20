import { useMutation, useQuery, useQueryClient, type UseMutationOptions, type UseQueryOptions } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import { createResourceHooks } from '@/hooks/factories/createApiHooks';
import { QUERY_STANDARD } from '@/api/queryOptions';
import { useTenant } from '@/providers/FeatureProviders';
import { withTenantKey } from '@/utils/tenant';
import type {
  CreateRepoRequest,
  RepoDetail,
  RepoSummary,
  RepoTimelineEvent,
  RepoTrainingJobLink,
  RepoVersionDetail,
  RepoVersionSummary,
  StartTrainingFromVersionRequest,
  TagVersionRequest,
  UpdateRepoRequest,
} from '@/api/repo-types';

const repoHooks = createResourceHooks<RepoSummary, RepoDetail, CreateRepoRequest, UpdateRepoRequest>({
  resourceName: 'repos',
  api: {
    list: () => apiClient.listRepos(),
    get: (id: string) => apiClient.getRepo(id),
    create: (payload: CreateRepoRequest) => apiClient.createRepo(payload),
    update: (id: string, data: UpdateRepoRequest) => apiClient.updateRepo(id, data),
  },
  staleTime: QUERY_STANDARD.staleTime as number,
});

export const repoKeys = repoHooks.keys;
export const useRepos = repoHooks.useList;
export const useRepo = repoHooks.useDetail;
export const useCreateRepo = repoHooks.useCreate;
export const useUpdateRepo = repoHooks.useUpdate;

export const repoVersionKeys = {
  all: ['repo-versions'] as const,
  list: (repoId: string) => ['repo-versions', 'list', repoId] as const,
  detail: (repoId: string, versionId: string) => ['repo-versions', 'detail', repoId, versionId] as const,
  timeline: (repoId: string) => ['repo-timeline', repoId] as const,
  trainingJobs: (repoId: string) => ['repo-training-jobs', repoId] as const,
};

export function useRepoVersions(
  repoId: string | undefined,
  options?: Partial<UseQueryOptions<RepoVersionSummary[], Error>>
) {
  const { selectedTenant } = useTenant();
  return useQuery({
    queryKey: repoId
      ? withTenantKey(repoVersionKeys.list(repoId), selectedTenant)
      : withTenantKey(['repo-versions', 'list', 'missing'], selectedTenant),
    queryFn: () => apiClient.listRepoVersions(repoId!),
    enabled: Boolean(repoId && selectedTenant),
    staleTime: QUERY_STANDARD.staleTime as number,
    ...options,
  });
}

export function useRepoVersion(
  repoId: string | undefined,
  versionId: string | undefined,
  options?: Partial<UseQueryOptions<RepoVersionDetail, Error>>
) {
  const { selectedTenant } = useTenant();
  return useQuery({
    queryKey: repoId && versionId
      ? withTenantKey(repoVersionKeys.detail(repoId, versionId), selectedTenant)
      : withTenantKey(['repo-versions', 'detail', 'missing'], selectedTenant),
    queryFn: () => apiClient.getRepoVersion(repoId!, versionId!),
    enabled: Boolean(repoId && versionId && selectedTenant),
    staleTime: QUERY_STANDARD.staleTime as number,
    ...options,
  });
}

export function useRepoTimeline(
  repoId: string | undefined,
  options?: Partial<UseQueryOptions<RepoTimelineEvent[], Error>>
) {
  const { selectedTenant } = useTenant();
  return useQuery({
    queryKey: repoId
      ? withTenantKey(repoVersionKeys.timeline(repoId), selectedTenant)
      : withTenantKey(['repo-timeline', 'missing'], selectedTenant),
    queryFn: () => apiClient.getRepoTimeline(repoId!),
    enabled: Boolean(repoId && selectedTenant),
    staleTime: QUERY_STANDARD.staleTime as number,
    ...options,
  });
}

export function useRepoTrainingJobs(
  repoId: string | undefined,
  options?: Partial<UseQueryOptions<RepoTrainingJobLink[], Error>>
) {
  const { selectedTenant } = useTenant();
  return useQuery({
    queryKey: repoId
      ? withTenantKey(repoVersionKeys.trainingJobs(repoId), selectedTenant)
      : withTenantKey(['repo-training-jobs', 'missing'], selectedTenant),
    queryFn: () => apiClient.listRepoTrainingJobs(repoId!),
    enabled: Boolean(repoId && selectedTenant),
    staleTime: QUERY_STANDARD.staleTime as number,
    ...options,
  });
}

export function usePromoteRepoVersion(
  repoId: string,
  options?: UseMutationOptions<RepoVersionDetail, Error, { versionId: string }>
) {
  const queryClient = useQueryClient();
  const { selectedTenant } = useTenant();
  const { onSuccess, ...rest } = options ?? {};
  return useMutation<RepoVersionDetail, Error, { versionId: string }>({
    mutationFn: ({ versionId }) => apiClient.promoteRepoVersion(repoId, versionId, {}),
    ...rest,
    onSuccess: async (data, variables, ...args) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: withTenantKey(repoVersionKeys.list(repoId), selectedTenant) }),
        queryClient.invalidateQueries({ queryKey: withTenantKey(repoVersionKeys.detail(repoId, variables.versionId), selectedTenant) }),
        queryClient.invalidateQueries({ queryKey: withTenantKey(repoKeys.detail(repoId), selectedTenant) }),
      ]);
      await onSuccess?.(data, variables, ...args);
    },
  });
}

export function useRollbackRepoVersion(
  repoId: string,
  options?: UseMutationOptions<RepoVersionDetail, Error, { versionId: string; reason?: string }>
) {
  const queryClient = useQueryClient();
  const { selectedTenant } = useTenant();
  const { onSuccess, ...rest } = options ?? {};
  return useMutation<RepoVersionDetail, Error, { versionId: string; reason?: string }>({
    mutationFn: ({ versionId, reason }) =>
      apiClient.rollbackRepoVersion(repoId, versionId, { reason }),
    ...rest,
    onSuccess: async (data, variables, ...args) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: withTenantKey(repoVersionKeys.list(repoId), selectedTenant) }),
        queryClient.invalidateQueries({ queryKey: withTenantKey(repoVersionKeys.detail(repoId, variables.versionId), selectedTenant) }),
        queryClient.invalidateQueries({ queryKey: withTenantKey(repoVersionKeys.timeline(repoId), selectedTenant) }),
      ]);
      await onSuccess?.(data, variables, ...args);
    },
  });
}

export function useTagRepoVersion(
  repoId: string,
  options?: UseMutationOptions<RepoVersionDetail, Error, { versionId: string; payload: TagVersionRequest }>
) {
  const queryClient = useQueryClient();
  const { selectedTenant } = useTenant();
  const { onSuccess, ...rest } = options ?? {};
  return useMutation<RepoVersionDetail, Error, { versionId: string; payload: TagVersionRequest }>({
    mutationFn: ({ versionId, payload }) => apiClient.tagRepoVersion(repoId, versionId, payload),
    ...rest,
    onSuccess: async (data, variables, ...args) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: withTenantKey(repoVersionKeys.detail(repoId, variables.versionId), selectedTenant) }),
        queryClient.invalidateQueries({ queryKey: withTenantKey(repoVersionKeys.list(repoId), selectedTenant) }),
      ]);
      await onSuccess?.(data, variables, ...args);
    },
  });
}

export function useStartTrainingFromVersion(
  repoId: string,
  options?: UseMutationOptions<RepoTrainingJobLink, Error, { versionId: string; payload: StartTrainingFromVersionRequest }>
) {
  const queryClient = useQueryClient();
  const { selectedTenant } = useTenant();
  const { onSuccess, ...rest } = options ?? {};
  return useMutation<RepoTrainingJobLink, Error, { versionId: string; payload: StartTrainingFromVersionRequest }>({
    mutationFn: ({ versionId, payload }) => apiClient.startTrainingFromVersion(repoId, versionId, payload),
    ...rest,
    onSuccess: async (data, variables, ...args) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: withTenantKey(repoVersionKeys.trainingJobs(repoId), selectedTenant) }),
        queryClient.invalidateQueries({ queryKey: withTenantKey(repoVersionKeys.detail(repoId, variables.versionId), selectedTenant) }),
      ]);
      await onSuccess?.(data, variables, ...args);
    },
  });
}
