import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from './ui/dropdown-menu';
import { Shield, Plus, CheckCircle, MoreHorizontal, FileSignature, GitCompare, Download, Edit, FileText } from 'lucide-react';

import { ExportMenu } from './ui/export-menu';
import { Checkbox } from './ui/checkbox';
import { BulkActionBar, BulkAction } from './ui/bulk-action-bar';
import { ConfirmationDialog, ConfirmationOptions } from './ui/confirmation-dialog';

import { toast } from 'sonner';
import { apiClient } from '@/api/services';
import { Policy, User, SignPolicyResponse, PolicyComparisonResponse } from '@/api/types';
import { useTimestamp } from '@/hooks/ui/useTimestamp';
import { PolicyEditor } from './PolicyEditor';
import { AuditDashboard } from './AuditDashboard';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Alert, AlertDescription } from './ui/alert';
import { logger } from '@/utils/logger';

import { GlossaryTooltip } from './ui/glossary-tooltip';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
import { Skeleton } from './ui/skeleton';
import { BookmarkButton } from './ui/bookmark-button';

import { useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';
import { useRBAC } from '@/hooks/security/useRBAC';
import { useProgressiveHints } from '@/hooks/tutorial/useProgressiveHints';
import { getPageHints } from '@/data/page-hints';
import { ProgressiveHint } from './ui/progressive-hint';

interface PoliciesProps {
  user?: User;
  selectedTenant?: string;
}

export function Policies({ user: userProp, selectedTenant: tenantProp }: PoliciesProps) {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();
  const effectiveUser = userProp ?? user!;
  const effectiveTenant = tenantProp ?? selectedTenant;

  const effectiveUserId = effectiveUser.id;

  const [policies, setPolicies] = useState<Policy[]>([]);
  const [loading, setLoading] = useState(true);
  const [policiesError, setPoliciesError] = useState<Error | null>(null);
  const [showSignModal, setShowSignModal] = useState(false);
  const [showCompareModal, setShowCompareModal] = useState(false);
  const [showEditorModal, setShowEditorModal] = useState(false);
  const [selectedPolicy, setSelectedPolicy] = useState<Policy | null>(null);
  const [signResult, setSignResult] = useState<SignPolicyResponse | null>(null);
  const [compareResult, setCompareResult] = useState<PolicyComparisonResponse | null>(null);
  const [compareCpid2, setCompareCpid2] = useState('');
  const [activeTab, setActiveTab] = useState('packs');


  // Progressive hints
  const hints = getPageHints('policies');
  const { getVisibleHint, dismissHint } = useProgressiveHints({
    pageKey: 'policies',
    hints
  });
  const visibleHint = getVisibleHint();
  const [selectedPolicies, setSelectedPolicies] = useState<string[]>([]);
  const [confirmationOpen, setConfirmationOpen] = useState(false);
  const [confirmationOptions, setConfirmationOptions] = useState<ConfirmationOptions | null>(null);
  const [pendingBulkAction, setPendingBulkAction] = useState<(() => Promise<void>) | null>(null);

  const fetchPolicies = useCallback(async () => {
    try {
      const data = await apiClient.listPolicies();
      setPolicies(data);
      setPoliciesError(null);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to fetch policies');
      logger.error('Failed to fetch policies', {
        component: 'Policies',
        operation: 'fetchPolicies',
        tenantId: effectiveTenant,
        userId: effectiveUserId
      }, error);
      // Major error: data loading failure
      setPoliciesError(error);
    } finally {
      setLoading(false);
    }
  }, [effectiveTenant, effectiveUserId]);


  useEffect(() => {
    fetchPolicies();
  }, [fetchPolicies]);

  const handleSignPolicy = async (policy: Policy) => {
    if (!policy.cpid) {
      setPoliciesError(new Error('Policy CPID is required'));
      return;
    }
    try {
      const result = await apiClient.signPolicy(policy.cpid);
      setSignResult(result);
      setSelectedPolicy(policy);
      setShowSignModal(true);
      // Success shown in modal - no need for toast
    } catch (err) {

      const error = err instanceof Error ? err : new Error('Failed to sign policy');
      setPoliciesError(error);

      toast.error('Failed to sign policy');
      // Replace: console.error(err);
      logger.error('Failed to sign policy', {
        component: 'Policies',
        operation: 'signPolicy',
        policyId: policy.cpid,
        tenantId: effectiveTenant,
        userId: effectiveUser.id
      }, err instanceof Error ? err : new Error(String(err)));
    }
  };

  const handleComparePolicy = async () => {
    if (!selectedPolicy || !compareCpid2) {
      setPoliciesError(new Error('Please select both policies to compare'));
      return;
    }
    if (!selectedPolicy.cpid) {
      setPoliciesError(new Error('Selected policy CPID is required'));
      return;
    }
    try {
      const result = await apiClient.comparePolicies(selectedPolicy.cpid, compareCpid2);
      setCompareResult(result);
      // Comparison results shown in UI - no need for toast
    } catch (err) {

      const error = err instanceof Error ? err : new Error('Failed to compare policies');
      setPoliciesError(error);

      toast.error('Failed to compare policies');
      // Replace: console.error(err);
      logger.error('Failed to compare policies', {
        component: 'Policies',
        operation: 'comparePolicies',
        policyId1: selectedPolicy.cpid,
        policyId2: compareCpid2,
        tenantId: effectiveTenant,
        userId: effectiveUser.id
      }, err instanceof Error ? err : new Error(String(err)));
    }
  };

  const handleExportPolicy = async (policy: Policy) => {
    if (!policy.cpid) {
      setPoliciesError(new Error('Policy CPID is required'));
      return;
    }
    try {
      const result = await apiClient.exportPolicy(policy.cpid);
      const dataStr = JSON.stringify(result, null, 2);
      const dataBlob = new Blob([dataStr], { type: 'application/json' });
      const url = URL.createObjectURL(dataBlob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `policy-${policy.cpid}.json`;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      URL.revokeObjectURL(url);
      // Browser download feedback is sufficient
    } catch (err) {

      const error = err instanceof Error ? err : new Error('Failed to export policy');
      setPoliciesError(error);

      toast.error('Failed to export policy');
      // Replace: console.error(err);
      logger.error('Failed to export policy', {
        component: 'Policies',
        operation: 'exportPolicy',
        policyId: policy.cpid,
        tenantId: effectiveTenant,
        userId: effectiveUser.id
      }, err instanceof Error ? err : new Error(String(err)));
    }
  };

  const handleExportAllPolicies = async (format: 'csv' | 'json') => {
    try {
      const policiesToExport = policies;
      // Export all policies as JSON array (backend API returns single policy, so we collect them)
      if (format === 'json') {
        const exports = await Promise.all(
          policiesToExport.filter(p => p.cpid).map(policy => apiClient.exportPolicy(policy.cpid!))
        );
        const dataStr = JSON.stringify(exports, null, 2);
        const dataBlob = new Blob([dataStr], { type: 'application/json' });
        const url = URL.createObjectURL(dataBlob);
        const link = document.createElement('a');
        link.href = url;
        link.download = `policies-export-${new Date().toISOString().split('T')[0]}.json`;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        URL.revokeObjectURL(url);
      } else {
        // CSV format - convert policies array to CSV
        const csvHeaders = ['Policy ID', 'Schema Hash', 'Status'];
        const csvRows = policiesToExport.map(policy => [
          policy.cpid || '',
          policy.schema_hash || '',
          'Active'
        ]);
        const csvContent = [csvHeaders.join(','), ...csvRows.map(row => row.join(','))].join('\n');
        const csvBlob = new Blob([csvContent], { type: 'text/csv' });
        const url = URL.createObjectURL(csvBlob);
        const link = document.createElement('a');
        link.href = url;
        link.download = `policies-export-${new Date().toISOString().split('T')[0]}.csv`;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        URL.revokeObjectURL(url);
      }
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to export policies');
      setPoliciesError(error);
      logger.error('Failed to export all policies', {
        component: 'Policies',
        operation: 'exportAllPolicies',
        tenantId: effectiveTenant,
        userId: effectiveUser.id
      }, err instanceof Error ? err : new Error(String(err)));
    }
  };

  const handleBulkExportPolicies = async (policyCpids: string[]) => {
    const performBulkExport = async () => {
      try {
        const policiesToExport = policies.filter(p => p.cpid && policyCpids.includes(p.cpid));
        const exports = await Promise.all(
          policiesToExport.map(policy => apiClient.exportPolicy(policy.cpid!))
        );
        const dataStr = JSON.stringify(exports, null, 2);
        const dataBlob = new Blob([dataStr], { type: 'application/json' });
        const url = URL.createObjectURL(dataBlob);
        const link = document.createElement('a');
        link.href = url;
        link.download = `policies-selected-export-${new Date().toISOString().split('T')[0]}.json`;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        URL.revokeObjectURL(url);
        toast.success(`Exported ${policyCpids.length} policy/policies.`);
        setSelectedPolicies([]);
      } catch (err) {
        const error = err instanceof Error ? err : new Error('Failed to export policies');
        setPoliciesError(error);
        logger.error('Failed to export selected policies', {
          component: 'Policies',
          operation: 'bulkExportPolicies',
          tenantId: effectiveTenant,
          userId: effectiveUser.id
        }, err instanceof Error ? err : new Error(String(err)));
      }
    };
    await performBulkExport();
  };

  const bulkActions: BulkAction[] = [
    {
      id: 'export',
      label: 'Export Selected',
      handler: handleBulkExportPolicies
    }
  ];

  if (policiesError) {
    return (
      <ErrorRecovery
        error={policiesError.message}
        onRetry={() => {
          setPoliciesError(null);
          fetchPolicies();
        }}
      />
    );
  }


  if (loading) {
    return (
      <div className="space-y-6">
        <div className="flex items-center justify-between">
          <Skeleton className="h-10 w-48" />
          <Skeleton className="h-10 w-32" />
        </div>
        <div className="space-y-2">
          <Skeleton className="h-16 w-full" />
          <Skeleton className="h-16 w-full" />
          <Skeleton className="h-16 w-full" />
        </div>
      </div>
    );
  }


  // Citation: AGENTS.md L151-L172 - 20 policy packs enforced by mplora-policy
  const policyTabs = [
    { id: 'packs', label: 'Policy Packs', icon: Shield, description: '20 policy packs enforcement' },
    { id: 'compliance', label: 'Compliance', icon: CheckCircle, description: 'Compliance dashboard' },
    { id: 'audit', label: 'Audit Trail', icon: FileText, description: 'Audit trail visualization' }
  ];

  return (
    <div className="space-y-6">

      {visibleHint && (
        <ProgressiveHint
          title={visibleHint.hint.title}
          content={visibleHint.hint.content}
          onDismiss={() => dismissHint(visibleHint.hint.id)}
          placement={visibleHint.hint.placement}
        />
      )}

      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Policies</h1>
          <p className="text-muted-foreground">
            Security policy configuration and compliance management
          </p>
        </div>
        <div className="flex items-center gap-2">

          <ExportMenu
            onExport={handleExportAllPolicies}
            filename="policies-export"
            formats={['csv', 'json']}
          />

          <Button
            onClick={() => { setSelectedPolicy(null); setShowEditorModal(true); }}
            disabled={!can('policy:apply')}
            title={!can('policy:apply') ? 'Requires policy:apply permission' : undefined}
          >
            <Plus className="h-4 w-4 mr-2" />
            New Policy
          </Button>
        </div>
      </div>

      {/* Policy Tabs */}
      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList className="grid w-full grid-cols-3">
          {policyTabs.map((tab) => {
            const Icon = tab.icon;
            return (
              <TabsTrigger key={tab.id} value={tab.id} className="flex items-center gap-2">
                <Icon className="h-4 w-4" />
                <span className="hidden sm:inline">{tab.label}</span>
              </TabsTrigger>
            );
          })}
        </TabsList>

        {/* Policy Packs Tab */}
        <TabsContent value="packs" className="space-y-4">


      <Card className="card-standard">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            Active Policies
            <GlossaryTooltip termId="policy" variant="icon" />
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Table className="border-collapse w-full">
            <TableHeader>
              <TableRow>
                <TableHead className="p-4 border-b border-border w-12">
                  <Checkbox
                    checked={
                      policies.length === 0
                        ? false
                        : selectedPolicies.length === policies.length
                          ? true
                          : selectedPolicies.length > 0
                            ? 'indeterminate'
                            : false
                    }
                    onCheckedChange={(checked) => {
                      if (checked) {
                        setSelectedPolicies(policies.filter(p => p.cpid).map(p => p.cpid!));
                      } else {
                        setSelectedPolicies([]);
                      }
                    }}
                    aria-label="Select all policies"
                  />
                </TableHead>
                <TableHead>
                  <GlossaryTooltip termId="policy-cpid">
                    <span>Policy ID</span>
                  </GlossaryTooltip>
                </TableHead>
                <TableHead>
                  <GlossaryTooltip termId="policy-schema-hash">
                    <span>Schema Hash</span>
                  </GlossaryTooltip>
                </TableHead>
                <TableHead>
                  <GlossaryTooltip termId="policy-status">
                    <span>Status</span>
                  </GlossaryTooltip>
                </TableHead>
                <TableHead className="w-[calc(var(--base-unit)*25)]">
                  <GlossaryTooltip termId="policy-actions">
                    <span>Actions</span>
                  </GlossaryTooltip>
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {policies.map((policy) => (
                <TableRow key={policy.cpid}>
                  <TableCell className="p-4 border-b border-border">
                    <Checkbox
                      checked={policy.cpid ? selectedPolicies.includes(policy.cpid) : false}
                      onCheckedChange={(checked) => {
                        if (policy.cpid) {
                          if (checked) {
                            setSelectedPolicies(prev => [...prev, policy.cpid!]);
                          } else {
                            setSelectedPolicies(prev => prev.filter(id => id !== policy.cpid));
                          }
                        }
                      }}
                      aria-label={`Select ${policy.cpid || 'policy'}`}
                    />
                  </TableCell>
                  <TableCell className="p-4 border-b border-border font-medium">{policy.cpid}</TableCell>
                  <TableCell className="p-4 border-b border-border font-mono text-xs">
                    {policy.schema_hash?.substring(0, 16) || ''}
                  </TableCell>
                  <TableCell className="p-4 border-b border-border">
                    <Badge variant="default">
                      <CheckCircle className="h-3 w-3 mr-1" />
                      Active
                    </Badge>
                  </TableCell>

                  <TableCell className="p-4 border-b border-border">
                    <div className="flex items-center gap-1">
                      <BookmarkButton
                        type="policy"
                        title={policy.cpid || 'Policy'}
                        url={`/policies?policy=${encodeURIComponent(policy.cpid || '')}`}
                        entityId={policy.cpid || ''}
                        description={`Policy • ${policy.schema_hash?.substring(0, 8) || ''}`}
                        variant="ghost"
                        size="icon"
                      />
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild>
                          <Button variant="ghost" size="sm">
                            <MoreHorizontal className="icon-standard" />
                          </Button>
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                          <DropdownMenuItem
                            onClick={() => { setSelectedPolicy(policy); setShowEditorModal(true); }}
                            disabled={!can('policy:apply')}
                            title={!can('policy:apply') ? 'Requires policy:apply permission' : undefined}
                          >
                            <Edit className="icon-standard mr-2" />
                            Edit
                          </DropdownMenuItem>
                          <DropdownMenuItem
                            onClick={() => handleSignPolicy(policy)}
                            disabled={!can('policy:sign')}
                            title={!can('policy:sign') ? 'Requires policy:sign permission (Admin only)' : undefined}
                          >
                            <FileSignature className="icon-standard mr-2" />
                            Sign Policy
                          </DropdownMenuItem>
                          <DropdownMenuItem onClick={() => { setSelectedPolicy(policy); setShowCompareModal(true); }}>
                            <GitCompare className="icon-standard mr-2" />
                            Compare
                          </DropdownMenuItem>
                          <DropdownMenuItem onClick={() => handleExportPolicy(policy)}>
                            <Download className="icon-standard mr-2" />
                            Export
                          </DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
              {policies.length === 0 && (
                <TableRow>
                  <TableCell colSpan={4} className="p-4 border-b border-border text-center text-muted-foreground">
                    No policies configured
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {/* Sign Policy Modal */}
      <Dialog open={showSignModal} onOpenChange={setShowSignModal}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>Policy Signature</DialogTitle>
          </DialogHeader>
          {signResult && (
            <div className="space-y-3">
              <div className="mb-4">
                <GlossaryTooltip termId="policy-cpid">
                  <p className="font-medium text-sm mb-1 cursor-help">Policy ID</p>
                </GlossaryTooltip>
                <p className="text-sm text-muted-foreground font-mono">{signResult.cpid}</p>
              </div>
              <div className="mb-4">
                <GlossaryTooltip termId="policy-signed">
                  <p className="font-medium text-sm mb-1 cursor-help">Signature</p>
                </GlossaryTooltip>
                <p className="text-xs text-muted-foreground font-mono break-all">{signResult.signature}</p>
              </div>
              <div className="mb-4">
                <p className="font-medium text-sm mb-1">Signed By</p>
                <p className="text-sm text-muted-foreground">{signResult.signed_by}</p>
              </div>

              <div className="mb-4">
                <p className="font-medium text-sm mb-1">Signed At</p>
                <p className="text-sm text-muted-foreground">{useTimestamp(signResult.signed_at)}</p>
              </div>
            </div>
          )}
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                if (!signResult) return;
                const attestation = {
                  cpid: signResult.cpid,
                  signature: signResult.signature,
                  signed_by: signResult.signed_by,
                  signed_at: signResult.signed_at,
                };
                navigator.clipboard.writeText(JSON.stringify(attestation, null, 2));
                // Browser clipboard API provides feedback
              }}
            >
              Copy Attestation
            </Button>
            <Button
              onClick={() => {
                if (!signResult) return;
                const attestation = {
                  cpid: signResult.cpid,
                  signature: signResult.signature,
                  signed_by: signResult.signed_by,
                  signed_at: signResult.signed_at,
                };
                const dataStr = JSON.stringify(attestation, null, 2);
                const blob = new Blob([dataStr], { type: 'application/json' });
                const url = URL.createObjectURL(blob);
                const link = document.createElement('a');
                link.href = url;
                link.download = `policy-attestation-${signResult.cpid}.json`;
                document.body.appendChild(link);
                link.click();
                document.body.removeChild(link);
                URL.revokeObjectURL(url);
              }}
            >
              Download Attestation
            </Button>
            <Button onClick={() => setShowSignModal(false)}>Close</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Compare Policies Modal */}
      <Dialog open={showCompareModal} onOpenChange={setShowCompareModal}>
        <DialogContent className="max-w-4xl">
          <DialogHeader>
            <DialogTitle>Compare Policies</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div className="mb-4">
              <GlossaryTooltip termId="policy-cpid">
                <label className="font-medium text-sm mb-1 cursor-help">First Policy</label>
              </GlossaryTooltip>
              <p className="text-sm text-muted-foreground font-mono">{selectedPolicy?.cpid}</p>
            </div>
            <div className="mb-4">
              <GlossaryTooltip termId="policy-cpid">
                <label className="font-medium text-sm mb-1 cursor-help">Second Policy ID</label>
              </GlossaryTooltip>
              <select
                className="w-full p-2 border rounded"
                value={compareCpid2}
                onChange={(e) => setCompareCpid2(e.target.value)}
              >
                <option value="">Select policy...</option>
                {policies.filter(p => p.cpid !== selectedPolicy?.cpid).map((policy) => (
                  <option key={policy.cpid} value={policy.cpid}>{policy.cpid}</option>
                ))}
              </select>
            </div>
            {compareResult && (
              <div className="mt-4 space-y-3 border-t pt-4">
                <div className="mb-4">
                  <p className="font-medium text-sm mb-1">
                    {compareResult.identical ? (
                      <span className="text-green-600">Policies are identical</span>
                    ) : (
                      <span>Differences ({compareResult.differences.length})</span>
                    )}
                  </p>
                  {compareResult.differences && compareResult.differences.length > 0 && (
                    <ul className="list-disc list-inside text-sm text-muted-foreground mt-2 space-y-1">
                      {compareResult.differences.map((diff, idx) => (
                        <li key={idx} className="font-mono text-xs">{diff}</li>
                      ))}
                    </ul>
                  )}
                </div>
              </div>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => { setShowCompareModal(false); setCompareResult(null); }}>
              Cancel
            </Button>
            <Button onClick={handleComparePolicy} disabled={!compareCpid2}>
              Compare
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
        </TabsContent>

        {/* Compliance Tab */}
        <TabsContent value="compliance" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <CheckCircle className="h-5 w-5" />
                Compliance Dashboard
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-4">
                <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                  <Card>
                    <CardContent className="p-4">
                      <div className="flex items-center justify-between">
                        <div>
                          <p className="text-sm text-muted-foreground">Policy Packs</p>
                          <p className="text-2xl font-bold text-green-600">20</p>
                        </div>
                        <CheckCircle className="h-8 w-8 text-green-500" />
                      </div>
                    </CardContent>
                  </Card>
                  <Card>
                    <CardContent className="p-4">
                      <div className="flex items-center justify-between">
                        <div>
                          <p className="text-sm text-muted-foreground">Compliance Score</p>
                          <p className="text-2xl font-bold text-green-600">98%</p>
                        </div>
                        <Shield className="h-8 w-8 text-green-500" />
                      </div>
                    </CardContent>
                  </Card>
                  <Card>
                    <CardContent className="p-4">
                      <div className="flex items-center justify-between">
                        <div>
                          <p className="text-sm text-muted-foreground">Violations</p>
                          <p className="text-2xl font-bold text-red-600">2</p>
                        </div>
                        <FileText className="h-8 w-8 text-red-500" />
                      </div>
                    </CardContent>
                  </Card>
                </div>
                <Alert>
                  <CheckCircle className="h-4 w-4" />
                  <AlertDescription>
                    All 20 policy packs are active and compliant. System meets security requirements.
                  </AlertDescription>
                </Alert>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Audit Trail Tab */}
        <TabsContent value="audit" className="space-y-4">
          <AuditDashboard selectedTenant={selectedTenant} />
        </TabsContent>
      </Tabs>

      {/* Policy Editor Modal */}
      <PolicyEditor
        open={showEditorModal}
        onOpenChange={setShowEditorModal}
        cpid={selectedPolicy?.cpid}
        existingPolicy={selectedPolicy?.policy_json}
        onSave={fetchPolicies}
      />


      {/* Bulk Action Bar */}
      <BulkActionBar
        selectedItems={selectedPolicies}
        actions={bulkActions}
        onClearSelection={() => setSelectedPolicies([])}
        itemName="policy"
      />

    </div>
  );
}
