/**
 * useSecurity Hook - React Query hooks for Security operations
 *
 * Provides hooks for:
 * - Policies (list, detail, validate, apply, sign, compare, export)
 * - Audit logs (query with filters, export)
 * - Compliance (audit reports, controls, violations)
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/api/client';
import type {
  Policy,
  PolicyComparisonResponse,
  SignPolicyResponse,
  ExportPolicyResponse,
  AuditLog,
  AuditLogFilters,
  ComplianceAuditResponse,
  PolicyPackResponse,
} from '@/api/types';

// Query Keys
export const securityKeys = {
  all: ['security'] as const,
  policies: () => [...securityKeys.all, 'policies'] as const,
  policy: (cpid: string) => [...securityKeys.policies(), cpid] as const,
  auditLogs: (filters?: AuditLogFilters) => [...securityKeys.all, 'audit-logs', filters] as const,
  compliance: () => [...securityKeys.all, 'compliance'] as const,
};

// Policies Hooks

/**
 * Hook to list all policies
 */
export function usePolicies() {
  const query = useQuery({
    queryKey: securityKeys.policies(),
    queryFn: () => apiClient.listPolicies(),
  });

  return {
    policies: query.data,
    isLoading: query.isLoading,
    error: query.error,
    refetch: query.refetch,
  };
}

/**
 * Hook to get a single policy by CPID
 */
export function usePolicyDetail(cpid: string | undefined) {
  const query = useQuery({
    queryKey: cpid ? securityKeys.policy(cpid) : [],
    queryFn: () => (cpid ? apiClient.getPolicy(cpid) : Promise.resolve(null)),
    enabled: !!cpid,
  });

  return {
    policy: query.data,
    isLoading: query.isLoading,
    error: query.error,
    refetch: query.refetch,
  };
}

/**
 * Hook for policy mutations (validate, apply, sign, compare, export)
 */
export function usePolicyMutations() {
  const queryClient = useQueryClient();

  const validatePolicy = useMutation({
    mutationFn: (data: { policy_json: string; policy_type?: string }) =>
      apiClient.validatePolicy(data),
  });

  const applyPolicy = useMutation({
    mutationFn: (data: { cpid: string; content: string }) =>
      apiClient.applyPolicy(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: securityKeys.policies() });
    },
  });

  const signPolicy = useMutation({
    mutationFn: (cpid: string) => apiClient.signPolicy(cpid),
    onSuccess: (_, cpid) => {
      queryClient.invalidateQueries({ queryKey: securityKeys.policy(cpid) });
      queryClient.invalidateQueries({ queryKey: securityKeys.policies() });
    },
  });

  const comparePolicy = useMutation({
    mutationFn: (data: { cpid1: string; cpid2: string }) =>
      apiClient.comparePolicies(data.cpid1, data.cpid2),
  });

  const exportPolicy = useMutation({
    mutationFn: (cpid: string) => apiClient.exportPolicy(cpid),
  });

  return {
    validatePolicy: validatePolicy.mutateAsync,
    isValidatingPolicy: validatePolicy.isPending,

    applyPolicy: applyPolicy.mutateAsync,
    isApplyingPolicy: applyPolicy.isPending,

    signPolicy: signPolicy.mutateAsync,
    isSigningPolicy: signPolicy.isPending,

    comparePolicy: comparePolicy.mutateAsync,
    isComparingPolicy: comparePolicy.isPending,

    exportPolicy: exportPolicy.mutateAsync,
    isExportingPolicy: exportPolicy.isPending,
  };
}

// Audit Logs Hooks

/**
 * Hook to query audit logs with optional filters
 */
export function useAuditLogs(filters?: AuditLogFilters) {
  const query = useQuery({
    queryKey: securityKeys.auditLogs(filters),
    queryFn: () => apiClient.queryAuditLogs(filters),
  });

  return {
    auditLogs: query.data,
    isLoading: query.isLoading,
    error: query.error,
    refetch: query.refetch,
  };
}

/**
 * Hook to export audit logs
 */
export function useExportAuditLogs() {
  const mutation = useMutation({
    mutationFn: (params?: {
      format?: 'csv' | 'json';
      startTime?: string;
      endTime?: string;
      tenantId?: string;
      eventType?: string;
      level?: string;
    }) => apiClient.exportAuditLogs(params),
  });

  return {
    exportAuditLogs: mutation.mutateAsync,
    isExporting: mutation.isPending,
  };
}

// Compliance Hooks

/**
 * Hook to get compliance audit report
 */
export function useComplianceAudit() {
  const query = useQuery({
    queryKey: securityKeys.compliance(),
    queryFn: () => apiClient.getComplianceAudit(),
  });

  return {
    complianceAudit: query.data,
    isLoading: query.isLoading,
    error: query.error,
    refetch: query.refetch,
  };
}

/**
 * Combined hook for all policies operations
 * @deprecated Use individual hooks instead (usePolicies, usePolicyMutations)
 */
export function usePoliciesLegacy() {
  const { policies, isLoading, error, refetch } = usePolicies();
  const mutations = usePolicyMutations();

  return {
    policies,
    isLoading,
    error,
    refetch,
    ...mutations,
  };
}
