import { useState, useEffect } from 'react';
import { usePolling } from '@/hooks/realtime/usePolling';
import { apiClient } from '@/api/services';
import { Tenant as ApiTenant, Policy, Adapter } from '@/api/types';
import { logger } from '@/utils/logger';

export interface UseTenantsDataOptions {
  userId?: string;
  canManage: boolean;
}

export interface UseTenantsDataReturn {
  tenants: ApiTenant[];
  policies: Policy[];
  adapters: Adapter[];
  isLoading: boolean;
  error: Error | null;
  refetch: () => void;
}

export function useTenantsData(options: UseTenantsDataOptions): UseTenantsDataReturn {
  const { userId, canManage } = options;
  const [tenants, setTenants] = useState<ApiTenant[]>([]);
  const [policies, setPolicies] = useState<Policy[]>([]);
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [error, setError] = useState<Error | null>(null);

  // Use polling for tenant list updates
  const {
    data: polledTenants,
    isLoading: pollingLoading,
    error: pollingError,
    refetch: refetchTenants,
  } = usePolling<ApiTenant[]>(
    () => apiClient.listTenants(),
    'slow',
    {
      enabled: canManage,
      operationName: 'fetchTenants',
      onSuccess: (data) => {
        setTenants(data as ApiTenant[]);
        setError(null);
      },
      onError: (err) => {
        logger.error('Failed to fetch tenants', {
          component: 'useTenantsData',
          operation: 'fetchTenants',
          userId,
        }, err);
        setError(err);
      },
    }
  );

  // Update tenants when polling data changes
  useEffect(() => {
    if (polledTenants) {
      setTenants(polledTenants);
    }
  }, [polledTenants]);

  // Fetch policies and adapters on mount
  useEffect(() => {
    const fetchData = async () => {
      try {
        const [policiesData, adaptersData] = await Promise.all([
          apiClient.listPolicies(),
          apiClient.listAdapters(),
        ]);
        setPolicies(policiesData);
        setAdapters(adaptersData);
      } catch (err) {
        logger.error('Failed to fetch policies/adapters', {
          component: 'useTenantsData',
          operation: 'fetchPoliciesAdapters',
          userId,
        }, err instanceof Error ? err : new Error(String(err)));
      }
    };
    fetchData();
  }, [userId]);

  return {
    tenants,
    policies,
    adapters,
    isLoading: pollingLoading,
    error: error || pollingError || null,
    refetch: refetchTenants,
  };
}
