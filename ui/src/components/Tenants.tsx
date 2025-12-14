import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Textarea } from './ui/textarea';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { VirtualizedTableRows } from './ui/virtualized-table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger, DialogFooter } from './ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Switch } from './ui/switch';
import { Alert, AlertDescription } from './ui/alert';
import { Progress } from './ui/progress';
import { Checkbox } from './ui/checkbox';
import { BulkActionBar, BulkAction } from './ui/bulk-action-bar';
import { ConfirmationDialog, ConfirmationOptions } from './ui/confirmation-dialog';
import { ExportDialog, ExportOptions } from './ui/export-dialog';
import { useUndoRedoContext } from '@/contexts/UndoRedoContext';
import { useModalManager } from '@/contexts/ModalContext';
import { TenantImportWizard } from './TenantImportWizard';
import { useRBAC } from '@/hooks/security/useRBAC';
import { PageErrorsProvider, PageErrors, usePageErrors } from '@/components/ui/page-error-boundary';
import { KpiGrid } from './ui/grid';

// Modal ID constants
const MODAL_IDS = {
  EDIT: 'edit-tenant',
  ASSIGN_POLICIES: 'assign-policies',
  ASSIGN_ADAPTERS: 'assign-adapters',
  USAGE: 'view-usage',
  ARCHIVE: 'archive-tenant',
  EXPORT: 'export-tenants',
  IMPORT: 'import-tenants',
} as const;
import { usePolling } from '@/hooks/realtime/usePolling';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { GlossaryTooltip } from './ui/glossary-tooltip';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';
import {
  Plus,
  Users,
  Shield,
  AlertTriangle,
  CheckCircle,
  Settings,
  Lock,
  Eye,
  UserCheck,
  Database,
  Network,
  MoreHorizontal,
  Edit,
  Archive,
  Layers,
  BarChart3,
  Building2,
  CreditCard,
  Calendar,
  AlertCircle,
  Download,
  Upload
} from 'lucide-react';
import apiClient from '@/api/client';
import * as types from '@/api/types';
import { Tenant as ApiTenant, User, Policy, Adapter, TenantUsageResponse } from '@/api/types';

import { logger, toError } from '@/utils/logger';
import { formatPercent, formatCount } from '@/utils';
import { BookmarkButton } from './ui/bookmark-button';

import { toast } from 'sonner';

interface TenantsProps {
  user: User;
  selectedTenant: string;
}

function TenantsContent({ user, selectedTenant }: TenantsProps) {
  const { addAction } = useUndoRedoContext();
  const { can, userRole } = useRBAC();
  const { openModal, closeModal, isOpen } = useModalManager();
  const { errors, addError, clearError } = usePageErrors();
  const [tenants, setTenants] = useState<ApiTenant[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedTenantForAction, setSelectedTenantForAction] = useState<ApiTenant | null>(null);
  const [editName, setEditName] = useState('');
  const [usageData, setUsageData] = useState<TenantUsageResponse | null>(null);
  const [selectedPolicies, setSelectedPolicies] = useState<string[]>([]);
  const [selectedAdapters, setSelectedAdapters] = useState<string[]>([]);
  const [selectedTenants, setSelectedTenants] = useState<string[]>([]);
  const [policies, setPolicies] = useState<Policy[]>([]);
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'info' | 'warning' } | null>(null);
  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);
  const [confirmationOpen, setConfirmationOpen] = useState(false);
  const [confirmationOptions, setConfirmationOptions] = useState<ConfirmationOptions | null>(null);
  const [pendingBulkAction, setPendingBulkAction] = useState<(() => Promise<void>) | null>(null);
  const userId = user.id;

  const showStatus = (message: string, variant: 'success' | 'info' | 'warning') => {
    setStatusMessage({ message, variant });
  };

  // Use polling for tenant list updates
  const { data: polledTenants, isLoading: pollingLoading, error: pollingError, refetch: refetchTenants } = usePolling<ApiTenant[]>(
    () => apiClient.listTenants(),
    'slow',
    {
      enabled: can('tenant:manage'),
      operationName: 'fetchTenants',
      onSuccess: (data) => {
        setTenants(data as ApiTenant[]);
        setStatusMessage(null);
        setErrorRecovery(null);
      },
      onError: (err) => {
        logger.error('Failed to fetch tenants', {
          component: 'Tenants',
          operation: 'fetchTenants',
          userId
        }, err);
        addError('fetch-tenants', err.message || 'Failed to load tenants.', () => {
          clearError('fetch-tenants');
          refetchTenants();
        });
        toast.error('Failed to load organizations');
      }
    }
  );

  // Update tenants when polling data changes
  useEffect(() => {
    if (polledTenants) {
      setTenants(polledTenants);
      setLoading(false);
    }
  }, [polledTenants]);

  // Update loading state
  useEffect(() => {
    setLoading(pollingLoading);
  }, [pollingLoading]);

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
          component: 'Tenants',
          operation: 'fetchPoliciesAdapters',
          userId: user.id
        }, err instanceof Error ? err : new Error(String(err)));
      }
    };
    fetchData();
  }, [userId, user.id]);

  const handleEdit = async () => {
    if (!selectedTenantForAction) return;
    try {
      await apiClient.updateTenant(selectedTenantForAction.id, editName);
      showStatus('Organization updated.', 'success');
      clearError('edit-tenant');
      closeModal();
      refetchTenants();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to update tenant';
      addError('edit-tenant', errorMsg, handleEdit);
    }
  };


  const handleArchive = async () => {
    if (!selectedTenantForAction) return;
    try {
      const tenant = selectedTenantForAction;
      await apiClient.archiveTenant(tenant.id);
      showStatus('Organization archived.', 'success');
      clearError('archive-tenant');
      closeModal();
      setSelectedTenantForAction(null);
      await refetchTenants();

      // Record undo action
      addAction({
        type: 'archive_tenant',
        description: `Archive tenant "${tenant.name}"`,
        previousState: tenant,
        reverse: async () => {
          // Would need restore/unarchive endpoint - for now, show warning
          showStatus('Undo not available - restore requires API endpoint.', 'warning');
        },
      });
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to archive tenant';
      addError('archive-tenant', errorMsg, handleArchive);
    }
  };

  const handleAssignPolicies = async () => {
    if (!selectedTenantForAction) return;
    try {
      await apiClient.assignTenantPolicies(selectedTenantForAction.id, selectedPolicies);
      showStatus(`Assigned ${selectedPolicies.length} policies.`, 'success');
      clearError('assign-policies');
      closeModal();
      setSelectedPolicies([]);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to assign policies';
      addError('assign-policies', errorMsg, handleAssignPolicies);
    }
  };

  const handleAssignAdapters = async () => {
    if (!selectedTenantForAction) return;
    try {
      await apiClient.assignTenantAdapters(selectedTenantForAction.id, selectedAdapters);
      showStatus(`Assigned ${selectedAdapters.length} adapters.`, 'success');
      clearError('assign-adapters');
      closeModal();
      setSelectedAdapters([]);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to assign adapters';
      addError('assign-adapters', errorMsg, handleAssignAdapters);
    }
  };

  const handleViewUsage = async (tenant: ApiTenant) => {
    try {
      const usage = await apiClient.getTenantUsage(tenant.id);
      setUsageData(usage);
      setSelectedTenantForAction(tenant);
      clearError('view-usage');
      openModal(MODAL_IDS.USAGE);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to fetch tenant usage';
      addError('view-usage', errorMsg, () => handleViewUsage(tenant));
    }
  };

  const handlePause = async (tenant: ApiTenant) => {
    try {
      const previousStatus = tenant.status;
      await apiClient.pauseTenant(tenant.id);
      showStatus(`Organization "${tenant.name}" paused.`, 'success');
      clearError('pause-tenant');
      await refetchTenants();

      // Record undo action
      addAction({
        type: 'pause_tenant',
        description: `Pause tenant "${tenant.name}"`,
        previousState: tenant,
        reverse: async () => {
          // Resume tenant - would need an API endpoint to resume
          showStatus('Undo not available - resume requires API endpoint.', 'warning');
        },
        forward: async () => {
          await apiClient.pauseTenant(tenant.id);
          await refetchTenants();
        },
      });
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to pause tenant';
      addError('pause-tenant', errorMsg, () => handlePause(tenant));
    }
  };

  // Bulk action handlers
  const handleBulkPause = async (tenantIds: string[]) => {
    const performBulkPause = async () => {
      const pausedTenants = tenants.filter(t => tenantIds.includes(t.id));
      let successCount = 0;
      let errorCount = 0;

      for (const tenantId of tenantIds) {
        try {
          await apiClient.pauseTenant(tenantId);
          successCount++;
        } catch (err) {
          errorCount++;
          logger.error('Failed to pause tenant in bulk operation', {
            component: 'Tenants',
            operation: 'bulkPause',
            tenantId
          }, toError(err));
        }
      }

      if (successCount > 0) {
        showStatus(`Successfully paused ${successCount} organization(s).`, 'success');

        // Record undo action
        addAction({
          type: 'bulk_pause_tenants',
          description: `Pause ${successCount} tenant(s)`,
          previousState: pausedTenants.slice(0, successCount),
          reverse: async () => {
            showStatus('Undo not available - resume requires API endpoint.', 'warning');
          }
        });
      }
      if (errorCount > 0) {
        addError('bulk-pause', `Failed to pause ${errorCount} organization(s).`, performBulkPause);
      }

      await refetchTenants();
      setSelectedTenants([]);
    };

    setConfirmationOptions({
      title: 'Pause Organizations',
      description: `Pause ${tenantIds.length} organization(s)? This will stop new sessions for these organizations.`,
      confirmText: 'Pause Organizations',
      variant: 'default'
    });
    setPendingBulkAction(() => performBulkPause);
    setConfirmationOpen(true);
  };

  const handleBulkArchive = async (tenantIds: string[]) => {
    const performBulkArchive = async () => {
      const archivedTenants = tenants.filter(t => tenantIds.includes(t.id));
      let successCount = 0;
      let errorCount = 0;

      for (const tenantId of tenantIds) {
        try {
          await apiClient.archiveTenant(tenantId);
          successCount++;
        } catch (err) {
          errorCount++;
          logger.error('Failed to archive tenant in bulk operation', {
            component: 'Tenants',
            operation: 'bulkArchive',
            tenantId
          }, toError(err));
        }
      }

      if (successCount > 0) {
        showStatus(`Successfully archived ${successCount} organization(s).`, 'success');

        // Record undo action
        addAction({
          type: 'bulk_archive_tenants',
          description: `Archive ${successCount} tenant(s)`,
          previousState: archivedTenants.slice(0, successCount),
          reverse: async () => {
            showStatus('Undo not available - restore requires API endpoint.', 'warning');
          }
        });
      }
      if (errorCount > 0) {
        addError('bulk-archive', `Failed to archive ${errorCount} organization(s).`, performBulkArchive);
      }

      await refetchTenants();
      setSelectedTenants([]);
    };

    setConfirmationOptions({
      title: 'Archive Organizations',
      description: `Permanently archive ${tenantIds.length} organization(s)? All associated resources will be suspended. This action can be reversed by an administrator.`,
      confirmText: 'Archive Organizations',
      variant: 'destructive'
    });
    setPendingBulkAction(() => performBulkArchive);
    setConfirmationOpen(true);
  };

  const bulkActions: BulkAction[] = [
    {
      id: 'pause',
      label: 'Pause',
      handler: handleBulkPause
    },
    {
      id: 'archive',
      label: 'Archive',
      variant: 'destructive',
      handler: handleBulkArchive
    }
  ];

  const handleExport = async (options: ExportOptions) => {
    try {
      let tenantsToExport: ApiTenant[] = [];

      if (options.scope === 'selected') {
        tenantsToExport = tenants.filter(t => selectedTenants.includes(t.id));
      } else if (options.scope === 'all') {
        tenantsToExport = tenants;
      } else {
        // filtered - for now, same as all
        tenantsToExport = tenants;
      }

      if (tenantsToExport.length === 0) {
        showStatus('No organizations to export.', 'warning');
        closeModal();
        return;
      }

      // Create export file
      const timestamp = new Date().toISOString().slice(0, 19).replace(/:/g, '-');
      const filename = `tenants-export-${timestamp}`;

      if (options.format === 'json') {
        const blob = new Blob([JSON.stringify(tenantsToExport, null, 2)], { type: 'application/json' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `${filename}.json`;
        a.click();
        URL.revokeObjectURL(url);
      } else {
        // CSV export
        const headers: (keyof types.Tenant)[] = ['id', 'name', 'description', 'status', 'data_classification', 'itar_compliant', 'users', 'adapters', 'policies', 'last_activity', 'created_at'];
        const csvRows = tenantsToExport.map(t =>
          headers.map(header => {
            const value = t[header] ?? '';
            const stringValue = String(value);
            if (stringValue.includes(',') || stringValue.includes('"')) {
              return `"${stringValue.replace(/"/g, '""')}"`;
            }
            return stringValue;
          }).join(',')
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

      showStatus(`Exported ${tenantsToExport.length} organization(s).`, 'success');
      closeModal();
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to export tenants');
      addError('export-tenants', error.message, () => handleExport(options));
    }
  };

  // Mock data removed - using real API data from state

  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [newTenant, setNewTenant] = useState({
    name: '',
    description: '',
    dataClassification: 'internal' as const,
    itarCompliant: false
  });

  const getStatusBadge = (status?: ApiTenant['status']) => {
    const currentStatus = status || 'active';
    switch (currentStatus) {
      case 'active':
        return <div className="status-indicator status-success"><CheckCircle className="h-3 w-3" />Active</div>;
      case 'suspended':
        return <div className="status-indicator status-error"><AlertTriangle className="h-3 w-3" />Suspended</div>;
      case 'maintenance':
        return <div className="status-indicator status-warning"><Settings className="h-3 w-3" />Maintenance</div>;
      case 'paused':
        return <div className="status-indicator status-neutral"><Lock className="h-3 w-3" />Inactive</div>;
      case 'archived':
        return <div className="status-indicator status-neutral"><Database className="h-3 w-3" />Archived</div>;
      default:
        return <div className="status-indicator status-neutral">Unknown</div>;
    }
  };

  const getClassificationBadge = (classification?: ApiTenant['data_classification']) => {
    const current = classification || 'internal';
    const colors: Record<string, string> = {
      public: 'status-info',
      internal: 'status-neutral',
      confidential: 'status-warning',
      restricted: 'status-error'
    };

    return (
      <div className={`status-indicator ${colors[current] || 'status-neutral'}`}>
        <Lock className="h-3 w-3" />
        {current.toUpperCase()}
      </div>
    );
  };

  const handleCreateTenant = async () => {
    if (!newTenant.name.trim()) return;
    try {
      await apiClient.createTenant({ name: newTenant.name, isolation_level: 'standard' });
      showStatus('Organization created.', 'success');
      clearError('create-tenant');
      setNewTenant({ name: '', description: '', dataClassification: 'internal', itarCompliant: false });
      setIsCreateDialogOpen(false);
      await refetchTenants();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to create tenant';
      addError('create-tenant', errorMsg, handleCreateTenant);
    }
  };

  if (!can('tenant:manage')) {
    return (
      <div className="flex items-center justify-center h-96">
        <div className="text-center space-y-2">
          <Shield className="h-12 w-12 text-muted-foreground mx-auto" />
          <h3>Access Restricted</h3>
          <p className="text-muted-foreground">
            Organization management requires Administrator privileges.
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
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold">Organization Management</h1>
          <p className="text-sm text-muted-foreground">
            Manage organization isolation, data classification, and access controls
          </p>
        </div>
        <Dialog open={isCreateDialogOpen} onOpenChange={setIsCreateDialogOpen}>
          <DialogTrigger asChild>
            <GlossaryTooltip termId="create-tenant-button">
              <Button disabled={!can('tenant:manage')}>
                <Plus className="h-4 w-4 mr-2" />
                Create Organization
              </Button>
            </GlossaryTooltip>
          </DialogTrigger>
          <DialogContent className="max-w-md">
            <DialogHeader>
              <DialogTitle>Create New Organization</DialogTitle>
            </DialogHeader>
            <div className="mb-4">
              <div className="mb-4">
                <div className="flex items-center gap-1 mb-1">
                  <Label htmlFor="name" className="font-medium text-sm">Organization Name</Label>
                  <GlossaryTooltip termId="tenant-name">
                    <span className="cursor-help text-muted-foreground">(?)</span>
                  </GlossaryTooltip>
                </div>
                <Input
                  id="name"
                  placeholder="Enter organization name"
                  value={newTenant.name}
                  onChange={(e) => setNewTenant({ ...newTenant, name: e.target.value })}
                />
              </div>

              <div className="mb-4">
                <div className="flex items-center gap-1 mb-1">
                  <Label htmlFor="description" className="font-medium text-sm">Description</Label>
                  <GlossaryTooltip termId="tenant-description">
                    <span className="cursor-help text-muted-foreground">(?)</span>
                  </GlossaryTooltip>
                </div>
                <Textarea
                  id="description"
                  placeholder="Describe the organization's purpose"
                  value={newTenant.description}
                  onChange={(e) => setNewTenant({ ...newTenant, description: e.target.value })}
                />
              </div>

              <div className="mb-4">
                <div className="flex items-center gap-1 mb-1">
                  <Label htmlFor="classification" className="font-medium text-sm">Data Classification</Label>
                  <GlossaryTooltip termId="data-classification">
                    <span className="cursor-help text-muted-foreground">(?)</span>
                  </GlossaryTooltip>
                </div>
                <Select
                  value={newTenant.dataClassification}
                  onValueChange={(value: string) => setNewTenant({ ...newTenant, dataClassification: value as 'internal' })}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="public">Public</SelectItem>
                    <SelectItem value="internal">Internal</SelectItem>
                    <SelectItem value="confidential">Confidential</SelectItem>
                    <SelectItem value="restricted">Restricted</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div className="flex items-center justify-between">
                <div className="flex items-center gap-1">
                  <Label htmlFor="itar" className="font-medium text-sm">ITAR Compliance Required</Label>
                  <GlossaryTooltip termId="itar-compliance">
                    <span className="cursor-help text-muted-foreground">(?)</span>
                  </GlossaryTooltip>
                </div>
                <Switch
                  id="itar"
                  checked={newTenant.itarCompliant}
                  onCheckedChange={(checked) => setNewTenant({ ...newTenant, itarCompliant: checked })}
                />
              </div>

              <div className="flex items-center justify-end mt-4">
                <Button variant="outline" onClick={() => setIsCreateDialogOpen(false)}>
                  Cancel
                </Button>
                <GlossaryTooltip termId="create-tenant-action">
                  <Button onClick={handleCreateTenant} disabled={!newTenant.name.trim() || !can('tenant:manage')}>
                    Create Organization
                  </Button>
                </GlossaryTooltip>
              </div>
            </div>
          </DialogContent>
        </Dialog>
      </div>

      {/* Tenant Statistics */}
      <KpiGrid>
        <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
          <CardContent className="pt-6">
            <div className="flex items-center justify-center">
              <Users className="h-4 w-4 text-blue-600" />
              <div>
                <p className="text-2xl font-bold">{tenants.length}</p>
                <p className="text-xs text-muted-foreground">Total Organizations</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
          <CardContent className="pt-6">
            <div className="flex items-center justify-center">
              <CheckCircle className="h-4 w-4 text-green-600" />
              <div>
                <p className="text-2xl font-bold">{tenants.filter(t => t.status === 'active').length}</p>
                <p className="text-xs text-muted-foreground">Active</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
          <CardContent className="pt-6">
            <div className="flex items-center justify-center">
              <Shield className="h-4 w-4 text-orange-600" />
              <div>
                <p className="text-2xl font-bold">{tenants.filter(t => t.itarCompliant).length}</p>
                <p className="text-xs text-muted-foreground">ITAR Compliant</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
          <CardContent className="pt-6">
            <div className="flex items-center justify-center">
              <Database className="h-4 w-4 text-purple-600" />
              <div>
                <p className="text-2xl font-bold">{tenants.reduce((sum, t) => sum + (Number(t.adapters) || 0), 0)}</p>
                <p className="text-xs text-muted-foreground">Total Adapters</p>
              </div>
            </div>
          </CardContent>
        </Card>
      </KpiGrid>

      {/* Tenants Table */}
      <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle className="flex items-center gap-2">
              Active Organizations
              <GlossaryTooltip termId="tenant" variant="icon" />
              <GlossaryTooltip termId="active-tenants">
                <span className="cursor-help text-muted-foreground text-sm">(Help)</span>
              </GlossaryTooltip>
            </CardTitle>
            <div className="flex gap-2">
              <GlossaryTooltip termId="import-tenants">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => openModal(MODAL_IDS.IMPORT)}
                  disabled={!can('tenant:manage')}
                >
                  <Upload className="h-4 w-4 mr-2" />
                  Import
                </Button>
              </GlossaryTooltip>
              <GlossaryTooltip termId="export-tenants">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => openModal(MODAL_IDS.EXPORT)}
                >
                  <Download className="h-4 w-4 mr-2" />
                  Export
                </Button>
              </GlossaryTooltip>
            </div>
          </div>
        </CardHeader>
        <CardContent>

          <div className="max-h-[600px] overflow-auto" data-virtual-container>
            <Table className="table-standard">
            <TableHeader>
              <TableRow>
                <TableHead className="table-cell-standard w-12">
                  <Checkbox
                    checked={
                      tenants.length === 0
                        ? false
                        : selectedTenants.length === tenants.length
                          ? true
                          : selectedTenants.length > 0
                            ? 'indeterminate'
                            : false
                    }
                    onCheckedChange={(checked) => {
                      if (checked) {
                        setSelectedTenants(tenants.map(t => t.id));
                      } else {
                        setSelectedTenants([]);
                      }
                    }}
                    aria-label="Select all tenants"
                  />
                </TableHead>
                <TableHead className="table-cell-standard">
                  <GlossaryTooltip termId="tenant-name">
                    <span className="cursor-help">Name</span>
                  </GlossaryTooltip>
                </TableHead>
                <TableHead className="table-cell-standard">
                  <GlossaryTooltip termId="tenant-id">
                    <span className="cursor-help">ID</span>
                  </GlossaryTooltip>
                </TableHead>
                <TableHead className="table-cell-standard">
                  <GlossaryTooltip termId="tenant-isolation">
                    <span className="cursor-help">Isolation</span>
                  </GlossaryTooltip>
                </TableHead>
                <TableHead className="table-cell-standard">
                  <GlossaryTooltip termId="tenant-users">
                    <span className="cursor-help">Users</span>
                  </GlossaryTooltip>
                </TableHead>
                <TableHead className="table-cell-standard">
                  <GlossaryTooltip termId="tenant-adapters">
                    <span className="cursor-help">Adapters</span>
                  </GlossaryTooltip>
                </TableHead>
                <TableHead className="table-cell-standard">
                  <GlossaryTooltip termId="tenant-status">
                    <span className="cursor-help">Status</span>
                  </GlossaryTooltip>
                </TableHead>
                <TableHead className="table-cell-standard">
                  <GlossaryTooltip termId="tenant-actions">
                    <span className="cursor-help">Actions</span>
                  </GlossaryTooltip>
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
                <VirtualizedTableRows items={tenants} estimateSize={60}>
                  {(tenant) => {
                    const tenantTyped = tenant as typeof tenants[0];
                    return (
                      <TableRow key={tenantTyped.id}>
                        <TableCell className="p-4 border-b border-border">
                          <Checkbox
                            checked={selectedTenants.includes(tenantTyped.id)}
                            onCheckedChange={(checked) => {
                              if (checked) {
                                setSelectedTenants(prev => [...prev, tenantTyped.id]);
                              } else {
                                setSelectedTenants(prev => prev.filter(id => id !== tenantTyped.id));
                              }
                            }}
                            aria-label={`Select ${tenantTyped.name}`}
                          />
                        </TableCell>
                        <TableCell className="p-4 border-b border-border" role="gridcell">
                          <div>
                            <p className="font-medium">{tenantTyped.name}</p>
                            <p className="text-sm text-muted-foreground">{tenantTyped.description || 'No description'}</p>
                          </div>
                        </TableCell>
                        <TableCell className="p-4 border-b border-border text-sm text-muted-foreground" role="gridcell">
                          {tenantTyped.id}
                        </TableCell>
                        <TableCell className="p-4 border-b border-border" role="gridcell">
                          <div className="status-indicator status-neutral">
                            {tenantTyped.isolation_level || 'standard'}
                          </div>
                        </TableCell>
                        <TableCell className="p-4 border-b border-border" role="gridcell">
                          <div className="flex items-center gap-1">
                            <UserCheck className="h-3 w-3 text-muted-foreground" />
                            <span>{tenantTyped.users || 0}</span>
                          </div>
                        </TableCell>
                        <TableCell className="p-4 border-b border-border" role="gridcell">
                          <div className="flex items-center gap-1">
                            <Network className="h-3 w-3 text-muted-foreground" />
                            <span>{tenantTyped.adapters || 0}</span>
                          </div>
                        </TableCell>
                        <TableCell className="p-4 border-b border-border" role="gridcell">{getStatusBadge(tenantTyped.status)}</TableCell>
                        <TableCell className="p-4 border-b border-border" role="gridcell">
                          <div className="flex items-center gap-1">
                            <BookmarkButton
                              type="tenant"
                              title={tenantTyped.name}
                              url={`/tenants?tenant=${encodeURIComponent(tenantTyped.id)}`}
                              entityId={tenantTyped.id}
                              description={tenantTyped.description || `Tenant • ${tenantTyped.isolation_level || 'Unknown isolation'}`}
                              variant="ghost"
                              size="icon"
                            />
                            <DropdownMenu>
                              <DropdownMenuTrigger asChild>
                              <Button variant="ghost" size="sm" aria-label={`Actions for ${tenantTyped.name}`}>
                                <MoreHorizontal className="h-4 w-4" />
                              </Button>
                              </DropdownMenuTrigger>
                              <DropdownMenuContent align="end">
                                {tenantTyped.status !== 'paused' && tenantTyped.status !== 'archived' && (
                                  <DropdownMenuItem
                                    onClick={() => handlePause(tenantTyped)}
                                    disabled={!can('tenant:manage')}
                                  >
                                    <Lock className="mr-2 h-4 w-4" />
                                    Pause
                                  </DropdownMenuItem>
                                )}
                                <DropdownMenuItem
                                  onClick={() => {
                                    setSelectedTenantForAction(tenantTyped);
                                    setEditName(tenantTyped.name);
                                    openModal(MODAL_IDS.EDIT);
                                  }}
                                  disabled={!can('tenant:manage')}
                                >
                                  <Edit className="mr-2 h-4 w-4" />
                                  Edit
                                </DropdownMenuItem>
                                <DropdownMenuItem
                                  onClick={() => {
                                    setSelectedTenantForAction(tenantTyped);
                                    openModal(MODAL_IDS.ASSIGN_POLICIES);
                                  }}
                                  disabled={!can('tenant:manage')}
                                >
                                  <Shield className="mr-2 h-4 w-4" />
                                  Assign Policies
                                </DropdownMenuItem>
                                <DropdownMenuItem
                                  onClick={() => {
                                    setSelectedTenantForAction(tenantTyped);
                                    openModal(MODAL_IDS.ASSIGN_ADAPTERS);
                                  }}
                                  disabled={!can('tenant:manage')}
                                >
                                  <Layers className="mr-2 h-4 w-4" />
                                  Assign Adapters
                                </DropdownMenuItem>
                                <DropdownMenuItem onClick={() => handleViewUsage(tenantTyped)}>
                                  <BarChart3 className="mr-2 h-4 w-4" />
                                  View Usage
                                </DropdownMenuItem>
                                <DropdownMenuItem
                                  onClick={() => {
                                    setSelectedTenantForAction(tenantTyped);
                                    openModal(MODAL_IDS.ARCHIVE);
                                  }}
                                  disabled={!can('tenant:manage')}
                                >
                                  <Archive className="mr-2 h-4 w-4 text-red-600" />
                                  Archive
                                </DropdownMenuItem>
                              </DropdownMenuContent>
                            </DropdownMenu>
                          </div>
                        </TableCell>
                      </TableRow>
                    );
                  }}
                </VirtualizedTableRows>
              </TableBody>
            </Table>
          </div>
        </CardContent>
      </Card>

      {/* Edit Tenant Modal */}
      <Dialog open={isOpen(MODAL_IDS.EDIT)} onOpenChange={(open) => !open && closeModal()}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Edit Organization</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <div className="flex items-center gap-1 mb-1">
                <Label>Organization Name</Label>
                <GlossaryTooltip termId="tenant-name">
                  <span className="cursor-help text-muted-foreground">(?)</span>
                </GlossaryTooltip>
              </div>
              <Input
                value={editName}
                onChange={(e) => setEditName(e.target.value)}
                placeholder="Enter organization name"
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={closeModal} aria-label="Cancel tenant edit">
              Cancel
            </Button>
            <GlossaryTooltip termId="save-tenant-changes">
              <Button onClick={handleEdit} aria-label="Save tenant changes" disabled={!can('tenant:manage')}>
                Save Changes
              </Button>
            </GlossaryTooltip>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Assign Policies Modal */}
      <Dialog open={isOpen(MODAL_IDS.ASSIGN_POLICIES)} onOpenChange={(open) => !open && closeModal()}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Assign Policies to {selectedTenantForAction?.name}</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 max-h-96 overflow-y-auto">
            {policies.map((policy) => (
              <div key={policy.cpid} className="flex items-center space-x-2 p-2 border rounded">
                <input
                  type="checkbox"
                  id={`policy-${policy.cpid}`}
                  checked={selectedPolicies.includes(policy.cpid || '')}
                  onChange={(e) => {
                    if (e.target.checked && policy.cpid) {
                      setSelectedPolicies([...selectedPolicies, policy.cpid]);
                    } else if (policy.cpid) {
                      setSelectedPolicies(selectedPolicies.filter(id => id !== policy.cpid));
                    }
                  }}
                  className="h-4 w-4"
                />
                <label htmlFor={`policy-${policy.cpid}`} className="flex-1 cursor-pointer">
                  <p className="font-medium">{policy.cpid || 'Unknown Policy'}</p>
                  <p className="text-xs text-muted-foreground">Hash: {policy.schema_hash?.substring(0, 16) || 'N/A'}</p>
                </label>
              </div>
            ))}
            {policies.length === 0 && (
              <p className="text-center text-muted-foreground">No policies available</p>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => {
              closeModal();
              setSelectedPolicies([]);
            }}>
              Cancel
            </Button>
            <GlossaryTooltip termId="assign-policies-action">
              <Button onClick={handleAssignPolicies} disabled={selectedPolicies.length === 0 || !can('tenant:manage')}>
                Assign {selectedPolicies.length} Policies
              </Button>
            </GlossaryTooltip>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Assign Adapters Modal */}
      <Dialog open={isOpen(MODAL_IDS.ASSIGN_ADAPTERS)} onOpenChange={(open) => !open && closeModal()}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Assign Adapters to {selectedTenantForAction?.name}</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 max-h-96 overflow-y-auto">
            {adapters.map((adapter) => (
              <div key={adapter.id} className="flex items-center space-x-2 p-2 border rounded">
                <input
                  type="checkbox"
                  id={`adapter-${adapter.id}`}
                  checked={selectedAdapters.includes(adapter.id)}
                  onChange={(e) => {
                    if (e.target.checked) {
                      setSelectedAdapters([...selectedAdapters, adapter.id]);
                    } else {
                      setSelectedAdapters(selectedAdapters.filter(id => id !== adapter.id));
                    }
                  }}
                  className="h-4 w-4"
                />
                <label htmlFor={`adapter-${adapter.id}`} className="flex-1 cursor-pointer">
                  <p className="font-medium">{adapter.name}</p>
                  <p className="text-xs text-muted-foreground">Rank: {adapter.rank}</p>
                </label>
              </div>
            ))}
            {adapters.length === 0 && (
              <p className="text-center text-muted-foreground">No adapters available</p>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => {
              closeModal();
              setSelectedAdapters([]);
            }}>
              Cancel
            </Button>
            <GlossaryTooltip termId="assign-adapters-action">
              <Button onClick={handleAssignAdapters} disabled={selectedAdapters.length === 0 || !can('tenant:manage')}>
                Assign {selectedAdapters.length} Adapters
              </Button>
            </GlossaryTooltip>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* View Usage Modal */}
      <Dialog open={isOpen(MODAL_IDS.USAGE)} onOpenChange={(open) => !open && closeModal()}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Organization Usage - {selectedTenantForAction?.name}</DialogTitle>
          </DialogHeader>
          {usageData && (
            <div className="space-y-4">
              <div>
                <Label>CPU Usage</Label>
                <Progress value={usageData.cpu_usage_pct ?? 0} className="mt-2" />
                <p className="text-sm text-muted-foreground mt-1">{formatPercent(usageData.cpu_usage_pct)}</p>
              </div>
              <div>
                <Label>GPU Usage</Label>
                <Progress value={usageData.gpu_usage_pct ?? 0} className="mt-2" />
                <p className="text-sm text-muted-foreground mt-1">{formatPercent(usageData.gpu_usage_pct)}</p>
              </div>
              <div>
                <Label>Memory Usage</Label>
                <p className="text-sm">{(usageData.memory_used_gb ?? 0).toFixed(2)} GB / {(usageData.memory_total_gb ?? 0).toFixed(2)} GB</p>
                <Progress value={usageData.memory_total_gb ? ((usageData.memory_used_gb ?? 0) / usageData.memory_total_gb) * 100 : 0} className="mt-2" />
              </div>
              <div>
                <Label>Inference Count (24h)</Label>
                <p className="text-lg font-medium">{formatCount(usageData.inference_count_24h)}</p>
              </div>
              <div>
                <Label>Active Adapters</Label>
                <p>{formatCount(usageData.active_adapters_count)}</p>
              </div>
            </div>
          )}
          <DialogFooter>
            {usageData && (
              <GlossaryTooltip termId="export-usage-csv">
                <Button
                  variant="outline"
                  onClick={() => {
                    const rows = [
                      ['cpu_usage_pct', (usageData.cpu_usage_pct ?? 0).toFixed(1)],
                      ['gpu_usage_pct', (usageData.gpu_usage_pct ?? 0).toFixed(1)],
                      ['memory_used_gb', (usageData.memory_used_gb ?? 0).toFixed(2)],
                      ['memory_total_gb', (usageData.memory_total_gb ?? 0).toFixed(2)],
                      ['inference_count_24h', (usageData.inference_count_24h ?? 0).toString()],
                      ['active_adapters_count', (usageData.active_adapters_count ?? 0).toString()],
                    ];
                    const csv = 'key,value\n' + rows.map(r => r.join(',')).join('\n');
                    const blob = new Blob([csv], { type: 'text/csv' });
                    const url = URL.createObjectURL(blob);
                    const a = document.createElement('a');
                    a.href = url;
                    a.download = `tenant-usage-${selectedTenantForAction?.id}.csv`;
                    a.click();
                    URL.revokeObjectURL(url);
                  }}
                >
                  Export CSV
                </Button>
              </GlossaryTooltip>
            )}
            <Button onClick={closeModal}>Close</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Archive Tenant Modal */}
      <Dialog open={isOpen(MODAL_IDS.ARCHIVE)} onOpenChange={(open) => !open && closeModal()}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Archive Organization</DialogTitle>
          </DialogHeader>
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertDescription>
              This will archive organization <strong>{selectedTenantForAction?.name}</strong>.
              All associated resources will be suspended. This action can be reversed by an administrator.
            </AlertDescription>
          </Alert>
          <DialogFooter>
            <Button variant="outline" onClick={() => {
              closeModal();
              setSelectedTenantForAction(null);
            }}>
              Cancel
            </Button>
            <GlossaryTooltip termId="archive-tenant-action">
              <Button variant="destructive" onClick={handleArchive} disabled={!can('tenant:manage')}>
                Archive Organization
              </Button>
            </GlossaryTooltip>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Bulk Action Bar */}
      <BulkActionBar
        selectedItems={selectedTenants}
        actions={bulkActions}
        onClearSelection={() => setSelectedTenants([])}
        itemName="tenant"
      />

      {/* Confirmation Dialog */}
      <ConfirmationDialog
        open={confirmationOpen}
        onOpenChange={(open) => {
          setConfirmationOpen(open);
          if (!open) {
            setPendingBulkAction(null);
            setConfirmationOptions(null);
          }
        }}
        onConfirm={async () => {
          if (pendingBulkAction) {
            await pendingBulkAction();
            setPendingBulkAction(null);
            setConfirmationOptions(null);
          }
        }}
        options={confirmationOptions || {
          title: 'Confirm Action',
          description: 'Are you sure?',
          variant: 'default'
        }}
      />

      {/* Export Dialog */}
      <ExportDialog
        open={isOpen(MODAL_IDS.EXPORT)}
        onOpenChange={(open) => !open && closeModal()}
        onExport={handleExport}
        itemName="tenants"
        hasSelected={selectedTenants.length > 0}
        hasFilters={false}
        defaultFormat="json"
        defaultScope={selectedTenants.length > 0 ? 'selected' : 'all'}
      />

      {/* Import Dialog */}
      <Dialog open={isOpen(MODAL_IDS.IMPORT)} onOpenChange={(open) => !open && closeModal()}>
        <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Import Organization</DialogTitle>
          </DialogHeader>
          <TenantImportWizard
            onComplete={(tenant) => {
              closeModal();
              refetchTenants();
              showStatus(`Organization "${tenant.name}" created successfully.`, 'success');
            }}
            onCancel={closeModal}
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
