// Security Policies Page - Policy management with apply, sign, compare functionality
// Citation: CLAUDE.md L573-L608 "Policies endpoints"

import React, { useState, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import {
  Shield,
  Plus,
  RefreshCw,
  FileSignature,
  GitCompare,
  CheckCircle,
  AlertTriangle,
  FileText,
  Download,
} from 'lucide-react';
import { toast } from 'sonner';

import { PolicyTable } from './PolicyTable';
import { PolicyDetail } from './PolicyDetail';
import { usePolicies, usePolicyMutations } from '@/hooks/usePolicies';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { PageHeader } from '@/components/ui/page-header';
import { Skeleton } from '@/components/ui/skeleton';
import type { Policy, PolicyComparisonResponse } from '@/api/types';

export default function PoliciesPage() {
  const { can } = useRBAC();
  const [activeTab, setActiveTab] = useState('list');
  const [selectedPolicy, setSelectedPolicy] = useState<Policy | null>(null);

  // Compare modal state
  const [showCompareModal, setShowCompareModal] = useState(false);
  const [compareCpid1, setCompareCpid1] = useState('');
  const [compareCpid2, setCompareCpid2] = useState('');
  const [compareResult, setCompareResult] = useState<PolicyComparisonResponse | null>(null);

  // Apply modal state
  const [showApplyModal, setShowApplyModal] = useState(false);
  const [applyContent, setApplyContent] = useState('');
  const [applyCpid, setApplyCpid] = useState('');

  // Queries and mutations
  const { policies, isLoading, error, refetch } = usePolicies();
  const { signPolicy, comparePolicy, applyPolicy, exportPolicy, isSigningPolicy, isApplyingPolicy, isComparingPolicy } = usePolicyMutations();

  // RBAC: Check if user has policy:view permission
  if (!can('policy:view')) {
    return (
      <div className="container mx-auto p-6">
        <PageHeader
          title="Security Policies"
          description="Manage and enforce security policies"
        />
        <ErrorRecovery
          error="You do not have permission to view policies. This page requires the policy:view permission."
          onRetry={() => window.location.reload()}
        />
      </div>
    );
  }

  const handleSign = useCallback(async (policy: Policy) => {
    if (!can('policy:sign')) {
      toast.error('You do not have permission to sign policies');
      return;
    }
    try {
      const result = await signPolicy(policy.cpid || policy.id);
      toast.success(`Policy signed successfully. Signature: ${result.signature?.substring(0, 16)}...`);
    } catch (err) {
      toast.error('Failed to sign policy');
    }
  }, [can, signPolicy]);

  const handleCompare = useCallback(async () => {
    if (!compareCpid1 || !compareCpid2) {
      toast.error('Please enter both policy CPIDs to compare');
      return;
    }
    try {
      const result = await comparePolicy({ cpid1: compareCpid1, cpid2: compareCpid2 });
      setCompareResult(result);
      toast.success('Policy comparison completed');
    } catch (err) {
      toast.error('Failed to compare policies');
    }
  }, [compareCpid1, compareCpid2, comparePolicy]);

  const handleApply = useCallback(async () => {
    if (!can('policy:apply')) {
      toast.error('You do not have permission to apply policies');
      return;
    }
    if (!applyCpid || !applyContent) {
      toast.error('Please enter both CPID and policy content');
      return;
    }
    try {
      await applyPolicy({ cpid: applyCpid, content: applyContent });
      toast.success('Policy applied successfully');
      setShowApplyModal(false);
      setApplyCpid('');
      setApplyContent('');
      refetch();
    } catch (err) {
      toast.error('Failed to apply policy');
    }
  }, [can, applyCpid, applyContent, applyPolicy, refetch]);

  const handleExport = useCallback(async (policy: Policy) => {
    try {
      const result = await exportPolicy(policy.cpid || policy.id);
      const dataStr = JSON.stringify(result, null, 2);
      const dataBlob = new Blob([dataStr], { type: 'application/json' });
      const url = URL.createObjectURL(dataBlob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `policy-${policy.cpid || policy.id}.json`;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      URL.revokeObjectURL(url);
      toast.success('Policy exported');
    } catch (err) {
      toast.error('Failed to export policy');
    }
  }, [exportPolicy]);

  const handleSelectPolicy = useCallback((policy: Policy) => {
    setSelectedPolicy(policy);
    setActiveTab('detail');
  }, []);

  const openCompareWithPolicy = useCallback((policy: Policy) => {
    setCompareCpid1(policy.cpid || policy.id);
    setCompareCpid2('');
    setCompareResult(null);
    setShowCompareModal(true);
  }, []);

  if (error) {
    return (
      <div className="container mx-auto p-6">
        <PageHeader
          title="Security Policies"
          description="Manage and enforce security policies"
        />
        <ErrorRecovery
          error={error.message}
          onRetry={refetch}
        />
      </div>
    );
  }

  return (
    <div className="container mx-auto p-6 space-y-6">
      <PageHeader
        title="Security Policies"
        description="Manage, sign, and enforce security policies across your organization"
      />

      {/* Summary Cards */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Total Policies
            </CardTitle>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <Skeleton className="h-8 w-16" />
            ) : (
              <div className="flex items-center gap-2">
                <Shield className="h-5 w-5 text-primary" />
                <span className="text-2xl font-bold">{policies?.length || 0}</span>
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Active Policies
            </CardTitle>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <Skeleton className="h-8 w-16" />
            ) : (
              <div className="flex items-center gap-2">
                <CheckCircle className="h-5 w-5 text-green-500" />
                <span className="text-2xl font-bold">
                  {policies?.filter(p => p.status === 'active' || p.enabled).length || 0}
                </span>
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Draft Policies
            </CardTitle>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <Skeleton className="h-8 w-16" />
            ) : (
              <div className="flex items-center gap-2">
                <FileText className="h-5 w-5 text-yellow-500" />
                <span className="text-2xl font-bold">
                  {policies?.filter(p => p.status === 'draft').length || 0}
                </span>
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Signed Policies
            </CardTitle>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <Skeleton className="h-8 w-16" />
            ) : (
              <div className="flex items-center gap-2">
                <FileSignature className="h-5 w-5 text-blue-500" />
                <span className="text-2xl font-bold">
                  {policies?.filter(p => p.signature).length || 0}
                </span>
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Action Buttons */}
      <div className="flex flex-wrap gap-2">
        <Button onClick={() => refetch()} variant="outline" size="sm">
          <RefreshCw className="h-4 w-4 mr-2" />
          Refresh
        </Button>
        {can('policy:apply') && (
          <Button onClick={() => setShowApplyModal(true)} size="sm">
            <Plus className="h-4 w-4 mr-2" />
            Apply Policy
          </Button>
        )}
        <Button
          onClick={() => {
            setCompareCpid1('');
            setCompareCpid2('');
            setCompareResult(null);
            setShowCompareModal(true);
          }}
          variant="outline"
          size="sm"
        >
          <GitCompare className="h-4 w-4 mr-2" />
          Compare Policies
        </Button>
      </div>

      {/* Main Content */}
      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList>
          <TabsTrigger value="list">Policy List</TabsTrigger>
          <TabsTrigger value="detail" disabled={!selectedPolicy}>
            Policy Detail
          </TabsTrigger>
        </TabsList>

        <TabsContent value="list" className="mt-4">
          {isLoading ? (
            <Card>
              <CardContent className="p-6">
                <div className="space-y-4">
                  {[1, 2, 3].map(i => (
                    <Skeleton key={i} className="h-12 w-full" />
                  ))}
                </div>
              </CardContent>
            </Card>
          ) : (
            <PolicyTable
              policies={policies || []}
              onSelect={handleSelectPolicy}
              onSign={handleSign}
              onCompare={openCompareWithPolicy}
              onExport={handleExport}
              canSign={can('policy:sign')}
              isSigningPolicy={isSigningPolicy}
            />
          )}
        </TabsContent>

        <TabsContent value="detail" className="mt-4">
          {selectedPolicy ? (
            <PolicyDetail
              policy={selectedPolicy}
              onSign={handleSign}
              onExport={handleExport}
              onCompare={openCompareWithPolicy}
              onBack={() => {
                setSelectedPolicy(null);
                setActiveTab('list');
              }}
              canSign={can('policy:sign')}
              canValidate={can('policy:validate')}
            />
          ) : (
            <Card>
              <CardContent className="p-6 text-center text-muted-foreground">
                Select a policy from the list to view details
              </CardContent>
            </Card>
          )}
        </TabsContent>
      </Tabs>

      {/* Apply Policy Modal */}
      <Dialog open={showApplyModal} onOpenChange={setShowApplyModal}>
        <DialogContent className="sm:max-w-[600px]">
          <DialogHeader>
            <DialogTitle>Apply Policy</DialogTitle>
            <DialogDescription>
              Enter the policy CPID and content to apply a new or updated policy.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="cpid">Policy CPID</Label>
              <Input
                id="cpid"
                value={applyCpid}
                onChange={(e) => setApplyCpid(e.target.value)}
                placeholder="e.g., policy-egress-v1"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="content">Policy Content (JSON)</Label>
              <Textarea
                id="content"
                value={applyContent}
                onChange={(e) => setApplyContent(e.target.value)}
                placeholder='{"rules": [...], "version": "1.0"}'
                className="font-mono h-48"
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowApplyModal(false)}>
              Cancel
            </Button>
            <Button onClick={handleApply} disabled={isApplyingPolicy}>
              {isApplyingPolicy ? 'Applying...' : 'Apply Policy'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Compare Policies Modal */}
      <Dialog open={showCompareModal} onOpenChange={setShowCompareModal}>
        <DialogContent className="sm:max-w-[700px]">
          <DialogHeader>
            <DialogTitle>Compare Policies</DialogTitle>
            <DialogDescription>
              Enter two policy CPIDs to compare their differences.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="cpid1">First Policy CPID</Label>
                <Input
                  id="cpid1"
                  value={compareCpid1}
                  onChange={(e) => setCompareCpid1(e.target.value)}
                  placeholder="e.g., policy-v1"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="cpid2">Second Policy CPID</Label>
                <Input
                  id="cpid2"
                  value={compareCpid2}
                  onChange={(e) => setCompareCpid2(e.target.value)}
                  placeholder="e.g., policy-v2"
                />
              </div>
            </div>
            <Button onClick={handleCompare} disabled={isComparingPolicy} className="w-full">
              {isComparingPolicy ? 'Comparing...' : 'Compare'}
            </Button>

            {compareResult && (
              <div className="mt-4 space-y-4">
                <div className="flex items-center gap-2">
                  {compareResult.identical ? (
                    <>
                      <CheckCircle className="h-5 w-5 text-green-500" />
                      <span className="font-medium text-green-700">Policies are identical</span>
                    </>
                  ) : (
                    <>
                      <AlertTriangle className="h-5 w-5 text-yellow-500" />
                      <span className="font-medium text-yellow-700">
                        Found {compareResult.differences?.length || 0} difference(s)
                      </span>
                    </>
                  )}
                </div>

                {compareResult.differences && compareResult.differences.length > 0 && (
                  <div className="border rounded-md p-4 bg-muted/50 max-h-64 overflow-y-auto">
                    <h4 className="font-medium mb-2">Differences:</h4>
                    <ul className="space-y-2 text-sm">
                      {compareResult.differences.map((diff, idx) => (
                        <li key={idx} className="flex items-start gap-2">
                          <span className="font-mono text-xs">{diff}</span>
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
              </div>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowCompareModal(false)}>
              Close
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
