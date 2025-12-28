import { useCallback } from 'react';
import { apiClient } from '@/api/services';
import { Tenant as ApiTenant, TenantUsageResponse } from '@/api/types';
import { logger } from '@/utils/logger';
import { UndoableAction } from '@/hooks/ui/useUndoRedo';

export type PartialUndoableAction<T = unknown> = Omit<UndoableAction<T>, 'id' | 'timestamp'>;

export interface TenantOperationCallbacks {
  onSuccess: (message: string) => void;
  onError: (errorKey: string, message: string, retryFn?: () => void) => void;
  clearError: (errorKey: string) => void;
  refetchTenants: () => void;
  addAction: <T = unknown>(action: PartialUndoableAction<T>) => void;
}

export interface UseTenantOperationsOptions {
  callbacks: TenantOperationCallbacks;
  canManage: boolean;
}

export interface UseTenantOperationsReturn {
  handleEdit: (tenant: ApiTenant, newName: string) => Promise<void>;
  handleArchive: (tenant: ApiTenant) => Promise<void>;
  handlePause: (tenant: ApiTenant) => Promise<void>;
  handleAssignPolicies: (tenant: ApiTenant, policyIds: string[]) => Promise<void>;
  handleAssignAdapters: (tenant: ApiTenant, adapterIds: string[]) => Promise<void>;
  handleViewUsage: (tenant: ApiTenant) => Promise<TenantUsageResponse | null>;
  handleCreate: (name: string) => Promise<void>;
}

export function useTenantOperations(
  options: UseTenantOperationsOptions
): UseTenantOperationsReturn {
  const { callbacks, canManage } = options;
  const { onSuccess, onError, clearError, refetchTenants, addAction } = callbacks;

  const handleEdit = useCallback(
    async (tenant: ApiTenant, newName: string) => {
      if (!canManage) return;
      try {
        await apiClient.updateTenant(tenant.id, newName);
        onSuccess('Workspace updated.');
        clearError('edit-tenant');
        refetchTenants();
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : 'Failed to update workspace';
        onError('edit-tenant', errorMsg, () => handleEdit(tenant, newName));
      }
    },
    [canManage, onSuccess, onError, clearError, refetchTenants]
  );

  const handleArchive = useCallback(
    async (tenant: ApiTenant) => {
      if (!canManage) return;
      try {
        await apiClient.archiveTenant(tenant.id);
        onSuccess('Workspace archived.');
        clearError('archive-tenant');
        refetchTenants();

        // Record undo action
        addAction({
          type: 'archive_tenant',
          description: `Archive workspace "${tenant.name}"`,
          previousState: tenant,
          reverse: async () => {
            onSuccess('Undo not available - restore requires API endpoint.');
          },
        });
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : 'Failed to archive workspace';
        onError('archive-tenant', errorMsg, () => handleArchive(tenant));
      }
    },
    [canManage, onSuccess, onError, clearError, refetchTenants, addAction]
  );

  const handlePause = useCallback(
    async (tenant: ApiTenant) => {
      if (!canManage) return;
      try {
        await apiClient.pauseTenant(tenant.id);
        onSuccess(`Workspace "${tenant.name}" paused.`);
        clearError('pause-tenant');
        refetchTenants();

        // Record undo action
        addAction({
          type: 'pause_tenant',
          description: `Pause workspace "${tenant.name}"`,
          previousState: tenant,
          reverse: async () => {
            onSuccess('Undo not available - resume requires API endpoint.');
          },
          forward: async () => {
            await apiClient.pauseTenant(tenant.id);
            refetchTenants();
          },
        });
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : 'Failed to pause workspace';
        onError('pause-tenant', errorMsg, () => handlePause(tenant));
      }
    },
    [canManage, onSuccess, onError, clearError, refetchTenants, addAction]
  );

  const handleAssignPolicies = useCallback(
    async (tenant: ApiTenant, policyIds: string[]) => {
      if (!canManage) return;
      try {
        await apiClient.assignTenantPolicies(tenant.id, policyIds);
        onSuccess(`Assigned ${policyIds.length} policies.`);
        clearError('assign-policies');
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : 'Failed to assign policies';
        onError('assign-policies', errorMsg, () => handleAssignPolicies(tenant, policyIds));
      }
    },
    [canManage, onSuccess, onError, clearError]
  );

  const handleAssignAdapters = useCallback(
    async (tenant: ApiTenant, adapterIds: string[]) => {
      if (!canManage) return;
      try {
        await apiClient.assignTenantAdapters(tenant.id, adapterIds);
        onSuccess(`Assigned ${adapterIds.length} adapters.`);
        clearError('assign-adapters');
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : 'Failed to assign adapters';
        onError('assign-adapters', errorMsg, () => handleAssignAdapters(tenant, adapterIds));
      }
    },
    [canManage, onSuccess, onError, clearError]
  );

  const handleViewUsage = useCallback(
    async (tenant: ApiTenant): Promise<TenantUsageResponse | null> => {
      try {
        const usage = await apiClient.getTenantUsage(tenant.id);
        clearError('view-usage');
        return usage;
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : 'Failed to fetch workspace usage';
        onError('view-usage', errorMsg, () => handleViewUsage(tenant));
        return null;
      }
    },
    [onError, clearError]
  );

  const handleCreate = useCallback(
    async (name: string) => {
      if (!canManage || !name.trim()) return;
      try {
        await apiClient.createTenant({ name, isolation_level: 'standard' });
        onSuccess('Workspace created.');
        clearError('create-tenant');
        refetchTenants();
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : 'Failed to create workspace';
        onError('create-tenant', errorMsg, () => handleCreate(name));
      }
    },
    [canManage, onSuccess, onError, clearError, refetchTenants]
  );

  return {
    handleEdit,
    handleArchive,
    handlePause,
    handleAssignPolicies,
    handleAssignAdapters,
    handleViewUsage,
    handleCreate,
  };
}
