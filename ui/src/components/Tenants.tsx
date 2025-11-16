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
import { useUndoRedoContext } from '../contexts/UndoRedoContext';
import { TenantImportWizard } from './TenantImportWizard';
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
import apiClient from '../api/client';
import { Tenant as ApiTenant, User, Policy, Adapter, TenantUsageResponse } from '../api/types';
<<<<<<< HEAD
import { logger, toError } from '../utils/logger';
import { ErrorRecoveryTemplates } from './ui/error-recovery';
import { BookmarkButton } from './ui/bookmark-button';
=======
import { toast } from 'sonner';
import { logger } from '../utils/logger';
>>>>>>> integration-branch

interface TenantsProps {
  user: User;
  selectedTenant: string;
}

export function Tenants({ user, selectedTenant }: TenantsProps) {
  const { addAction } = useUndoRedoContext();
  const [tenants, setTenants] = useState<ApiTenant[]>([]);
  const [loading, setLoading] = useState(true);
  const [showEditModal, setShowEditModal] = useState(false);
  const [showAssignPoliciesModal, setShowAssignPoliciesModal] = useState(false);
  const [showAssignAdaptersModal, setShowAssignAdaptersModal] = useState(false);
  const [showUsageModal, setShowUsageModal] = useState(false);
  const [showArchiveModal, setShowArchiveModal] = useState(false);
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
  const [showExportDialog, setShowExportDialog] = useState(false);
  const [showImportDialog, setShowImportDialog] = useState(false);
  const userId = user.id;

  const showStatus = (message: string, variant: 'success' | 'info' | 'warning') => {
    setStatusMessage({ message, variant });
  };

  const fetchTenants = useCallback(async () => {
    setLoading(true);
    try {
      const data = await apiClient.listTenants();
      setTenants(data);
      setStatusMessage(null);
      setErrorRecovery(null);
    } catch (err) {
<<<<<<< HEAD
      logger.error('Failed to fetch tenants', {
        component: 'Tenants',
        operation: 'fetchTenants',
        userId
      }, err instanceof Error ? err : new Error(String(err)));
      setStatusMessage({ message: 'Failed to load tenants.', variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to load tenants'),
          () => fetchTenants()
        )
      );
=======
      // Replace: console.error('Failed to fetch tenants:', err);
      logger.error('Failed to fetch tenants', {
        component: 'Tenants',
        operation: 'fetchTenants',
        userId: user.id
      }, err instanceof Error ? err : new Error(String(err)));
      toast.error('Failed to load tenants');
>>>>>>> integration-branch
    } finally {
      setLoading(false);
    }
  }, [userId]);

  useEffect(() => {
    fetchTenants();
  }, [fetchTenants]);

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
<<<<<<< HEAD
        logger.error('Failed to fetch policies/adapters', {
          component: 'Tenants',
          operation: 'fetchPoliciesAdapters',
          userId
=======
        // Replace: console.error('Failed to fetch policies/adapters:', err);
        logger.error('Failed to fetch policies/adapters', {
          component: 'Tenants',
          operation: 'fetchPoliciesAdapters',
          userId: user.id
>>>>>>> integration-branch
        }, err instanceof Error ? err : new Error(String(err)));
      }
    };
    fetchData();
  }, [userId]);

  const handleEdit = async () => {
    if (!selectedTenantForAction) return;
    try {
      await apiClient.updateTenant(selectedTenantForAction.id, editName);
      showStatus('Tenant updated.', 'success');
      setShowEditModal(false);
      fetchTenants();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to update tenant';
      setStatusMessage({ message: errorMsg, variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMsg),
          () => handleEdit()
        )
      );
    }
  };


  const handleArchive = async () => {
    if (!selectedTenantForAction) return;
    try {
      const tenant = selectedTenantForAction;
      await apiClient.archiveTenant(tenant.id);
      showStatus('Tenant archived.', 'success');
      setShowArchiveModal(false);
      setSelectedTenantForAction(null);
      await fetchTenants();

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
      setStatusMessage({ message: errorMsg, variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMsg),
          () => handleArchive()
        )
      );
    }
  };

  const handleAssignPolicies = async () => {
    if (!selectedTenantForAction) return;
    try {
      await apiClient.assignTenantPolicies(selectedTenantForAction.id, selectedPolicies);
      showStatus(`Assigned ${selectedPolicies.length} policies.`, 'success');
      setShowAssignPoliciesModal(false);
      setSelectedPolicies([]);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to assign policies';
      setStatusMessage({ message: errorMsg, variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMsg),
          () => handleAssignPolicies()
        )
      );
    }
  };

  const handleAssignAdapters = async () => {
    if (!selectedTenantForAction) return;
    try {
      await apiClient.assignTenantAdapters(selectedTenantForAction.id, selectedAdapters);
      showStatus(`Assigned ${selectedAdapters.length} adapters.`, 'success');
      setShowAssignAdaptersModal(false);
      setSelectedAdapters([]);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to assign adapters';
      setStatusMessage({ message: errorMsg, variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMsg),
          () => handleAssignAdapters()
        )
      );
    }
  };

  const handleViewUsage = async (tenant: ApiTenant) => {
    try {
      const usage = await apiClient.getTenantUsage(tenant.id);
      setUsageData(usage);
      setSelectedTenantForAction(tenant);
      setShowUsageModal(true);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to fetch tenant usage';
      setStatusMessage({ message: errorMsg, variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMsg),
          () => handleViewUsage(tenant)
        )
      );
    }
  };

  const handlePause = async (tenant: ApiTenant) => {
    try {
      const previousStatus = tenant.status;
      await apiClient.pauseTenant(tenant.id);
      showStatus(`Tenant "${tenant.name}" paused.`, 'success');
      await fetchTenants();

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
          await fetchTenants();
        },
      });
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to pause tenant';
      setStatusMessage({ message: errorMsg, variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMsg),
          () => handlePause(tenant)
        )
      );
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
        showStatus(`Successfully paused ${successCount} tenant(s).`, 'success');
        
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
        setErrorRecovery(
          ErrorRecoveryTemplates.genericError(
            new Error(`Failed to pause ${errorCount} tenant(s).`),
            () => performBulkPause()
          )
        );
      }

      await fetchTenants();
      setSelectedTenants([]);
    };

    setConfirmationOptions({
      title: 'Pause Tenants',
      description: `Pause ${tenantIds.length} tenant(s)? This will stop new sessions for these tenants.`,
      confirmText: 'Pause Tenants',
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
        showStatus(`Successfully archived ${successCount} tenant(s).`, 'success');
        
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
        setErrorRecovery(
          ErrorRecoveryTemplates.genericError(
            new Error(`Failed to archive ${errorCount} tenant(s).`),
            () => performBulkArchive()
          )
        );
      }

      await fetchTenants();
      setSelectedTenants([]);
    };

    setConfirmationOptions({
      title: 'Archive Tenants',
      description: `Permanently archive ${tenantIds.length} tenant(s)? All associated resources will be suspended. This action can be reversed by an administrator.`,
      confirmText: 'Archive Tenants',
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
        showStatus('No tenants to export.', 'warning');
        setShowExportDialog(false);
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

      showStatus(`Exported ${tenantsToExport.length} tenant(s).`, 'success');
      setShowExportDialog(false);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to export tenants');
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          error,
          () => handleExport(options)
        )
      );
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
<<<<<<< HEAD
        return <div className="status-indicator status-neutral"><Lock className="h-3 w-3" />Inactive</div>;
=======
        return <div className="status-indicator status-neutral"><Lock className="icon-small" />Inactive</div>;
>>>>>>> integration-branch
      case 'archived':
        return <div className="status-indicator status-neutral"><Database className="h-3 w-3" />Archived</div>;
      default:
        return <div className="status-indicator status-neutral">Unknown</div>;
    }
  };

  const getClassificationBadge = (classification?: ApiTenant['data_classification']) => {
    const current = classification || 'internal';
    const colors = {
      public: 'status-info',
      internal: 'status-neutral',
      confidential: 'status-warning',
      restricted: 'status-error'
    };
    
    return (
      <div className={`status-indicator ${colors[current]}`}>
        <Lock className="h-3 w-3" />
        {current.toUpperCase()}
      </div>
    );
  };

  const handleCreateTenant = async () => {
    if (!newTenant.name.trim()) return;
    try {
      await apiClient.createTenant({ name: newTenant.name, isolation_level: 'standard' });
      showStatus('Tenant created.', 'success');
      setNewTenant({ name: '', description: '', dataClassification: 'internal', itarCompliant: false });
      setIsCreateDialogOpen(false);
      await fetchTenants();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to create tenant';
      setStatusMessage({ message: errorMsg, variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMsg),
          () => handleCreateTenant()
        )
      );
    }
  };

  if (user.role !== 'admin') {
    return (
      <div className="flex items-center justify-center h-96">
        <div className="text-center space-y-2">
          <Shield className="h-12 w-12 text-muted-foreground mx-auto" />
          <h3>Access Restricted</h3>
          <p className="text-muted-foreground">
            Tenant management requires Administrator privileges.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {errorRecovery && (
        <div>
          {errorRecovery}
        </div>
      )}

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
          <h1 className="text-2xl font-bold">Tenant Management</h1>
          <p className="text-sm text-muted-foreground">
            Manage tenant isolation, data classification, and access controls
          </p>
        </div>
        <Dialog open={isCreateDialogOpen} onOpenChange={setIsCreateDialogOpen}>
          <DialogTrigger asChild>
            <Button>
              <Plus className="h-4 w-4 mr-2" />
              Create Tenant
            </Button>
          </DialogTrigger>
          <DialogContent className="max-w-md">
            <DialogHeader>
              <DialogTitle>Create New Tenant</DialogTitle>
            </DialogHeader>
            <div className="mb-4">
              <div className="mb-4">
                <Label htmlFor="name" className="font-medium text-sm mb-1">Tenant Name</Label>
                <Input
                  id="name"
                  placeholder="Enter tenant name"
                  value={newTenant.name}
                  onChange={(e) => setNewTenant({ ...newTenant, name: e.target.value })}
                />
              </div>
              
              <div className="mb-4">
                <Label htmlFor="description" className="font-medium text-sm mb-1">Description</Label>
                <Textarea
                  id="description"
                  placeholder="Describe the tenant's purpose"
                  value={newTenant.description}
                  onChange={(e) => setNewTenant({ ...newTenant, description: e.target.value })}
                />
              </div>
              
              <div className="mb-4">
                <Label htmlFor="classification" className="font-medium text-sm mb-1">Data Classification</Label>
                <Select 
                  value={newTenant.dataClassification} 
                  onValueChange={(value: any) => setNewTenant({ ...newTenant, dataClassification: value })}
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
                <Label htmlFor="itar" className="font-medium text-sm mb-1">ITAR Compliance Required</Label>
                <Switch
                  id="itar"
                  checked={newTenant.itarCompliant}
                  onCheckedChange={(checked) => setNewTenant({ ...newTenant, itarCompliant: checked })}
                />
              </div>
              
              <div className="flex items-center justify-end">
                <Button variant="outline" onClick={() => setIsCreateDialogOpen(false)}>
                  Cancel
                </Button>
                <Button onClick={handleCreateTenant} disabled={!newTenant.name.trim()}>
                  Create Tenant
                </Button>
              </div>
            </div>
          </DialogContent>
        </Dialog>
      </div>

      {/* Tenant Statistics */}
      <div className="grid-standard grid-cols-4">
        <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
          <CardContent className="pt-6">
            <div className="flex items-center justify-center">
              <Users className="h-4 w-4 text-blue-600" />
              <div>
                <p className="text-2xl font-bold">{tenants.length}</p>
                <p className="text-xs text-muted-foreground">Total Tenants</p>
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
                <p className="text-2xl font-bold">{tenants.reduce((sum, t) => sum + (t.adapters ?? 0), 0)}</p>
                <p className="text-xs text-muted-foreground">Total Adapters</p>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Tenants Table */}
      <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>Active Tenants</CardTitle>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => setShowImportDialog(true)}
              >
                <Upload className="h-4 w-4 mr-2" />
                Import
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => setShowExportDialog(true)}
              >
                <Download className="h-4 w-4 mr-2" />
                Export
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
<<<<<<< HEAD
          <div className="max-h-[600px] overflow-auto" data-virtual-container>
            <Table className="border-collapse w-full" role="table" aria-label="Tenant management">
              <TableHeader>
                <TableRow role="row">
                  <TableHead className="p-4 border-b border-border w-12" role="columnheader" scope="col">
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
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Tenant</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Status</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Classification</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Users</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Adapters</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Policies</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">ITAR</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Last Activity</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Actions</TableHead>
=======
          <Table className="table-standard">
            <TableHeader>
              <TableRow>
                <TableHead className="table-cell-standard">Tenant</TableHead>
                <TableHead className="table-cell-standard">Status</TableHead>
                <TableHead className="table-cell-standard">Classification</TableHead>
                <TableHead className="table-cell-standard">Users</TableHead>
                <TableHead className="table-cell-standard">Adapters</TableHead>
                <TableHead className="table-cell-standard">Policies</TableHead>
                <TableHead className="table-cell-standard">ITAR</TableHead>
                <TableHead className="table-cell-standard">Last Activity</TableHead>
                <TableHead className="table-cell-standard">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {tenants.map((tenant) => (
                <TableRow key={tenant.id}>
                  <TableCell className="table-cell-standard">
                    <div>
                      <p className="font-medium">{tenant.name}</p>
                      <p className="text-sm text-muted-foreground">{tenant.description || 'No description'}</p>
                    </div>
                  </TableCell>
                  <TableCell className="table-cell-standard">{getStatusBadge(tenant.status)}</TableCell>
                  <TableCell className="table-cell-standard">{getClassificationBadge(tenant.data_classification)}</TableCell>
                  <TableCell className="table-cell-standard">
                    <div className="flex-center">
                      <UserCheck className="icon-small text-muted-foreground" />
                      <span>{tenant.users || 0}</span>
                    </div>
                  </TableCell>
                  <TableCell className="table-cell-standard">
                    <div className="flex-center">
                      <Network className="icon-small text-muted-foreground" />
                      <span>{tenant.adapters || 0}</span>
                    </div>
                  </TableCell>
                  <TableCell className="table-cell-standard">
                    <div className="flex-center">
                      <Shield className="icon-small text-muted-foreground" />
                      <span>{tenant.policies || 0}</span>
                    </div>
                  </TableCell>
                  <TableCell className="table-cell-standard">
                    {tenant.itar_compliant ? (
                      <div className="status-indicator status-success">Yes</div>
                    ) : (
                      <div className="status-indicator status-neutral">No</div>
                    )}
                  </TableCell>
                  <TableCell className="table-cell-standard text-sm text-muted-foreground">
                    {tenant.last_activity || 'Unknown'}
                  </TableCell>
                  <TableCell className="table-cell-standard">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant="ghost" size="sm">
                          <MoreHorizontal className="h-4 w-4" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem onClick={() => {
                          setSelectedTenantForAction(tenant);
                          setEditName(tenant.name);
                          setShowEditModal(true);
                        }}>
                          <Edit className="mr-2 h-4 w-4" />
                          Edit
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => {
                          setSelectedTenantForAction(tenant);
                          setShowAssignPoliciesModal(true);
                        }}>
                          <Shield className="mr-2 h-4 w-4" />
                          Assign Policies
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => {
                          setSelectedTenantForAction(tenant);
                          setShowAssignAdaptersModal(true);
                        }}>
                          <Layers className="mr-2 h-4 w-4" />
                          Assign Adapters
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => handleViewUsage(tenant)}>
                          <BarChart3 className="mr-2 h-4 w-4" />
                          View Usage
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => {
                          setSelectedTenantForAction(tenant);
                          setShowArchiveModal(true);
                        }}>
                          <Archive className="mr-2 h-4 w-4 text-red-600" />
                          Archive
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </TableCell>
>>>>>>> integration-branch
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
                        <TableCell className="p-4 border-b border-border" role="gridcell">{getStatusBadge(tenantTyped.status)}</TableCell>
                        <TableCell className="p-4 border-b border-border" role="gridcell">{getClassificationBadge(tenantTyped.data_classification)}</TableCell>
                        <TableCell className="p-4 border-b border-border" role="gridcell">
                          <div className="flex items-center justify-center">
                            <UserCheck className="h-3 w-3 text-muted-foreground" />
                            <span>{tenantTyped.users || 0}</span>
                          </div>
                        </TableCell>
                        <TableCell className="p-4 border-b border-border" role="gridcell">
                          <div className="flex items-center justify-center">
                            <Network className="h-3 w-3 text-muted-foreground" />
                            <span>{tenantTyped.adapters || 0}</span>
                          </div>
                        </TableCell>
                        <TableCell className="p-4 border-b border-border" role="gridcell">
                          <div className="flex items-center justify-center">
                            <Shield className="h-3 w-3 text-muted-foreground" />
                            <span>{tenantTyped.policies || 0}</span>
                          </div>
                        </TableCell>
                        <TableCell className="p-4 border-b border-border" role="gridcell">
                          {tenantTyped.itar_compliant ? (
                            <div className="status-indicator status-success">Yes</div>
                          ) : (
                            <div className="status-indicator status-neutral">No</div>
                          )}
                        </TableCell>
                        <TableCell className="p-4 border-b border-border text-sm text-muted-foreground" role="gridcell">
                          {tenantTyped.last_activity || 'Unknown'}
                        </TableCell>
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
                                  <DropdownMenuItem onClick={() => handlePause(tenantTyped)}>
                                    <Lock className="mr-2 h-4 w-4" />
                                    Pause
                                  </DropdownMenuItem>
                                )}
                                <DropdownMenuItem onClick={() => {
                                  setSelectedTenantForAction(tenantTyped);
                                  setEditName(tenantTyped.name);
                                  setShowEditModal(true);
                                }}>
                                  <Edit className="mr-2 h-4 w-4" />
                                  Edit
                                </DropdownMenuItem>
                                <DropdownMenuItem onClick={() => {
                                  setSelectedTenantForAction(tenantTyped);
                                  setShowAssignPoliciesModal(true);
                                }}>
                                  <Shield className="mr-2 h-4 w-4" />
                                  Assign Policies
                                </DropdownMenuItem>
                                <DropdownMenuItem onClick={() => {
                                  setSelectedTenantForAction(tenantTyped);
                                  setShowAssignAdaptersModal(true);
                                }}>
                                  <Layers className="mr-2 h-4 w-4" />
                                  Assign Adapters
                                </DropdownMenuItem>
                                <DropdownMenuItem onClick={() => handleViewUsage(tenantTyped)}>
                                  <BarChart3 className="mr-2 h-4 w-4" />
                                  View Usage
                                </DropdownMenuItem>
                                <DropdownMenuItem onClick={() => {
                                  setSelectedTenantForAction(tenantTyped);
                                  setShowArchiveModal(true);
                                }}>
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
      <Dialog open={showEditModal} onOpenChange={setShowEditModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Edit Tenant</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <Label>Tenant Name</Label>
              <Input 
                value={editName} 
                onChange={(e) => setEditName(e.target.value)}
                placeholder="Enter tenant name"
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowEditModal(false)} aria-label="Cancel tenant edit">
              Cancel
            </Button>
            <Button onClick={handleEdit} aria-label="Save tenant changes">Save Changes</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Assign Policies Modal */}
      <Dialog open={showAssignPoliciesModal} onOpenChange={setShowAssignPoliciesModal}>
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
                  checked={selectedPolicies.includes(policy.cpid)}
                  onChange={(e) => {
                    if (e.target.checked) {
                      setSelectedPolicies([...selectedPolicies, policy.cpid]);
                    } else {
                      setSelectedPolicies(selectedPolicies.filter(id => id !== policy.cpid));
                    }
                  }}
                  className="h-4 w-4"
                />
                <label htmlFor={`policy-${policy.cpid}`} className="flex-1 cursor-pointer">
                  <p className="font-medium">{policy.cpid}</p>
                  <p className="text-xs text-muted-foreground">Hash: {policy.schema_hash.substring(0, 16)}</p>
                </label>
              </div>
            ))}
            {policies.length === 0 && (
              <p className="text-center text-muted-foreground">No policies available</p>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => {
              setShowAssignPoliciesModal(false);
              setSelectedPolicies([]);
            }}>
              Cancel
            </Button>
            <Button onClick={handleAssignPolicies} disabled={selectedPolicies.length === 0}>
              Assign {selectedPolicies.length} Policies
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Assign Adapters Modal */}
      <Dialog open={showAssignAdaptersModal} onOpenChange={setShowAssignAdaptersModal}>
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
              setShowAssignAdaptersModal(false);
              setSelectedAdapters([]);
            }}>
              Cancel
            </Button>
            <Button onClick={handleAssignAdapters} disabled={selectedAdapters.length === 0}>
              Assign {selectedAdapters.length} Adapters
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* View Usage Modal */}
      <Dialog open={showUsageModal} onOpenChange={setShowUsageModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Tenant Usage - {selectedTenantForAction?.name}</DialogTitle>
          </DialogHeader>
          {usageData && (
            <div className="space-y-4">
              <div>
                <Label>CPU Usage</Label>
                <Progress value={usageData.cpu_usage_pct} className="mt-2" />
                <p className="text-sm text-muted-foreground mt-1">{usageData.cpu_usage_pct.toFixed(1)}%</p>
              </div>
              <div>
                <Label>GPU Usage</Label>
                <Progress value={usageData.gpu_usage_pct} className="mt-2" />
                <p className="text-sm text-muted-foreground mt-1">{usageData.gpu_usage_pct.toFixed(1)}%</p>
              </div>
              <div>
                <Label>Memory Usage</Label>
                <p className="text-sm">{usageData.memory_used_gb.toFixed(2)} GB / {usageData.memory_total_gb.toFixed(2)} GB</p>
                <Progress value={(usageData.memory_used_gb / usageData.memory_total_gb) * 100} className="mt-2" />
              </div>
              <div>
                <Label>Inference Count (24h)</Label>
                <p className="text-lg font-medium">{usageData.inference_count_24h.toLocaleString()}</p>
              </div>
              <div>
                <Label>Active Adapters</Label>
                <p>{usageData.active_adapters_count}</p>
              </div>
            </div>
          )}
          <DialogFooter>
            {usageData && (
              <Button
                variant="outline"
                onClick={() => {
                  const rows = [
                    ['cpu_usage_pct', usageData.cpu_usage_pct.toFixed(1)],
                    ['gpu_usage_pct', usageData.gpu_usage_pct.toFixed(1)],
                    ['memory_used_gb', usageData.memory_used_gb.toFixed(2)],
                    ['memory_total_gb', usageData.memory_total_gb.toFixed(2)],
                    ['inference_count_24h', usageData.inference_count_24h.toString()],
                    ['active_adapters_count', usageData.active_adapters_count.toString()],
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
            )}
            <Button onClick={() => setShowUsageModal(false)}>Close</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Archive Tenant Modal */}
      <Dialog open={showArchiveModal} onOpenChange={setShowArchiveModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Archive Tenant</DialogTitle>
          </DialogHeader>
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertDescription>
              This will archive tenant <strong>{selectedTenantForAction?.name}</strong>. 
              All associated resources will be suspended. This action can be reversed by an administrator.
            </AlertDescription>
          </Alert>
          <DialogFooter>
            <Button variant="outline" onClick={() => {
              setShowArchiveModal(false);
              setSelectedTenantForAction(null);
            }}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleArchive}>
              Archive Tenant
            </Button>
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
        open={showExportDialog}
        onOpenChange={setShowExportDialog}
        onExport={handleExport}
        itemName="tenants"
        hasSelected={selectedTenants.length > 0}
        hasFilters={false}
        defaultFormat="json"
        defaultScope={selectedTenants.length > 0 ? 'selected' : 'all'}
      />

      {/* Undo/Redo Bar */}

      {/* Import Dialog */}
      <Dialog open={showImportDialog} onOpenChange={setShowImportDialog}>
        <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Import Tenant</DialogTitle>
          </DialogHeader>
          <TenantImportWizard
            onComplete={(tenant) => {
              setShowImportDialog(false);
              fetchTenants();
              showStatus(`Tenant "${tenant.name}" created successfully.`, 'success');
            }}
            onCancel={() => setShowImportDialog(false)}
          />
        </DialogContent>
      </Dialog>
    </div>
  );
}
