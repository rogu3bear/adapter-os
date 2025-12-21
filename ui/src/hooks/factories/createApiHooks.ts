/**
 * Factory for creating standardized React Query API hooks
 *
 * This factory generates consistent CRUD hooks with cache invalidation
 * for any API resource, reducing boilerplate across the codebase.
 *
 * @example
 * ```ts
 * const documentHooks = createResourceHooks({
 *   resourceName: 'documents',
 *   api: {
 *     list: () => apiClient.listDocuments(),
 *     get: (id) => apiClient.getDocument(id),
 *     delete: (id) => apiClient.deleteDocument(id),
 *   },
 * });
 *
 * // Use in components:
 * const { data } = documentHooks.useList();
 * const { data } = documentHooks.useDetail(id);
 * ```
 */

import {
  useQuery,
  useMutation,
  useQueryClient,
  UseQueryOptions,
  UseMutationOptions,
  QueryClient,
} from '@tanstack/react-query';
import { useTenant } from '@/providers/FeatureProviders';
import { withTenantKey } from '@/utils/tenant';

/**
 * Configuration for creating resource hooks
 */
export interface ResourceHooksConfig<
  TList,
  TDetail,
  TCreate = unknown,
  TUpdate = unknown,
  TCreateResult = TDetail,
> {
  /** Unique name for the resource (used as query key prefix) */
  resourceName: string;

  /** API methods for the resource */
  api: {
    /** Fetch list of resources */
    list?: () => Promise<TList[]>;
    /** Fetch single resource by ID */
    get?: (id: string) => Promise<TDetail>;
    /** Create new resource (can return different type than detail) */
    create?: (data: TCreate) => Promise<TCreateResult>;
    /** Update existing resource */
    update?: (id: string, data: TUpdate) => Promise<TDetail>;
    /** Delete resource by ID */
    delete?: (id: string) => Promise<void>;
  };

  /** Default stale time in milliseconds */
  staleTime?: number;

  /** Additional query keys to invalidate on mutations */
  invalidatesOnMutate?: string[];

  /** Error messages for centralized error handling */
  errorMessages?: {
    list?: string;
    detail?: string;
  };
}

/**
 * Query key factory for a resource
 */
export interface ResourceQueryKeys {
  all: readonly string[];
  lists: () => readonly string[];
  list: () => readonly string[];
  details: () => readonly string[];
  detail: (id: string) => readonly string[];
}

/**
 * Create query keys for a resource
 */
export function createQueryKeys(resourceName: string): ResourceQueryKeys {
  return {
    all: [resourceName] as const,
    lists: () => [resourceName, 'list'] as const,
    list: () => [resourceName, 'list'] as const,
    details: () => [resourceName, 'detail'] as const,
    detail: (id: string) => [resourceName, 'detail', id] as const,
  };
}

/**
 * Return type for created resource hooks
 */
export interface ResourceHooks<TList, TDetail, TCreate, TUpdate, TCreateResult = TDetail> {
  /** Query keys for cache management */
  keys: ResourceQueryKeys;

  /** Hook for listing all resources */
  useList: (options?: Partial<UseQueryOptions<TList[], Error>>) => ReturnType<typeof useQuery<TList[], Error>>;

  /** Hook for getting a single resource */
  useDetail: (
    id: string | undefined,
    options?: Partial<UseQueryOptions<TDetail, Error>>
  ) => ReturnType<typeof useQuery<TDetail, Error>>;

  /** Hook for creating a resource */
  useCreate: (
    options?: UseMutationOptions<TCreateResult, Error, TCreate>
  ) => ReturnType<typeof useMutation<TCreateResult, Error, TCreate>>;

  /** Hook for updating a resource */
  useUpdate: (
    options?: UseMutationOptions<TDetail, Error, { id: string; data: TUpdate }>
  ) => ReturnType<typeof useMutation<TDetail, Error, { id: string; data: TUpdate }>>;

  /** Hook for deleting a resource */
  useDelete: (
    options?: UseMutationOptions<void, Error, string>
  ) => ReturnType<typeof useMutation<void, Error, string>>;

  /** Invalidate all queries for this resource */
  invalidateAll: (queryClient: QueryClient, tenantId?: string | null) => Promise<void>;
}

/**
 * Create standardized React Query hooks for a resource
 */
export function createResourceHooks<
  TList,
  TDetail = TList,
  TCreate = Partial<TDetail>,
  TUpdate = Partial<TDetail>,
  TCreateResult = TDetail,
>(config: ResourceHooksConfig<TList, TDetail, TCreate, TUpdate, TCreateResult>): ResourceHooks<TList, TDetail, TCreate, TUpdate, TCreateResult> {
  const { resourceName, api, staleTime = 30000, invalidatesOnMutate = [], errorMessages = {} } = config;
  const keys = createQueryKeys(resourceName);

  // Invalidation helper
  const invalidateRelated = (queryClient: QueryClient, tenantId?: string | null) => {
    const scopedAll = withTenantKey(keys.all, tenantId);
    queryClient.invalidateQueries({ queryKey: scopedAll });
    invalidatesOnMutate.forEach((key) => {
      queryClient.invalidateQueries({ queryKey: withTenantKey([key], tenantId) });
    });
  };

  return {
    keys,

    useList: (options = {}) => {
      const { selectedTenant } = useTenant();
      return useQuery({
        queryKey: withTenantKey(keys.list(), selectedTenant),
        queryFn: api.list ?? (() => Promise.resolve([] as TList[])),
        staleTime,
        enabled: !!api.list && !!selectedTenant,
        ...(errorMessages.list && { meta: { errorMessage: errorMessages.list } }),
        ...options,
      });
    },

    useDetail: (id, options = {}) => {
      const { selectedTenant } = useTenant();
      return useQuery({
        queryKey: withTenantKey(keys.detail(id ?? ''), selectedTenant),
        queryFn: () => api.get!(id!),
        staleTime,
        enabled: !!id && !!api.get && !!selectedTenant,
        ...(errorMessages.detail && { meta: { errorMessage: errorMessages.detail } }),
        ...options,
      });
    },

    useCreate: (options = {}) => {
      const queryClient = useQueryClient();
      const { selectedTenant } = useTenant();
      const { onSuccess, ...restOptions } = options;
      return useMutation<TCreateResult, Error, TCreate>({
        mutationFn: api.create ?? (() => Promise.reject(new Error('Create not implemented'))),
        ...restOptions,
        onSuccess: async (...args) => {
          invalidateRelated(queryClient, selectedTenant);
          await onSuccess?.(...args);
        },
      });
    },

    useUpdate: (options = {}) => {
      const queryClient = useQueryClient();
      const { selectedTenant } = useTenant();
      const { onSuccess, ...restOptions } = options;
      return useMutation<TDetail, Error, { id: string; data: TUpdate }>({
        mutationFn: ({ id, data }: { id: string; data: TUpdate }) =>
          api.update?.(id, data) ?? Promise.reject(new Error('Update not implemented')),
        ...restOptions,
        onSuccess: async (data, variables, ...rest) => {
          queryClient.invalidateQueries({ queryKey: withTenantKey(keys.detail(variables.id), selectedTenant) });
          invalidateRelated(queryClient, selectedTenant);
          await onSuccess?.(data, variables, ...rest);
        },
      });
    },

    useDelete: (options = {}) => {
      const queryClient = useQueryClient();
      const { selectedTenant } = useTenant();
      const { onSuccess, ...restOptions } = options;
      return useMutation<void, Error, string>({
        mutationFn: api.delete ?? (() => Promise.reject(new Error('Delete not implemented'))),
        ...restOptions,
        onSuccess: async (data, id, ...rest) => {
          queryClient.removeQueries({ queryKey: withTenantKey(keys.detail(id), selectedTenant) });
          invalidateRelated(queryClient, selectedTenant);
          await onSuccess?.(data, id, ...rest);
        },
      });
    },

    invalidateAll: async (queryClient: QueryClient, tenantId?: string | null) => {
      await queryClient.invalidateQueries({ queryKey: withTenantKey(keys.all, tenantId) });
    },
  };
}

export default createResourceHooks;
