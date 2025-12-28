import { useState, useCallback } from 'react';
import { apiClient } from '@/api/services';
import { Tenant as ApiTenant } from '@/api/types';
import { logger, toError } from '@/utils/logger';
import { BulkAction } from '@/components/ui/bulk-action-bar';
import { ConfirmationOptions } from '@/components/ui/confirmation-dialog';
import { ExportOptions } from '@/components/ui/export-dialog';
import * as types from '@/api/types';
import { UndoableAction } from '@/hooks/ui/useUndoRedo';

export type PartialUndoableAction<T = unknown> = Omit<UndoableAction<T>, 'id' | 'timestamp'>;

export interface BulkActionCallbacks {
  onSuccess: (message: string) => void;
  onError: (errorKey: string, message: string, retryFn?: () => void) => void;
  refetchTenants: () => void;
  addAction: <T = unknown>(action: PartialUndoableAction<T>) => void;
}

export interface UseTenantBulkActionsOptions {
  tenants: ApiTenant[];
  selectedTenants: string[];
  setSelectedTenants: (tenants: string[]) => void;
  callbacks: BulkActionCallbacks;
}

export interface UseTenantBulkActionsReturn {
  bulkActions: BulkAction[];
  confirmationOpen: boolean;
  setConfirmationOpen: (open: boolean) => void;
  confirmationOptions: ConfirmationOptions | null;
  pendingBulkAction: (() => Promise<void>) | null;
  handleConfirmBulkAction: () => Promise<void>;
  clearPendingAction: () => void;
  handleExport: (options: ExportOptions) => Promise<void>;
}

export function useTenantBulkActions(
  options: UseTenantBulkActionsOptions
): UseTenantBulkActionsReturn {
  const { tenants, selectedTenants, setSelectedTenants, callbacks } = options;
  const { onSuccess, onError, refetchTenants, addAction } = callbacks;

  const [confirmationOpen, setConfirmationOpen] = useState(false);
  const [confirmationOptions, setConfirmationOptions] = useState<ConfirmationOptions | null>(null);
  const [pendingBulkAction, setPendingBulkAction] = useState<(() => Promise<void>) | null>(null);

  const handleBulkPause = useCallback(
    async (tenantIds: string[]) => {
      const performBulkPause = async () => {
        const pausedTenants = tenants.filter((t) => tenantIds.includes(t.id));
        let successCount = 0;
        let errorCount = 0;

        for (const tenantId of tenantIds) {
          try {
            await apiClient.pauseTenant(tenantId);
            successCount++;
          } catch (err) {
            errorCount++;
            logger.error('Failed to pause tenant in bulk operation', {
              component: 'useTenantBulkActions',
              operation: 'bulkPause',
              tenantId,
            }, toError(err));
          }
        }

        if (successCount > 0) {
          onSuccess(`Successfully paused ${successCount} workspace(s).`);

          addAction({
            type: 'bulk_pause_tenants',
            description: `Pause ${successCount} workspace(s)`,
            previousState: pausedTenants.slice(0, successCount),
            reverse: async () => {
              onSuccess('Undo not available - resume requires API endpoint.');
            },
          });
        }
        if (errorCount > 0) {
          onError('bulk-pause', `Failed to pause ${errorCount} workspace(s).`, performBulkPause);
        }

        await refetchTenants();
        setSelectedTenants([]);
      };

      setConfirmationOptions({
        title: 'Pause Workspaces',
        description: `Pause ${tenantIds.length} workspace(s)? This will stop new sessions for these workspaces.`,
        confirmText: 'Pause Workspaces',
        variant: 'default',
      });
      setPendingBulkAction(() => performBulkPause);
      setConfirmationOpen(true);
    },
    [tenants, onSuccess, onError, refetchTenants, setSelectedTenants, addAction]
  );

  const handleBulkArchive = useCallback(
    async (tenantIds: string[]) => {
      const performBulkArchive = async () => {
        const archivedTenants = tenants.filter((t) => tenantIds.includes(t.id));
        let successCount = 0;
        let errorCount = 0;

        for (const tenantId of tenantIds) {
          try {
            await apiClient.archiveTenant(tenantId);
            successCount++;
          } catch (err) {
            errorCount++;
            logger.error('Failed to archive tenant in bulk operation', {
              component: 'useTenantBulkActions',
              operation: 'bulkArchive',
              tenantId,
            }, toError(err));
          }
        }

        if (successCount > 0) {
          onSuccess(`Successfully archived ${successCount} workspace(s).`);

          addAction({
            type: 'bulk_archive_tenants',
            description: `Archive ${successCount} workspace(s)`,
            previousState: archivedTenants.slice(0, successCount),
            reverse: async () => {
              onSuccess('Undo not available - restore requires API endpoint.');
            },
          });
        }
        if (errorCount > 0) {
          onError('bulk-archive', `Failed to archive ${errorCount} workspace(s).`, performBulkArchive);
        }

        await refetchTenants();
        setSelectedTenants([]);
      };

      setConfirmationOptions({
        title: 'Archive Workspaces',
        description: `Permanently archive ${tenantIds.length} workspace(s)? All associated resources will be suspended. This action can be reversed by an administrator.`,
        confirmText: 'Archive Workspaces',
        variant: 'destructive',
      });
      setPendingBulkAction(() => performBulkArchive);
      setConfirmationOpen(true);
    },
    [tenants, onSuccess, onError, refetchTenants, setSelectedTenants, addAction]
  );

  const handleConfirmBulkAction = useCallback(async () => {
    if (pendingBulkAction) {
      await pendingBulkAction();
      setPendingBulkAction(null);
      setConfirmationOptions(null);
    }
  }, [pendingBulkAction]);

  const clearPendingAction = useCallback(() => {
    setPendingBulkAction(null);
    setConfirmationOptions(null);
  }, []);

  const handleExport = useCallback(
    async (exportOptions: ExportOptions) => {
      try {
        let tenantsToExport: ApiTenant[] = [];

        if (exportOptions.scope === 'selected') {
          tenantsToExport = tenants.filter((t) => selectedTenants.includes(t.id));
        } else if (exportOptions.scope === 'all') {
          tenantsToExport = tenants;
        } else {
          // filtered - for now, same as all
          tenantsToExport = tenants;
        }

        if (tenantsToExport.length === 0) {
          onSuccess('No workspaces to export.');
          return;
        }

        // Create export file
        const timestamp = new Date().toISOString().slice(0, 19).replace(/:/g, '-');
        const filename = `workspaces-export-${timestamp}`;

        if (exportOptions.format === 'json') {
          const blob = new Blob([JSON.stringify(tenantsToExport, null, 2)], {
            type: 'application/json',
          });
          const url = URL.createObjectURL(blob);
          const a = document.createElement('a');
          a.href = url;
          a.download = `${filename}.json`;
          a.click();
          URL.revokeObjectURL(url);
        } else {
          // CSV export
          const headers: (keyof types.Tenant)[] = [
            'id',
            'name',
            'description',
            'status',
            'data_classification',
            'itar_compliant',
            'users',
            'adapters',
            'policies',
            'last_activity',
            'created_at',
          ];
          const csvRows = tenantsToExport.map((t) =>
            headers
              .map((header) => {
                const value = t[header] ?? '';
                const stringValue = String(value);
                if (stringValue.includes(',') || stringValue.includes('"')) {
                  return `"${stringValue.replace(/"/g, '""')}"`;
                }
                return stringValue;
              })
              .join(',')
          );
          const csv = [headers.join(','), ...csvRows].join('\n');
          const blob = new Blob([csv], { type: 'text/csv' });
          const url = URL.createObjectURL(blob);
          const a = document.createElement('a');
          a.href = url;
          a.download = `${filename}.csv`;
          a.click();
          URL.revokeObjectURL(url);
        }

        onSuccess(`Exported ${tenantsToExport.length} workspace(s).`);
      } catch (err) {
        const error = err instanceof Error ? err : new Error('Failed to export workspaces');
        onError('export-tenants', error.message, () => handleExport(exportOptions));
      }
    },
    [tenants, selectedTenants, onSuccess, onError]
  );

  const bulkActions: BulkAction[] = [
    {
      id: 'pause',
      label: 'Pause',
      handler: handleBulkPause,
    },
    {
      id: 'archive',
      label: 'Archive',
      variant: 'destructive',
      handler: handleBulkArchive,
    },
  ];

  return {
    bulkActions,
    confirmationOpen,
    setConfirmationOpen,
    confirmationOptions,
    pendingBulkAction,
    handleConfirmBulkAction,
    clearPendingAction,
    handleExport,
  };
}
