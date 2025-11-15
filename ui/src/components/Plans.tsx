import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { Label } from './ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Alert, AlertDescription } from './ui/alert';
import { Checkbox } from './ui/checkbox';
import { BulkActionBar, BulkAction } from './ui/bulk-action-bar';
import { ConfirmationDialog, ConfirmationOptions } from './ui/confirmation-dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';
import { 
  FileText, 
  Plus, 
  Zap, 
  MoreHorizontal, 
  RefreshCw, 
  Download, 
  GitCompare,
  Undo2,
  CheckCircle,
  AlertTriangle
} from 'lucide-react';
import apiClient from '../api/client';
import { Plan, User, PlanComparisonResponse } from '../api/types';
import { logger, toError } from '../utils/logger';
import { ErrorRecoveryTemplates } from './ui/error-recovery';

interface PlansProps {
  user: User;
  selectedTenant: string;
}

export function Plans({ user, selectedTenant }: PlansProps) {
  const [plans, setPlans] = useState<Plan[]>([]);
  const [loading, setLoading] = useState(true);
  const [showCompareModal, setShowCompareModal] = useState(false);
  const [selectedPlan1, setSelectedPlan1] = useState('');
  const [selectedPlan2, setSelectedPlan2] = useState('');
  const [compareResult, setCompareResult] = useState<PlanComparisonResponse | null>(null);
  const [showRollbackModal, setShowRollbackModal] = useState(false);
  const [showBuildModal, setShowBuildModal] = useState(false);
  const [manifestHash, setManifestHash] = useState('');
  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'info' | 'warning' } | null>(null);
  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);
  const [selectedPlans, setSelectedPlans] = useState<string[]>([]);
  const [confirmationOpen, setConfirmationOpen] = useState(false);
  const [confirmationOptions, setConfirmationOptions] = useState<ConfirmationOptions | null>(null);
  const [pendingBulkAction, setPendingBulkAction] = useState<(() => Promise<void>) | null>(null);

  const bulkActions: BulkAction[] = [
    {
      id: 'delete',
      label: 'Delete Selected',
      variant: 'destructive',
      handler: async (selectedItems) => {
        setConfirmationOptions({
          title: 'Delete Plans',
          description: `Are you sure you want to delete ${selectedItems.length} plan(s)? This action cannot be undone.`,
          confirmText: 'Delete',
          variant: 'destructive',
        });
        setPendingBulkAction(() => {
          const performDeletion = async () => {
            try {
              await Promise.all(selectedItems.map((planId) => apiClient.deletePlan(planId)));
              showStatus(
                selectedItems.length === 1
                  ? 'Plan deleted successfully.'
                  : `${selectedItems.length} plans deleted successfully.`,
                'success'
              );
              setErrorRecovery(null);
              setSelectedPlans([]);
              await fetchPlans();
            } catch (err) {
              const error = err instanceof Error ? err : new Error('Failed to delete plans');
              logger.error('Failed to delete plans', {
                component: 'Plans',
                operation: 'deletePlans',
                planIds: selectedItems,
                tenantId: selectedTenant,
              }, error);
              showStatus('Failed to delete selected plans.', 'warning');
              setErrorRecovery(
                ErrorRecoveryTemplates.genericError(
                  error,
                  () => performDeletion()
                )
              );
            }
          };
          return performDeletion;
        });
        setConfirmationOpen(true);
      },
    },
  ];

  const showStatus = (message: string, variant: 'success' | 'info' | 'warning') => {
    setStatusMessage({ message, variant });
  };

  const fetchPlans = useCallback(async () => {
    setLoading(true);
    try {
      const data = await apiClient.listPlans();
      setPlans(data);
      setStatusMessage(null);
      setErrorRecovery(null);
    } catch (err) {
      logger.error('Failed to fetch plans', {
        component: 'Plans',
        operation: 'fetchPlans',
        tenantId: selectedTenant,
      }, toError(err));
      setStatusMessage({ message: 'Failed to load plans.', variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to load plans'),
          () => fetchPlans()
        )
      );
    } finally {
      setLoading(false);
    }
  }, [selectedTenant]);

  useEffect(() => {
    fetchPlans();
  }, [fetchPlans]);

  const handleRebuild = async (planId: string) => {
    try {
      await apiClient.rebuildPlan(planId);
      showStatus('Plan rebuild initiated.', 'success');
      fetchPlans();
    } catch (err) {
      logger.error('Failed to rebuild plan', {
        component: 'Plans',
        operation: 'rebuildPlan',
        planId,
        tenantId: selectedTenant,
      }, toError(err));
      setStatusMessage({ message: 'Failed to rebuild plan.', variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to rebuild plan'),
          () => handleRebuild(planId)
        )
      );
    }
  };

  const handleExportManifest = async (planId: string) => {
    try {
      const blob = await apiClient.exportPlanManifest(planId);
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `plan-${planId}-manifest.json`;
      a.click();
      URL.revokeObjectURL(url);
      showStatus('Manifest downloaded.', 'success');
    } catch (err) {
      logger.error('Failed to export plan manifest', {
        component: 'Plans',
        operation: 'exportManifest',
        planId,
      }, toError(err));
      setStatusMessage({ message: 'Failed to export manifest.', variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to export manifest'),
          () => handleExportManifest(planId)
        )
      );
    }
  };

  const handleComparePlans = async () => {
    if (!selectedPlan1 || !selectedPlan2) {
      showStatus('Please select two plans to compare.', 'warning');
      return;
    }
    if (selectedPlan1 === selectedPlan2) {
      showStatus('Please select different plans.', 'warning');
      return;
    }
    try {
      const result = await apiClient.comparePlans(selectedPlan1, selectedPlan2);
      setCompareResult(result);
      showStatus('Plans compared successfully.', 'success');
    } catch (err) {
      logger.error('Failed to compare plans', {
        component: 'Plans',
        operation: 'comparePlans',
        planA: selectedPlan1,
        planB: selectedPlan2,
      }, toError(err));
      setStatusMessage({ message: 'Failed to compare plans.', variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to compare plans'),
          () => handleComparePlans()
        )
      );
    }
  };

  const handleDownloadDiff = () => {
    if (!compareResult) return;
    const blob = new Blob([JSON.stringify(compareResult, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `plan-compare-${compareResult.plan_1}-${compareResult.plan_2}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const handleBuildPlan = async () => {
    if (!manifestHash.trim()) return;
    try {
      await apiClient.buildPlan({ tenant_id: selectedTenant, manifest_hash_b3: manifestHash.trim() });
      showStatus('Plan build started.', 'success');
      setShowBuildModal(false);
      setManifestHash('');
      fetchPlans();
    } catch (err) {
      logger.error('Failed to start plan build', {
        component: 'Plans',
        operation: 'buildPlan',
        tenantId: selectedTenant,
      }, toError(err));
      setStatusMessage({ message: 'Failed to start plan build.', variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to start plan build'),
          () => handleBuildPlan()
        )
      );
    }
  };

  const handleRollback = async () => {
    try {
      await apiClient.rollback();
      showStatus('Rollback initiated.', 'success');
      setShowRollbackModal(false);
      fetchPlans();
    } catch (err) {
      logger.error('Failed to rollback plan state', {
        component: 'Plans',
        operation: 'rollback',
        tenantId: selectedTenant,
      }, toError(err));
      setStatusMessage({ message: 'Failed to rollback.', variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Failed to rollback'),
          () => handleRollback()
        )
      );
    }
  };

  if (loading) {
    return <div className="text-center p-8">Loading plans...</div>;
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

      <div className="flex justify-between items-start">
        <div>
          <h1 className="text-2xl font-bold">Execution Plans</h1>
          <p className="text-sm text-muted-foreground">
            Manage compiled plans and kernel configurations
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" onClick={() => setShowCompareModal(true)}>
            <GitCompare className="icon-standard mr-2" />
            Compare Plans
          </Button>
          <Button variant="destructive" onClick={() => setShowRollbackModal(true)}>
            <Undo2 className="icon-standard mr-2" />
            Rollback
          </Button>
          <Button onClick={() => setShowBuildModal(true)}>
            <Plus className="icon-standard mr-2" />
            Build Plan
          </Button>
        </div>
      </div>

      <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
        <CardHeader>
          <CardTitle>Plans</CardTitle>
        </CardHeader>
        <CardContent>
          <Table className="border-collapse w-full">
            <TableHeader>
              <TableRow>
                <TableHead className="p-4 border-b border-border w-12">
                  <Checkbox
                    checked={
                      plans.length === 0
                        ? false
                        : selectedPlans.length === plans.length
                          ? true
                          : selectedPlans.length > 0
                            ? 'indeterminate'
                            : false
                    }
                    onCheckedChange={(checked) => {
                      if (checked) {
                        setSelectedPlans(plans.map(p => p.id));
                      } else {
                        setSelectedPlans([]);
                      }
                    }}
                    aria-label="Select all plans"
                  />
                </TableHead>
                <TableHead>Plan ID</TableHead>
                <TableHead>CPID</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Created</TableHead>
                <TableHead>Metallib Hash</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {plans.map((plan) => (
                <TableRow key={plan.id}>
                  <TableCell className="p-4 border-b border-border">
                    <Checkbox
                      checked={selectedPlans.includes(plan.id)}
                      onCheckedChange={(checked) => {
                        if (checked) {
                          setSelectedPlans(prev => [...prev, plan.id]);
                        } else {
                          setSelectedPlans(prev => prev.filter(id => id !== plan.id));
                        }
                      }}
                      aria-label={`Select ${plan.id}`}
                    />
                  </TableCell>
                  <TableCell className="p-4 border-b border-border font-medium">{plan.id}</TableCell>
                  <TableCell className="p-4 border-b border-border">{plan.cpid}</TableCell>
                  <TableCell className="p-4 border-b border-border">
                    <Badge variant={plan.status === 'ready' ? 'default' : 'secondary'}>
                      {plan.status}
                    </Badge>
                  </TableCell>
                  <TableCell className="p-4 border-b border-border">{new Date(plan.created_at).toLocaleString()}</TableCell>
                  <TableCell className="p-4 border-b border-border font-mono text-xs">
                    {plan.metallib_hash?.substring(0, 16) || 'N/A'}
                  </TableCell>
                  <TableCell className="p-4 border-b border-border">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant="ghost" size="sm">
                          <MoreHorizontal className="h-4 w-4" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem onClick={() => handleRebuild(plan.id)}>
                          <RefreshCw className="mr-2 h-4 w-4" />
                          Rebuild
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => handleExportManifest(plan.id)}>
                          <Download className="mr-2 h-4 w-4" />
                          Export Manifest
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </TableCell>
                </TableRow>
              ))}
              {plans.length === 0 && (
                <TableRow>
                  <TableCell colSpan={6} className="p-4 border-b border-border text-center text-muted-foreground">
                    No plans available
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {/* Build Plan Modal */}
      <Dialog open={showBuildModal} onOpenChange={setShowBuildModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Build Plan</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <Label>Manifest Hash (B3)</Label>
              <input
                className="w-full border rounded px-3 py-2"
                placeholder="b3:..."
                value={manifestHash}
                onChange={(e) => setManifestHash(e.target.value)}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowBuildModal(false)}>Cancel</Button>
            <Button onClick={handleBuildPlan} disabled={!manifestHash.trim()}>Build</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Compare Plans Modal */}
      <Dialog open={showCompareModal} onOpenChange={setShowCompareModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Compare Plans</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div className="grid grid-cols-2 gap-2">
              <div>
                <Label>Plan A</Label>
                <select className="w-full border rounded px-2 py-1" value={selectedPlan1} onChange={(e) => setSelectedPlan1(e.target.value)}>
                  <option value="">Select</option>
                  {plans.map(p => <option key={p.id} value={p.id}>{p.id}</option>)}
                </select>
              </div>
              <div>
                <Label>Plan B</Label>
                <select className="w-full border rounded px-2 py-1" value={selectedPlan2} onChange={(e) => setSelectedPlan2(e.target.value)}>
                  <option value="">Select</option>
                  {plans.map(p => <option key={p.id} value={p.id}>{p.id}</option>)}
                </select>
              </div>
            </div>
            <div className="flex gap-2">
              <Button onClick={handleComparePlans} disabled={!selectedPlan1 || !selectedPlan2}>Run Compare</Button>
              {compareResult && <Button variant="outline" onClick={handleDownloadDiff}>Download Diff JSON</Button>}
            </div>
            {compareResult && (
              <div className="border rounded p-3 text-sm">
                <div>Metallib Changed: {compareResult.metallib_hash_changed ? 'Yes' : 'No'}</div>
                <div className="mt-2">
                  Differences:
                  <ul className="list-disc ml-5">
                    {compareResult.differences.map((d, i) => <li key={i}>{d}</li>)}
                  </ul>
                </div>
              </div>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowCompareModal(false)}>Close</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Compare Plans Modal */}
      <Dialog open={showCompareModal} onOpenChange={setShowCompareModal}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Compare Plans</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <Label>Plan 1</Label>
              <Select value={selectedPlan1} onValueChange={setSelectedPlan1}>
                <SelectTrigger>
                  <SelectValue placeholder="Select first plan" />
                </SelectTrigger>
                <SelectContent>
                  {plans.filter(plan => plan.id && plan.id !== '').map((plan) => (
                    <SelectItem key={plan.id} value={plan.id}>
                      {plan.id} ({plan.cpid})
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div>
              <Label>Plan 2</Label>
              <Select value={selectedPlan2} onValueChange={setSelectedPlan2}>
                <SelectTrigger>
                  <SelectValue placeholder="Select second plan" />
                </SelectTrigger>
                <SelectContent>
                  {plans.filter(plan => plan.id && plan.id !== '').map((plan) => (
                    <SelectItem key={plan.id} value={plan.id}>
                      {plan.id} ({plan.cpid})
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <Button onClick={handleComparePlans} disabled={!selectedPlan1 || !selectedPlan2}>
              Compare
            </Button>
            {compareResult && (
              <Alert>
                <AlertDescription>
                  <div className="space-y-2">
                    <p><strong>Comparison Results:</strong></p>
                    <p>Plan 1: {compareResult.plan_1}</p>
                    <p>Plan 2: {compareResult.plan_2}</p>
                    <p>Metallib Hash Changed: {compareResult.metallib_hash_changed ? 'Yes' : 'No'}</p>
                    {compareResult.differences.length > 0 && (
                      <div>
                        <p><strong>Differences:</strong></p>
                        <ul className="list-disc list-inside">
                          {compareResult.differences.map((diff, idx) => (
                            <li key={idx}>{diff}</li>
                          ))}
                        </ul>
                      </div>
                    )}
                    {compareResult.adapter_changes.added.length > 0 && (
                      <div>
                        <p><strong>Adapters Added:</strong></p>
                        <ul className="list-disc list-inside">
                          {compareResult.adapter_changes.added.map((adapter, idx) => (
                            <li key={idx}>{adapter}</li>
                          ))}
                        </ul>
                      </div>
                    )}
                    {compareResult.adapter_changes.removed.length > 0 && (
                      <div>
                        <p><strong>Adapters Removed:</strong></p>
                        <ul className="list-disc list-inside">
                          {compareResult.adapter_changes.removed.map((adapter, idx) => (
                            <li key={idx}>{adapter}</li>
                          ))}
                        </ul>
                      </div>
                    )}
                  </div>
                </AlertDescription>
              </Alert>
            )}
          </div>
        </DialogContent>
      </Dialog>

      {/* Rollback Modal */}
      <Dialog open={showRollbackModal} onOpenChange={setShowRollbackModal}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Confirm Rollback</DialogTitle>
          </DialogHeader>
          <Alert variant="destructive">
            <AlertDescription>
              This will rollback to the previous checkpoint. This action cannot be undone.
              Are you sure you want to proceed?
            </AlertDescription>
          </Alert>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowRollbackModal(false)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleRollback}>
              Confirm Rollback
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Bulk Action Bar */}
      <BulkActionBar
        selectedItems={selectedPlans}
        actions={bulkActions}
        onClearSelection={() => setSelectedPlans([])}
        itemName="plan"
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
    </div>
  );
}
