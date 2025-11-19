import { useState, useCallback, useEffect } from 'react';
import { PolicyCheck } from '../components/golden/PolicyCheckDisplay';
import apiClient from '../api/client';
import { logger } from '../utils/logger';

export interface UsePolicyChecksOptions {
  cpid: string;
  autoFetch?: boolean;
}

export interface UsePolicyChecksResult {
  policies: PolicyCheck[];
  loading: boolean;
  error: string | null;
  refetch: () => Promise<void>;
  overridePolicy: (policyId: string, reason: string) => Promise<void>;
}

/**
 * Hook to fetch and manage policy checks for a given CPID.
 * Provides methods to retrieve policies, handle overrides, and manage loading/error states.
 */
export function usePolicyChecks({
  cpid,
  autoFetch = true,
}: UsePolicyChecksOptions): UsePolicyChecksResult {
  const [policies, setPolicies] = useState<PolicyCheck[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchPolicies = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);

      // Fetch policies from the API
      // This assumes the API client has a method to get policy checks
      // Adjust the endpoint as needed based on your API contract
      const response = await apiClient.getPolicies(cpid);
      setPolicies(response.policies || []);

      logger.info('Policies fetched successfully', {
        component: 'usePolicyChecks',
        cpid,
        count: response.policies?.length || 0,
      });
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to fetch policies';
      setError(message);
      logger.error('Failed to fetch policies', { component: 'usePolicyChecks', cpid }, err);
    } finally {
      setLoading(false);
    }
  }, [cpid]);

  const overridePolicy = useCallback(
    async (policyId: string, reason: string) => {
      try {
        setLoading(true);
        setError(null);

        // Call the override endpoint
        // This assumes the API client has a method to override policies
        await apiClient.overridePolicy(cpid, policyId, {
          reason,
          overriddenAt: new Date().toISOString(),
        });

        // Update the local state to mark the policy as overridden
        setPolicies(prevPolicies =>
          prevPolicies.map(p =>
            p.id === policyId
              ? {
                  ...p,
                  status: 'passed',
                  overrideReason: reason,
                }
              : p,
          ),
        );

        logger.info('Policy override applied', {
          component: 'usePolicyChecks',
          cpid,
          policyId,
        });
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to override policy';
        setError(message);
        logger.error('Failed to override policy', { component: 'usePolicyChecks', cpid, policyId }, err);
        throw err;
      } finally {
        setLoading(false);
      }
    },
    [cpid],
  );

  // Auto-fetch on mount if cpid is provided
  useEffect(() => {
    if (autoFetch && cpid) {
      fetchPolicies();
    }
  }, [cpid, autoFetch, fetchPolicies]);

  return {
    policies,
    loading,
    error,
    refetch: fetchPolicies,
    overridePolicy,
  };
}

export default usePolicyChecks;
