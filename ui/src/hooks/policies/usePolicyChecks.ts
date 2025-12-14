import { useState, useCallback, useEffect } from 'react';
import { PolicyCheck, PolicyStatus, PolicyCategory, PolicySeverity } from '@/components/golden/PolicyCheckDisplay';
import apiClient from '@/api/client';
import { Policy } from '@/api/types';
import { logger } from '@/utils/logger';

// Transform API Policy to UI PolicyCheck
function policyToPolicyCheck(policy: Policy): PolicyCheck {
  // Map policy status to PolicyStatus
  const statusMap: Record<string, PolicyStatus> = {
    active: 'passed',
    draft: 'pending',
    archived: 'warning',
  };

  return {
    id: policy.id,
    name: policy.name,
    description: policy.content || '',
    status: statusMap[policy.status] || 'pending',
    category: (policy.type as PolicyCategory) || 'compliance',
    severity: 'medium' as PolicySeverity,
    message: policy.enabled ? 'Policy is enabled' : 'Policy is disabled',
    canOverride: policy.status !== 'archived',
  };
}

export interface UsePolicyChecksOptions {
  cpid: string;
  autoFetch?: boolean;
}

export interface UsePolicyChecksResult {
  policies: PolicyCheck[];
  isLoading: boolean;
  error: Error | null;
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
  const [error, setError] = useState<Error | null>(null);

  const fetchPolicies = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);

      // Fetch policies from the API
      // This assumes the API client has a method to get policy checks
      // Adjust the endpoint as needed based on your API contract
      const response = await apiClient.getPolicy(cpid);
      // Transform API Policy to UI PolicyCheck and wrap in array
      setPolicies(response ? [policyToPolicyCheck(response)] : []);

      logger.info('Policies fetched successfully', {
        component: 'usePolicyChecks',
        cpid,
        count: response ? 1 : 0,
      });
    } catch (err) {
      setError(err instanceof Error ? err : new Error('Failed to fetch policies'));
      logger.error('Failed to fetch policies', { component: 'usePolicyChecks', cpid }, err instanceof Error ? err : new Error(String(err)));
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
        // Use updatePolicy to apply override - content includes the override metadata
        await apiClient.updatePolicy(cpid, JSON.stringify({
          policyId,
          reason,
          overriddenAt: new Date().toISOString(),
        }));

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
        setError(err instanceof Error ? err : new Error('Failed to override policy'));
        logger.error('Failed to override policy', { component: 'usePolicyChecks', cpid, policyId }, err instanceof Error ? err : new Error(String(err)));
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
    isLoading: loading,
    error,
    refetch: fetchPolicies,
    overridePolicy,
  };
}

export default usePolicyChecks;
