/**
 * useWorkspaceActiveState - Fetch and track workspace active state
 *
 * Provides real-time workspace state including active base model, plan,
 * adapters, manifest hash, and policy mask digest.
 *
 * @example
 * ```tsx
 * const {
 *   workspaceActiveState,
 *   loading,
 *   refetch,
 * } = useWorkspaceActiveState({
 *   tenantId: 'my-tenant',
 * });
 * ```
 */

import { useState, useEffect, useCallback } from 'react';
import { apiClient } from '@/api/services';
import { logger, toError } from '@/utils/logger';

// ============================================================================
// Types
// ============================================================================

/**
 * Workspace active state shape
 */
export interface WorkspaceActiveState {
  activeBaseModelId?: string | null;
  activePlanId?: string | null;
  activeAdapterIds?: string[] | null;
  manifestHashB3?: string | null;
  policyMaskDigestB3?: string | null;
  updatedAt?: string | null;
}

/**
 * Hook configuration options
 */
export interface UseWorkspaceActiveStateOptions {
  /** Tenant ID to fetch workspace state for */
  tenantId: string;
}

/** Data source for workspace state */
export type WorkspaceStateSource = 'canonical' | 'legacy' | null;

/**
 * Hook return value
 */
export interface UseWorkspaceActiveStateReturn {
  /** Current workspace active state */
  workspaceActiveState: WorkspaceActiveState | null;
  /** True if currently loading */
  loading: boolean;
  /** Data source: 'canonical' (v1/workspaces/:id/active), 'legacy' (v1/workspaces/active-state), or null */
  source: WorkspaceStateSource;
  /** True if data came from legacy endpoint */
  isLegacy: boolean;
  /** Refetch workspace state */
  refetch: () => Promise<void>;
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * Fetch and track workspace active state
 *
 * Features:
 * - Canonical endpoint with legacy fallback
 * - Automatic refetch on tenant change
 * - Manual refetch capability
 */
export function useWorkspaceActiveState(
  options: UseWorkspaceActiveStateOptions
): UseWorkspaceActiveStateReturn {
  const { tenantId } = options;

  const [workspaceActiveState, setWorkspaceActiveState] = useState<WorkspaceActiveState | null>(null);
  const [loading, setLoading] = useState(false);
  const [source, setSource] = useState<WorkspaceStateSource>(null);

  const refetch = useCallback(async () => {
    setLoading(true);
    let resolved = false;

    try {
      const canonicalPath = `/v1/workspaces/${encodeURIComponent(tenantId)}/active`;
      const response = await apiClient.request<WorkspaceActiveState>(canonicalPath);
      setWorkspaceActiveState(response);
      setSource('canonical');
      resolved = true;
    } catch (err) {
      logger.warn(
        'Failed to fetch workspace active state via canonical endpoint',
        { component: 'useWorkspaceActiveState', tenantId, hint: 'workspace_active_state' },
        toError(err)
      );
    }

    if (!resolved) {
      try {
        const query = tenantId ? `?tenant_id=${encodeURIComponent(tenantId)}` : '';
        const response = await apiClient.request<WorkspaceActiveState>(`/v1/workspaces/active-state${query}`);
        setWorkspaceActiveState(response);
        setSource('legacy');
        resolved = true;
        logger.warn('Workspace active state loaded via legacy endpoint', {
          component: 'useWorkspaceActiveState',
          tenantId,
          hint: 'workspace_active_state',
        });
      } catch (err) {
        logger.warn(
          'Failed to fetch workspace active state via legacy endpoint',
          { component: 'useWorkspaceActiveState', tenantId, hint: 'workspace_active_state' },
          toError(err)
        );
      }
    }

    if (!resolved) {
      setWorkspaceActiveState(null);
      setSource(null);
    }

    setLoading(false);
  }, [tenantId]);

  useEffect(() => {
    void refetch();
  }, [refetch]);

  return {
    workspaceActiveState,
    loading,
    source,
    isLegacy: source === 'legacy',
    refetch,
  };
}
