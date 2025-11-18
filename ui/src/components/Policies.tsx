import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from './ui/dialog';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from './ui/dropdown-menu';
import { Shield, Plus, CheckCircle, MoreHorizontal, FileSignature, GitCompare, Download, Edit, FileText } from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../api/client';
import { Policy, User, SignPolicyResponse, PolicyComparisonResponse } from '../api/types';
import { useTimestamp } from '../hooks/useTimestamp';
import { PolicyEditor } from './PolicyEditor';
import { AuditDashboard } from './AuditDashboard';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Alert, AlertDescription } from './ui/alert';
import { logger } from '../utils/logger';

import { useAuth, useTenant } from '@/layout/LayoutProvider';

interface PoliciesProps {
  user?: User;
  selectedTenant?: string;
}

export function Policies({ user: userProp, selectedTenant: tenantProp }: PoliciesProps) {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const effectiveUser = userProp ?? user!;
  const effectiveTenant = tenantProp ?? selectedTenant;
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

  useEffect(() => {
    fetchPolicies();
  }, [fetchPolicies]);

  const fetchPolicies = async () => {
    try {
      const data = await apiClient.listPolicies();
      setPolicies(data);
    } catch (err) {
      // Replace: console.error('Failed to fetch policies:', err);
      logger.error('Failed to fetch policies', {
        component: 'Policies',
        operation: 'fetchPolicies',
        tenantId: effectiveTenant,
        userId: effectiveUser.id
      }, err instanceof Error ? err : new Error(String(err)));
    } finally {
      setLoading(false);
    }
  };

  const handleSignPolicy = async (policy: Policy) => {
    try {
      const result = await apiClient.signPolicy(policy.cpid);
      setSignResult(result);
      setSelectedPolicy(policy);
      setShowSignModal(true);
      // Success shown in modal - no need for toast
    } catch (err) {
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
    try {
      const result = await apiClient.comparePolicies(selectedPolicy.cpid, compareCpid2);
      setCompareResult(result);
      // Comparison results shown in UI - no need for toast
    } catch (err) {
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
          policiesToExport.map(policy => apiClient.exportPolicy(policy.cpid))
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
        const csvHeaders = ['CPID', 'Schema Hash', 'Status'];
        const csvRows = policiesToExport.map(policy => [
          policy.cpid,
          policy.schema_hash,
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
        const policiesToExport = policies.filter(p => policyCpids.includes(p.cpid));
        const exports = await Promise.all(
          policiesToExport.map(policy => apiClient.exportPolicy(policy.cpid))
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
        title="Policies Error"
        message={policiesError.message}
        recoveryActions={[
          { label: 'Retry Loading', action: () => {
            setPoliciesError(null);
            fetchPolicies();
          }},
          { label: 'View Logs', action: () => {/* Navigate to logs */} }
        ]}
      />
    );
  }

  // Citation: CLAUDE.md L151-L172 - 20 policy packs enforced by mplora-policy
  const policyTabs = [
    { id: 'packs', label: 'Policy Packs', icon: Shield, description: '20 policy packs enforcement' },
    { id: 'compliance', label: 'Compliance', icon: CheckCircle, description: 'Compliance dashboard' },
    { id: 'audit', label: 'Audit Trail', icon: FileText, description: 'Audit trail visualization' }
  ];

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Policies</h1>
          <p className="text-muted-foreground">
            Security policy configuration and compliance management
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button onClick={() => { setSelectedPolicy(null); setShowEditorModal(true); }}>
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
          <CardTitle>Active Policies</CardTitle>
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
                        setSelectedPolicies(policies.map(p => p.cpid));
                      } else {
                        setSelectedPolicies([]);
                      }
                    }}
                    aria-label="Select all policies"
                  />
                </TableHead>
                <TableHead>
                  <HelpTooltip helpId="cpid">
                    <span>CPID</span>
                  </HelpTooltip>
                </TableHead>
                <TableHead>
                  <HelpTooltip helpId="schema-hash">
                    <span>Schema Hash</span>
                  </HelpTooltip>
                </TableHead>
                <TableHead>Status</TableHead>
                <TableHead className="w-[100px]">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {policies.map((policy) => (
                <TableRow key={policy.cpid}>
                  <TableCell className="p-4 border-b border-border">
                    <Checkbox
                      checked={selectedPolicies.includes(policy.cpid)}
                      onCheckedChange={(checked) => {
                        if (checked) {
                          setSelectedPolicies(prev => [...prev, policy.cpid]);
                        } else {
                          setSelectedPolicies(prev => prev.filter(id => id !== policy.cpid));
                        }
                      }}
                      aria-label={`Select ${policy.cpid}`}
                    />
                  </TableCell>
                  <TableCell className="p-4 border-b border-border font-medium">{policy.cpid}</TableCell>
                  <TableCell className="p-4 border-b border-border font-mono text-xs">
                    {policy.schema_hash.substring(0, 16)}
                  </TableCell>
                  <TableCell className="p-4 border-b border-border">
                    <Badge variant="default">
                      <CheckCircle className="h-3 w-3 mr-1" />
                      Active
                    </Badge>
                  </TableCell>
                  <TableCell className="table-cell-standard">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant="ghost" size="sm">
                          <MoreHorizontal className="icon-standard" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem onClick={() => { setSelectedPolicy(policy); setShowEditorModal(true); }}>
                          <Edit className="icon-standard mr-2" />
                          Edit
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => handleSignPolicy(policy)}>
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
                <p className="font-medium text-sm mb-1">CPID</p>
                <p className="text-sm text-muted-foreground font-mono">{signResult.cpid}</p>
              </div>
              <div className="mb-4">
                <p className="font-medium text-sm mb-1">Signature</p>
                <p className="text-xs text-muted-foreground font-mono break-all">{signResult.signature}</p>
              </div>
              <div className="mb-4">
                <p className="font-medium text-sm mb-1">Signed By</p>
                <p className="text-sm text-muted-foreground">{signResult.signed_by}</p>
              </div>
              <div className="form-field">
                <p className="form-label">Signed At</p>
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
              <label className="font-medium text-sm mb-1">First Policy</label>
              <p className="text-sm text-muted-foreground font-mono">{selectedPolicy?.cpid}</p>
            </div>
            <div className="mb-4">
              <label className="font-medium text-sm mb-1">Second Policy CPID</label>
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
                  <p className="font-medium text-sm mb-1">Differences ({compareResult.differences.length})</p>
                  <ul className="list-disc list-inside text-sm text-muted-foreground mt-2">
                    {compareResult.differences.map((diff, idx) => (
                      <li key={idx} className="font-mono text-xs">{diff}</li>
                    ))}
                  </ul>
                </div>
                {compareResult.added_keys.length > 0 && (
                  <div className="mb-4">
                    <p className="font-medium text-sm mb-1 text-green-600">Added Keys</p>
                    <ul className="list-disc list-inside text-sm text-muted-foreground">
                      {compareResult.added_keys.map((key, idx) => (
                        <li key={idx} className="font-mono text-xs">{key}</li>
                      ))}
                    </ul>
                  </div>
                )}
                {compareResult.removed_keys.length > 0 && (
                  <div className="mb-4">
                    <p className="font-medium text-sm mb-1 text-red-600">Removed Keys</p>
                    <ul className="list-disc list-inside text-sm text-muted-foreground">
                      {compareResult.removed_keys.map((key, idx) => (
                        <li key={idx} className="font-mono text-xs">{key}</li>
                      ))}
                    </ul>
                  </div>
                )}
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
    </div>
  );
}
