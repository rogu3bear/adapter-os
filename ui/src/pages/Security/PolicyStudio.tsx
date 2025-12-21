// Policy Studio Page - Tenant policy customization interface
// Citation: AGENTS.md - Policy Studio feature for tenant-safe policy authoring

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
} from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  Plus,
  Edit,
  Trash2,
  Send,
  FileText,
  CheckCircle,
  XCircle,
  Clock,
  AlertTriangle,
  RefreshCw,
} from 'lucide-react';
import { toast } from 'sonner';
import {
  useTenantCustomizations,
  useCreateCustomization,
  useUpdateCustomization,
  useDeleteCustomization,
  useSubmitForReview,
  type TenantPolicyCustomization,
} from '@/hooks/security/useTenantPolicies';
import { useRBAC } from '@/hooks/security/useRBAC';
import { useAuth } from '@/providers/CoreProviders';
import { POLICY_PACKS, getPolicyPack, getDefaultPolicyConfig } from '@/constants/policySchema';
import { Skeleton } from '@/components/ui/skeleton';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { PermissionDenied } from '@/components/ui/permission-denied';

export default function PolicyStudio() {
  const { can } = useRBAC();
  const { user } = useAuth();
  const tenantId = user?.tenant_id || 'default';

  const [selectedPolicyType, setSelectedPolicyType] = useState<string>('');
  const [editingCustomization, setEditingCustomization] = useState<TenantPolicyCustomization | null>(null);
  const [showCreateDialog, setShowCreateDialog] = useState(false);
  const [customizationsJson, setCustomizationsJson] = useState('{}');
  const [validationErrors, setValidationErrors] = useState<string[]>([]);

  // Queries and mutations
  const { data: customizations, isLoading, error, refetch } = useTenantCustomizations(tenantId);
  const createMutation = useCreateCustomization(tenantId);
  const updateMutation = useUpdateCustomization(tenantId, editingCustomization?.id || '');
  const deleteMutation = useDeleteCustomization(tenantId);
  const submitMutation = useSubmitForReview(tenantId);

  // Check permissions
  if (!can('policy:customize')) {
    return (
      <div className="container mx-auto p-6">
        <PermissionDenied
          requiredPermission="policy:customize"
          requiredRoles={['admin', 'developer']}
        />
      </div>
    );
  }

  const handleCreate = useCallback(() => {
    if (!selectedPolicyType) {
      toast.error('Please select a policy type');
      return;
    }

    const defaultConfig = getDefaultPolicyConfig();
    const packs = defaultConfig.packs as Record<string, unknown>;
    const packConfig = packs[selectedPolicyType] || {};
    setCustomizationsJson(JSON.stringify(packConfig, null, 2));
    setValidationErrors([]);
    setShowCreateDialog(true);
  }, [selectedPolicyType]);

  const handleSaveCreate = useCallback(async () => {
    try {
      const response = await createMutation.mutateAsync({
        base_policy_type: selectedPolicyType,
        customizations_json: customizationsJson,
      });

      if (response.validation && !response.validation.valid) {
        setValidationErrors(response.validation.errors);
        return;
      }

      setShowCreateDialog(false);
      setSelectedPolicyType('');
      setCustomizationsJson('{}');
      setValidationErrors([]);
    } catch (error) {
      // Error handled by mutation
    }
  }, [selectedPolicyType, customizationsJson, createMutation]);

  const handleEdit = useCallback((customization: TenantPolicyCustomization) => {
    setEditingCustomization(customization);
    setCustomizationsJson(customization.customizations_json);
    setValidationErrors([]);
  }, []);

  const handleSaveEdit = useCallback(async () => {
    if (!editingCustomization) return;

    try {
      const response = await updateMutation.mutateAsync({
        customizations_json: customizationsJson,
      });

      if (response.validation && !response.validation.valid) {
        setValidationErrors(response.validation.errors);
        return;
      }

      setEditingCustomization(null);
      setCustomizationsJson('{}');
      setValidationErrors([]);
    } catch (error) {
      // Error handled by mutation
    }
  }, [editingCustomization, customizationsJson, updateMutation]);

  const handleDelete = useCallback(async (id: string) => {
    if (!confirm('Are you sure you want to delete this customization?')) return;
    await deleteMutation.mutateAsync(id);
  }, [deleteMutation]);

  const handleSubmit = useCallback(async (id: string) => {
    if (!confirm('Submit this customization for review?')) return;
    await submitMutation.mutateAsync(id);
  }, [submitMutation]);

  const getStatusBadge = (status: TenantPolicyCustomization['status']) => {
    switch (status) {
      case 'draft':
        return <Badge variant="outline"><FileText className="h-3 w-3 mr-1" />Draft</Badge>;
      case 'pending_review':
        return <Badge className="bg-yellow-500"><Clock className="h-3 w-3 mr-1" />Pending Review</Badge>;
      case 'approved':
        return <Badge className="bg-green-500"><CheckCircle className="h-3 w-3 mr-1" />Approved</Badge>;
      case 'rejected':
        return <Badge variant="destructive"><XCircle className="h-3 w-3 mr-1" />Rejected</Badge>;
      case 'active':
        return <Badge className="bg-blue-500"><CheckCircle className="h-3 w-3 mr-1" />Active</Badge>;
      default:
        return <Badge variant="outline">{status}</Badge>;
    }
  };

  const policyPack = selectedPolicyType ? getPolicyPack(selectedPolicyType) : null;

  if (error) {
    return (
      <div className="container mx-auto p-6">
        <ErrorRecovery error={error.message} onRetry={refetch} />
      </div>
    );
  }

  return (
    <div className="container mx-auto p-6 space-y-6">
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-3xl font-bold">Policy Studio</h1>
          <p className="text-muted-foreground mt-1">
            Customize policy parameters for your tenant
          </p>
        </div>
        <Button onClick={() => refetch()} variant="outline" size="sm">
          <RefreshCw className="h-4 w-4 mr-2" />
          Refresh
        </Button>
      </div>

      {/* Summary Cards */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Total Customizations
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{customizations?.length || 0}</div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Drafts
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {customizations?.filter(c => c.status === 'draft').length || 0}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Pending Review
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {customizations?.filter(c => c.status === 'pending_review').length || 0}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Active
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {customizations?.filter(c => c.status === 'active').length || 0}
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Create New Customization */}
      <Card>
        <CardHeader>
          <CardTitle>Create New Customization</CardTitle>
          <CardDescription>
            Select a guardrail to customize parameters within allowed bounds
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex gap-4 items-end">
            <div className="flex-1">
              <Label htmlFor="policy-type">Guardrail</Label>
              <Select value={selectedPolicyType} onValueChange={setSelectedPolicyType}>
                <SelectTrigger id="policy-type">
                  <SelectValue placeholder="Select guardrail..." />
                </SelectTrigger>
                <SelectContent>
                  {POLICY_PACKS.map(pack => (
                    <SelectItem key={pack.id} value={pack.id}>
                      {pack.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <Button onClick={handleCreate} disabled={!selectedPolicyType}>
              <Plus className="h-4 w-4 mr-2" />
              Create Customization
            </Button>
          </div>
          {policyPack && (
            <Alert>
              <AlertDescription>{policyPack.description}</AlertDescription>
            </Alert>
          )}
        </CardContent>
      </Card>

      {/* Customizations List */}
      <Card>
        <CardHeader>
          <CardTitle>Your Customizations</CardTitle>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <div className="space-y-4">
              {[1, 2, 3].map(i => (
                <Skeleton key={i} className="h-20 w-full" />
              ))}
            </div>
          ) : customizations?.length === 0 ? (
            <div className="text-center text-muted-foreground py-8">
              No customizations yet. Create one to get started.
            </div>
          ) : (
            <div className="space-y-4">
              {customizations?.map(customization => {
                const pack = getPolicyPack(customization.base_policy_type);
                return (
                  <Card key={customization.id}>
                    <CardHeader>
                      <div className="flex justify-between items-start">
                        <div>
                          <CardTitle className="text-lg">{pack?.name || customization.base_policy_type}</CardTitle>
                          <CardDescription className="mt-1">
                            {pack?.description}
                          </CardDescription>
                        </div>
                        <div className="flex gap-2 items-center">
                          {getStatusBadge(customization.status)}
                        </div>
                      </div>
                    </CardHeader>
                    <CardContent>
                      <div className="flex justify-between items-center">
                        <div className="text-sm text-muted-foreground">
                          Created {new Date(customization.created_at).toLocaleDateString()} by {customization.created_by}
                          {customization.reviewed_by && (
                            <span className="ml-2">• Reviewed by {customization.reviewed_by}</span>
                          )}
                        </div>
                        <div className="flex gap-2">
                          {customization.status === 'draft' && (
                            <>
                              <Button
                                size="sm"
                                variant="outline"
                                onClick={() => handleEdit(customization)}
                              >
                                <Edit className="h-4 w-4 mr-1" />
                                Edit
                              </Button>
                              <Button
                                size="sm"
                                variant="outline"
                                onClick={() => handleSubmit(customization.id)}
                              >
                                <Send className="h-4 w-4 mr-1" />
                                Submit
                              </Button>
                              <Button
                                size="sm"
                                variant="destructive"
                                onClick={() => handleDelete(customization.id)}
                              >
                                <Trash2 className="h-4 w-4" />
                              </Button>
                            </>
                          )}
                          {customization.status === 'rejected' && customization.review_notes && (
                            <Alert variant="destructive" className="mt-2">
                              <AlertTriangle className="h-4 w-4" />
                              <AlertDescription>
                                Rejection reason: {customization.review_notes}
                              </AlertDescription>
                            </Alert>
                          )}
                        </div>
                      </div>
                    </CardContent>
                  </Card>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Create/Edit Dialog */}
      <Dialog open={showCreateDialog || !!editingCustomization} onOpenChange={(open) => {
        if (!open) {
          setShowCreateDialog(false);
          setEditingCustomization(null);
          setValidationErrors([]);
        }
      }}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>
              {editingCustomization ? 'Edit Customization' : 'Create Customization'}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <Label htmlFor="customizations">Customizations (JSON)</Label>
              <Textarea
                id="customizations"
                value={customizationsJson}
                onChange={(e) => setCustomizationsJson(e.target.value)}
                rows={15}
                className="font-mono text-sm"
                placeholder='{"field_name": "value"}'
              />
            </div>
            {validationErrors.length > 0 && (
              <Alert variant="destructive">
                <AlertTriangle className="h-4 w-4" />
                <AlertDescription>
                  <div className="space-y-1">
                    <p className="font-medium">Validation Errors:</p>
                    <ul className="list-disc list-inside">
                      {validationErrors.map((error, idx) => (
                        <li key={idx} className="text-sm">{error}</li>
                      ))}
                    </ul>
                  </div>
                </AlertDescription>
              </Alert>
            )}
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setShowCreateDialog(false);
                setEditingCustomization(null);
                setValidationErrors([]);
              }}
            >
              Cancel
            </Button>
            <Button
              onClick={editingCustomization ? handleSaveEdit : handleSaveCreate}
              disabled={createMutation.isPending || updateMutation.isPending}
            >
              {editingCustomization ? 'Save' : 'Create'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

