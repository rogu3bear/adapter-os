import React, { useState, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { BulkActionBar } from '@/components/ui/bulk-action-bar';
import { ConfirmationDialog } from '@/components/ui/confirmation-dialog';
import { ExportDialog } from '@/components/ui/export-dialog';
import { useUndoRedoContext } from '@/contexts/UndoRedoContext';
import { createDialogManager } from '@/hooks/ui/useDialogManager';
import { TenantImportWizard } from '@/components/TenantImportWizard';
import { useRBAC } from '@/hooks/security/useRBAC';
import { PageErrorsProvider, PageErrors, usePageErrors } from '@/components/ui/page-error-boundary';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { Shield, CheckCircle, AlertTriangle } from 'lucide-react';
import { User, Tenant as ApiTenant, TenantUsageResponse } from '@/api/types';
import { toast } from 'sonner';

// Hooks
import {
  useTenantsData,
  useTenantOperations,
  useTenantBulkActions,
} from '@/hooks/tenants';

// Components
import { TenantsHeader } from './TenantsHeader';
import { TenantsKpiCards } from './TenantsKpiCards';
import { TenantsTable } from './TenantsTable';
import { CreateTenantDialog, NewTenantData } from './CreateTenantDialog';
import { EditTenantDialog } from './EditTenantDialog';
import { TenantUsageDialog } from './TenantUsageDialog';
import { AssignPoliciesDialog } from './AssignPoliciesDialog';
import { AssignAdaptersDialog } from './AssignAdaptersDialog';
import { ArchiveTenantDialog } from './ArchiveTenantDialog';

// Dialog manager for tenant dialogs
const useTenantDialogs = createDialogManager<
  'edit' | 'assignPolicies' | 'assignAdapters' | 'usage' | 'archive' | 'export' | 'import',
  {
    edit: undefined;
    assignPolicies: undefined;
    assignAdapters: undefined;
    usage: undefined;
    archive: undefined;
    export: undefined;
    import: undefined;
  }
>(['edit', 'assignPolicies', 'assignAdapters', 'usage', 'archive', 'export', 'import'] as const);

export interface TenantsProps {
  user: User;
  selectedTenant: string;
}

function TenantsContent({ user }: TenantsProps) {
  const { addAction } = useUndoRedoContext();
  const { can } = useRBAC();
  const dialogs = useTenantDialogs();
  const { errors, addError, clearError } = usePageErrors();

  // Local state
  const [selectedTenantForAction, setSelectedTenantForAction] = useState<ApiTenant | null>(null);
  const [editName, setEditName] = useState('');
  const [usageData, setUsageData] = useState<TenantUsageResponse | null>(null);
  const [selectedPolicies, setSelectedPolicies] = useState<string[]>([]);
  const [selectedAdapters, setSelectedAdapters] = useState<string[]>([]);
  const [selectedTenants, setSelectedTenants] = useState<string[]>([]);
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [statusMessage, setStatusMessage] = useState<{
    message: string;
    variant: 'success' | 'info' | 'warning';
  } | null>(null);

  const canManage = can('tenant:manage');
  const userId = user.id;

  // Data fetching hook
  const { tenants, policies, adapters, isLoading, refetch: refetchTenants } = useTenantsData({
    userId,
    canManage,
  });

  // Status message helper
  const showStatus = useCallback(
    (message: string, variant: 'success' | 'info' | 'warning' = 'success') => {
      setStatusMessage({ message, variant });
    },
    []
  );

  // Operation callbacks
  const operationCallbacks = {
    onSuccess: showStatus,
    onError: (errorKey: string, message: string, retryFn?: () => void) => {
      addError(errorKey, message, retryFn);
      toast.error(message);
    },
    clearError,
    refetchTenants,
    addAction,
  };

  // CRUD operations hook
  const operations = useTenantOperations({
    callbacks: operationCallbacks,
    canManage,
  });

  // Bulk actions hook
  const bulkActions = useTenantBulkActions({
    tenants,
    selectedTenants,
    setSelectedTenants,
    callbacks: operationCallbacks,
  });

  // Handler functions
  const handleCreateTenant = async (data: NewTenantData) => {
    await operations.handleCreate(data.name);
    setIsCreateDialogOpen(false);
  };

  const handleEditSubmit = async () => {
    if (!selectedTenantForAction) return;
    await operations.handleEdit(selectedTenantForAction, editName);
    dialogs.closeDialog('edit');
    setSelectedTenantForAction(null);
  };

  const handleArchiveSubmit = async () => {
    if (!selectedTenantForAction) return;
    await operations.handleArchive(selectedTenantForAction);
    dialogs.closeDialog('archive');
    setSelectedTenantForAction(null);
  };

  const handleAssignPoliciesSubmit = async () => {
    if (!selectedTenantForAction) return;
    await operations.handleAssignPolicies(selectedTenantForAction, selectedPolicies);
    dialogs.closeDialog('assignPolicies');
    setSelectedPolicies([]);
  };

  const handleAssignAdaptersSubmit = async () => {
    if (!selectedTenantForAction) return;
    await operations.handleAssignAdapters(selectedTenantForAction, selectedAdapters);
    dialogs.closeDialog('assignAdapters');
    setSelectedAdapters([]);
  };

  const handleViewUsage = async (tenant: ApiTenant) => {
    const usage = await operations.handleViewUsage(tenant);
    if (usage) {
      setUsageData(usage);
      setSelectedTenantForAction(tenant);
      dialogs.openDialog('usage');
    }
  };

  // Access restricted view
  if (!canManage) {
    return (
      <div className="flex items-center justify-center h-96">
        <div className="text-center space-y-2">
          <Shield className="h-12 w-12 text-muted-foreground mx-auto" />
          <h3>Access Restricted</h3>
          <p className="text-muted-foreground">
            Workspace management requires Administrator privileges.
          </p>
          <GlossaryTooltip termId="requires-admin">
            <Button variant="ghost" size="sm" className="text-muted-foreground">
              Why am I seeing this?
            </Button>
          </GlossaryTooltip>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Consolidated Error Display */}
      <PageErrors errors={errors} />

      {/* Status Message */}
      {statusMessage && (
        <Alert
          className={
            statusMessage.variant === 'success'
              ? 'border-green-200 bg-green-50'
              : statusMessage.variant === 'warning'
                ? 'border-amber-200 bg-amber-50'
                : 'border-blue-200 bg-blue-50'
          }
        >
          {statusMessage.variant === 'success' ? (
            <CheckCircle className="h-4 w-4 text-green-600" />
          ) : statusMessage.variant === 'warning' ? (
            <AlertTriangle className="h-4 w-4 text-amber-600" />
          ) : (
            <AlertTriangle className="h-4 w-4 text-blue-600" />
          )}
          <AlertDescription
            className={
              statusMessage.variant === 'success'
                ? 'text-green-700'
                : statusMessage.variant === 'warning'
                  ? 'text-amber-700'
                  : 'text-blue-700'
            }
          >
            {statusMessage.message}
          </AlertDescription>
        </Alert>
      )}

      {/* Header */}
      <TenantsHeader canManage={canManage} onCreateClick={() => setIsCreateDialogOpen(true)} />

      {/* KPI Cards */}
      <TenantsKpiCards tenants={tenants} />

      {/* Workspaces Table */}
      <TenantsTable
        tenants={tenants}
        selectedTenants={selectedTenants}
        onSelectedTenantsChange={setSelectedTenants}
        onPause={(tenant) => operations.handlePause(tenant)}
        onEdit={(tenant) => {
          setSelectedTenantForAction(tenant);
          setEditName(tenant.name);
          dialogs.openDialog('edit');
        }}
        onAssignPolicies={(tenant) => {
          setSelectedTenantForAction(tenant);
          dialogs.openDialog('assignPolicies');
        }}
        onAssignAdapters={(tenant) => {
          setSelectedTenantForAction(tenant);
          dialogs.openDialog('assignAdapters');
        }}
        onViewUsage={handleViewUsage}
        onArchive={(tenant) => {
          setSelectedTenantForAction(tenant);
          dialogs.openDialog('archive');
        }}
        onImport={() => dialogs.openDialog('import')}
        onExport={() => dialogs.openDialog('export')}
        canManage={canManage}
      />

      {/* Create Tenant Dialog */}
      <CreateTenantDialog
        open={isCreateDialogOpen}
        onOpenChange={setIsCreateDialogOpen}
        onSubmit={handleCreateTenant}
        canManage={canManage}
      />

      {/* Edit Tenant Dialog */}
      <EditTenantDialog
        open={dialogs.isOpen('edit')}
        onOpenChange={(open) => !open && dialogs.closeDialog('edit')}
        tenant={selectedTenantForAction}
        editName={editName}
        onEditNameChange={setEditName}
        onSubmit={handleEditSubmit}
        canManage={canManage}
      />

      {/* Assign Policies Dialog */}
      <AssignPoliciesDialog
        open={dialogs.isOpen('assignPolicies')}
        onOpenChange={(open) => !open && dialogs.closeDialog('assignPolicies')}
        tenant={selectedTenantForAction}
        policies={policies}
        selectedPolicies={selectedPolicies}
        onSelectedPoliciesChange={setSelectedPolicies}
        onSubmit={handleAssignPoliciesSubmit}
        canManage={canManage}
      />

      {/* Assign Adapters Dialog */}
      <AssignAdaptersDialog
        open={dialogs.isOpen('assignAdapters')}
        onOpenChange={(open) => !open && dialogs.closeDialog('assignAdapters')}
        tenant={selectedTenantForAction}
        adapters={adapters}
        selectedAdapters={selectedAdapters}
        onSelectedAdaptersChange={setSelectedAdapters}
        onSubmit={handleAssignAdaptersSubmit}
        canManage={canManage}
      />

      {/* View Usage Dialog */}
      <TenantUsageDialog
        open={dialogs.isOpen('usage')}
        onOpenChange={(open) => !open && dialogs.closeDialog('usage')}
        tenant={selectedTenantForAction}
        usageData={usageData}
      />

      {/* Archive Tenant Dialog */}
      <ArchiveTenantDialog
        open={dialogs.isOpen('archive')}
        onOpenChange={(open) => {
          if (!open) {
            dialogs.closeDialog('archive');
            setSelectedTenantForAction(null);
          }
        }}
        tenant={selectedTenantForAction}
        onSubmit={handleArchiveSubmit}
        canManage={canManage}
      />

      {/* Bulk Action Bar */}
      <BulkActionBar
        selectedItems={selectedTenants}
        actions={bulkActions.bulkActions}
        onClearSelection={() => setSelectedTenants([])}
        itemName="workspace"
      />

      {/* Confirmation Dialog */}
      <ConfirmationDialog
        open={bulkActions.confirmationOpen}
        onOpenChange={(open) => {
          bulkActions.setConfirmationOpen(open);
          if (!open) {
            bulkActions.clearPendingAction();
          }
        }}
        onConfirm={bulkActions.handleConfirmBulkAction}
        options={
          bulkActions.confirmationOptions || {
            title: 'Confirm Action',
            description: 'Are you sure?',
            variant: 'default',
          }
        }
      />

      {/* Export Dialog */}
      <ExportDialog
        open={dialogs.isOpen('export')}
        onOpenChange={(open) => !open && dialogs.closeDialog('export')}
        onExport={async (options) => {
          await bulkActions.handleExport(options);
          dialogs.closeDialog('export');
        }}
        itemName="workspaces"
        hasSelected={selectedTenants.length > 0}
        hasFilters={false}
        defaultFormat="json"
        defaultScope={selectedTenants.length > 0 ? 'selected' : 'all'}
      />

      {/* Import Dialog */}
      <Dialog
        open={dialogs.isOpen('import')}
        onOpenChange={(open) => !open && dialogs.closeDialog('import')}
      >
        <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Import Workspace</DialogTitle>
          </DialogHeader>
          <TenantImportWizard
            onComplete={(tenant) => {
              dialogs.closeDialog('import');
              refetchTenants();
              showStatus(`Workspace "${tenant.name}" created successfully.`, 'success');
            }}
            onCancel={() => dialogs.closeDialog('import')}
          />
        </DialogContent>
      </Dialog>
    </div>
  );
}

// Wrap with PageErrorsProvider
export function Tenants(props: TenantsProps) {
  return (
    <PageErrorsProvider>
      <TenantsContent {...props} />
    </PageErrorsProvider>
  );
}
