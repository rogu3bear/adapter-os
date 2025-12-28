// Policy Review Queue Page - Admin/Compliance policy approval interface
// Citation: AGENTS.md - Policy Studio feature review workflow

import React, { useState, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  CheckCircle,
  XCircle,
  Clock,
  RefreshCw,
  FileText,
  User,
  Calendar,
  AlertTriangle,
} from 'lucide-react';
import { toast } from 'sonner';
import {
  usePendingReviews,
  useApproveCustomization,
  useRejectCustomization,
  useActivateCustomization,
  type TenantPolicyCustomization,
} from '@/hooks/security/useTenantPolicies';
import { useRBAC } from '@/hooks/security/useRBAC';
import { getPolicyPack } from '@/constants/policySchema';
import { Skeleton } from '@/components/ui/skeleton';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { PermissionDenied } from '@/components/ui/permission-denied';

export default function PolicyReviewQueue() {
  const { can } = useRBAC();

  const [reviewingCustomization, setReviewingCustomization] = useState<TenantPolicyCustomization | null>(null);
  const [reviewAction, setReviewAction] = useState<'approve' | 'reject' | null>(null);
  const [reviewNotes, setReviewNotes] = useState('');

  // Queries and mutations
  const { data: pendingReviews, isLoading, error, refetch } = usePendingReviews();
  const approveMutation = useApproveCustomization();
  const rejectMutation = useRejectCustomization();
  const activateMutation = useActivateCustomization();

  // Check permissions
  if (!can('policy:review')) {
    return (
      <div className="container mx-auto p-6">
        <PermissionDenied
          requiredPermission="policy:review"
          requiredRoles={['admin', 'compliance', 'developer']}
        />
      </div>
    );
  }

  const handleReview = useCallback((customization: TenantPolicyCustomization, action: 'approve' | 'reject') => {
    setReviewingCustomization(customization);
    setReviewAction(action);
    setReviewNotes('');
  }, []);

  const handleConfirmReview = useCallback(async () => {
    if (!reviewingCustomization || !reviewAction) return;

    try {
      if (reviewAction === 'approve') {
        await approveMutation.mutateAsync({
          customizationId: reviewingCustomization.id,
          notes: reviewNotes || undefined,
        });
      } else {
        await rejectMutation.mutateAsync({
          customizationId: reviewingCustomization.id,
          notes: reviewNotes || undefined,
        });
      }

      setReviewingCustomization(null);
      setReviewAction(null);
      setReviewNotes('');
    } catch (error) {
      // Error handled by mutation
    }
  }, [reviewingCustomization, reviewAction, reviewNotes, approveMutation, rejectMutation]);

  const handleActivate = useCallback(async (id: string) => {
    if (!confirm('Activate this policy customization? This will deactivate any existing active customization for the same policy type.')) return;
    await activateMutation.mutateAsync(id);
  }, [activateMutation]);

  if (error) {
    return (
      <div className="container mx-auto p-6">
        <ErrorRecovery error={error.message} onRetry={() => refetch()} />
      </div>
    );
  }

  return (
    <div className="container mx-auto p-6 space-y-6">
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-3xl font-bold">Policy Review Queue</h1>
          <p className="text-muted-foreground mt-1">
            Review and approve tenant policy customizations
          </p>
        </div>
        <Button onClick={() => refetch()} variant="outline" size="sm">
          <RefreshCw className="h-4 w-4 mr-2" />
          Refresh
        </Button>
      </div>

      {/* Summary Card */}
      <Card>
        <CardHeader>
          <CardTitle>Pending Reviews</CardTitle>
          <CardDescription>
            {pendingReviews?.length || 0} customization{pendingReviews?.length !== 1 ? 's' : ''} waiting for review
          </CardDescription>
        </CardHeader>
      </Card>

      {/* Pending Reviews List */}
      <div className="space-y-4">
        {isLoading ? (
          <div className="space-y-4">
            {[1, 2, 3].map(i => (
              <Skeleton key={i} className="h-32 w-full" />
            ))}
          </div>
        ) : pendingReviews?.length === 0 ? (
          <Card>
            <CardContent className="p-8 text-center text-muted-foreground">
              No pending reviews. All clear!
            </CardContent>
          </Card>
        ) : (
          pendingReviews?.map(customization => {
            const pack = getPolicyPack(customization.base_policy_type);
            const customizationsObj = JSON.parse(customization.customizations_json);

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
                    <Badge className="bg-yellow-500">
                      <Clock className="h-3 w-3 mr-1" />
                      Pending Review
                    </Badge>
                  </div>
                </CardHeader>
                <CardContent className="space-y-4">
                  {/* Metadata */}
                  <div className="grid grid-cols-3 gap-4 text-sm">
                    <div className="flex items-center gap-2">
                      <User className="h-4 w-4 text-muted-foreground" />
                      <span className="text-muted-foreground">Workspace:</span>
                      <span className="font-medium">{customization.tenant_id}</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <User className="h-4 w-4 text-muted-foreground" />
                      <span className="text-muted-foreground">Created by:</span>
                      <span className="font-medium">{customization.created_by}</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <Calendar className="h-4 w-4 text-muted-foreground" />
                      <span className="text-muted-foreground">Submitted:</span>
                      <span className="font-medium">
                        {customization.submitted_at ? new Date(customization.submitted_at).toLocaleDateString() : 'N/A'}
                      </span>
                    </div>
                  </div>

                  {/* Customizations Preview */}
                  <div>
                    <Label>Customization Values:</Label>
                    <div className="bg-muted p-3 rounded-md mt-1 max-h-40 overflow-y-auto">
                      <pre className="text-xs font-mono">
                        {JSON.stringify(customizationsObj, null, 2)}
                      </pre>
                    </div>
                  </div>

                  {/* Action Buttons */}
                  <div className="flex justify-end gap-2 pt-4 border-t">
                    <Button
                      variant="outline"
                      onClick={() => handleReview(customization, 'reject')}
                      disabled={approveMutation.isPending || rejectMutation.isPending}
                    >
                      <XCircle className="h-4 w-4 mr-2" />
                      Reject
                    </Button>
                    <Button
                      onClick={() => handleReview(customization, 'approve')}
                      disabled={approveMutation.isPending || rejectMutation.isPending}
                    >
                      <CheckCircle className="h-4 w-4 mr-2" />
                      Approve
                    </Button>
                  </div>
                </CardContent>
              </Card>
            );
          })
        )}
      </div>

      {/* Review Dialog */}
      <Dialog open={!!reviewingCustomization} onOpenChange={(open) => {
        if (!open) {
          setReviewingCustomization(null);
          setReviewAction(null);
          setReviewNotes('');
        }
      }}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {reviewAction === 'approve' ? 'Approve Customization' : 'Reject Customization'}
            </DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            {reviewingCustomization && (
              <Alert>
                <FileText className="h-4 w-4" />
                <AlertDescription>
                  <div className="space-y-1">
                    <p className="font-medium">
                      {getPolicyPack(reviewingCustomization.base_policy_type)?.name || reviewingCustomization.base_policy_type}
                    </p>
                    <p className="text-sm">
                      Workspace: {reviewingCustomization.tenant_id}
                    </p>
                    <p className="text-sm">
                      Created by: {reviewingCustomization.created_by}
                    </p>
                  </div>
                </AlertDescription>
              </Alert>
            )}
            <div>
              <Label htmlFor="review-notes">
                Review Notes {reviewAction === 'reject' && <span className="text-red-500">(Required for rejection)</span>}
              </Label>
              <Textarea
                id="review-notes"
                value={reviewNotes}
                onChange={(e) => setReviewNotes(e.target.value)}
                rows={4}
                placeholder={reviewAction === 'approve' 
                  ? 'Optional notes about this approval...'
                  : 'Explain why this customization is being rejected...'
                }
              />
            </div>
            {reviewAction === 'approve' && (
              <Alert>
                <AlertTriangle className="h-4 w-4" />
                <AlertDescription>
                  After approval, an Admin must activate the customization for it to take effect.
                </AlertDescription>
              </Alert>
            )}
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setReviewingCustomization(null);
                setReviewAction(null);
                setReviewNotes('');
              }}
            >
              Cancel
            </Button>
            <Button
              variant={reviewAction === 'approve' ? 'default' : 'destructive'}
              onClick={handleConfirmReview}
              disabled={
                approveMutation.isPending || 
                rejectMutation.isPending || 
                (reviewAction === 'reject' && !reviewNotes.trim())
              }
            >
              {reviewAction === 'approve' ? 'Approve' : 'Reject'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
