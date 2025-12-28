/**
 * Example: PolicyCheckDisplay Integration with Promotion Workflow
 *
 * This file demonstrates how to integrate PolicyCheckDisplay into a
 * promotion workflow component. Use this as a reference when building
 * the actual PromotionWorkflow component.
 */

import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { PolicyCheckDisplay, usePolicyChecks } from './index';
import { AlertCircle, CheckCircle2, ArrowRight } from 'lucide-react';
import { User } from '@/api/types';

export interface PromotionWorkflowExampleProps {
  cpid: string;
  user: User;
  selectedTenant: string;
  onPromote: (cpid: string) => Promise<void>;
}

/**
 * Example promotion workflow with integrated policy checks.
 * This shows the recommended integration pattern.
 */
export function PromotionWorkflowExample({
  cpid,
  user,
  selectedTenant,
  onPromote,
}: PromotionWorkflowExampleProps) {
  const [promotionStep, setPromotionStep] = useState<'verify' | 'review' | 'confirm'>('verify');
  const [isPromoting, setIsPromoting] = useState(false);
  const [promotionError, setPromotionError] = useState<string | null>(null);

  // Fetch policy checks using the hook
  const { policies, isLoading: policiesLoading, error: policiesError, overridePolicy } =
    usePolicyChecks({ cpid });

  // Calculate statistics
  const stats = {
    total: policies.length,
    passed: policies.filter(p => p.status === 'passed').length,
    failed: policies.filter(p => p.status === 'failed').length,
    warnings: policies.filter(p => p.status === 'warning').length,
    canPromote: policies.filter(p => p.status === 'failed').length === 0,
  };

  const handlePromote = async () => {
    try {
      setIsPromoting(true);
      setPromotionError(null);
      await onPromote(cpid);
      // Navigate to next step or show success
      setPromotionStep('confirm');
    } catch (err) {
      setPromotionError(err instanceof Error ? err.message : 'Promotion failed');
    } finally {
      setIsPromoting(false);
    }
  };

  const isAdmin = user.role.toLowerCase() === 'admin';
  const canPromote = stats.canPromote || (isAdmin && stats.failed > 0);

  return (
    <div className="w-full space-y-4">
      {/* Promotion status header */}
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <div>
              <CardTitle>Promotion Status</CardTitle>
              <p className="text-sm text-muted-foreground mt-1">
                Plan: <code className="bg-muted px-2 py-1 rounded text-xs">{cpid}</code>
              </p>
            </div>
            <div className="text-right">
              <p className="text-sm text-muted-foreground">Current Step</p>
              <Badge variant="info" className="mt-1">
                {promotionStep === 'verify' && 'Policy Verification'}
                {promotionStep === 'review' && 'Review & Approval'}
                {promotionStep === 'confirm' && 'Promotion Complete'}
              </Badge>
            </div>
          </div>
        </CardHeader>
      </Card>

      {/* Step 1: Policy Verification */}
      {promotionStep === 'verify' && (
        <Tabs value="policies" className="w-full">
          <TabsList className="grid w-full grid-cols-2">
            <TabsTrigger value="policies" disabled={policiesLoading}>
              <span className="flex items-center gap-2">
                Policies
                {stats.canPromote ? (
                  <CheckCircle2 className="w-4 h-4 text-green-600" />
                ) : (
                  <AlertCircle className="w-4 h-4 text-red-600" />
                )}
              </span>
            </TabsTrigger>
            <TabsTrigger value="details">Details</TabsTrigger>
          </TabsList>

          <TabsContent value="policies" className="space-y-4 mt-4">
            {/* Show error if policy fetch failed */}
            {policiesError && (
              <Alert variant="destructive">
                <AlertCircle className="h-4 w-4" />
                <AlertTitle>Error Loading Policies</AlertTitle>
                <AlertDescription>{policiesError.message}</AlertDescription>
              </Alert>
            )}

            {/* Display policy checks */}
            {!policiesError && (
              <PolicyCheckDisplay
                cpid={cpid}
                policies={policies}
                loading={policiesLoading}
                onOverride={overridePolicy}
                blockPromotion={!stats.canPromote && !isAdmin}
                allowAdmin={isAdmin}
                userRole={user.role}
              />
            )}

            {/* Action buttons */}
            <div className="pt-4 border-t space-y-2">
              <div className="flex gap-2">
                <Button
                  variant="outline"
                  className="flex-1"
                  onClick={() => setPromotionStep('verify')}
                >
                  Refresh Checks
                </Button>
                <Button
                  className="flex-1 gap-2"
                  onClick={() => setPromotionStep('review')}
                  disabled={!canPromote || policiesLoading}
                >
                  Continue to Review
                  <ArrowRight className="w-4 h-4" />
                </Button>
              </div>

              {!stats.canPromote && !isAdmin && (
                <p className="text-xs text-red-600 text-center">
                  Cannot proceed: {stats.failed} policy failure(s) must be resolved
                </p>
              )}
            </div>
          </TabsContent>

          <TabsContent value="details">
            <div className="space-y-4 mt-4">
              <Card>
                <CardHeader>
                  <CardTitle className="text-base">Plan Summary</CardTitle>
                </CardHeader>
                <CardContent className="space-y-3">
                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <p className="text-xs text-muted-foreground">Workspace</p>
                      <p className="font-medium">{selectedTenant}</p>
                    </div>
                    <div>
                      <p className="text-xs text-muted-foreground">User</p>
                      <p className="font-medium">{user.display_name || user.email}</p>
                    </div>
                  </div>
                </CardContent>
              </Card>

              <Card>
                <CardHeader>
                  <CardTitle className="text-base">Next Steps</CardTitle>
                </CardHeader>
                <CardContent>
                  <ol className="space-y-2 text-sm">
                    <li className="flex gap-2">
                      <span className="font-bold text-muted-foreground">1.</span>
                      <span>Verify all policies pass (or override with justification)</span>
                    </li>
                    <li className="flex gap-2">
                      <span className="font-bold text-muted-foreground">2.</span>
                      <span>Review plan details and changes</span>
                    </li>
                    <li className="flex gap-2">
                      <span className="font-bold text-muted-foreground">3.</span>
                      <span>Confirm promotion to apply plan</span>
                    </li>
                  </ol>
                </CardContent>
              </Card>
            </div>
          </TabsContent>
        </Tabs>
      )}

      {/* Step 2: Review & Approval */}
      {promotionStep === 'review' && (
        <Card>
          <CardHeader>
            <CardTitle>Review Plan</CardTitle>
            <p className="text-sm text-muted-foreground mt-2">
              Review the plan details before final promotion.
            </p>
          </CardHeader>
          <CardContent className="space-y-4">
            {/* Plan details would go here */}
            <div className="bg-muted p-4 rounded space-y-2">
              <p className="text-sm font-medium">Plan ID: {cpid}</p>
              <p className="text-sm text-muted-foreground">
                Policies: {stats.passed} passed, {stats.failed} failed, {stats.warnings} warnings
              </p>
            </div>

            {promotionError && (
              <Alert variant="destructive">
                <AlertCircle className="h-4 w-4" />
                <AlertTitle>Promotion Error</AlertTitle>
                <AlertDescription>{promotionError}</AlertDescription>
              </Alert>
            )}

            <div className="pt-4 border-t space-y-2">
              <Button
                variant="outline"
                className="w-full"
                onClick={() => setPromotionStep('verify')}
                disabled={isPromoting}
              >
                Back to Verification
              </Button>
              <Button
                className="w-full gap-2"
                onClick={handlePromote}
                disabled={isPromoting}
              >
                {isPromoting ? 'Promoting...' : 'Confirm Promotion'}
              </Button>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Step 3: Confirmation */}
      {promotionStep === 'confirm' && (
        <Card className="border-green-200 bg-green-50">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-green-900">
              <CheckCircle2 className="w-5 h-5" />
              Promotion Successful
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <p className="text-sm text-green-700">
              Plan {cpid} has been successfully promoted to production.
            </p>
            <div className="space-y-2">
              <Button className="w-full" onClick={() => window.location.href = '/dashboard'}>
                Return to Dashboard
              </Button>
              <Button
                variant="outline"
                className="w-full"
                onClick={() => setPromotionStep('verify')}
              >
                Start New Promotion
              </Button>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

export default PromotionWorkflowExample;

/**
 * Integration Checklist:
 *
 * [ ] Import PolicyCheckDisplay and usePolicyChecks
 * [ ] Add cpid prop to component
 * [ ] Use usePolicyChecks hook to fetch policies
 * [ ] Pass policies to PolicyCheckDisplay
 * [ ] Handle onOverride callback
 * [ ] Set allowAdmin based on user role
 * [ ] Block promotion if failures exist (unless admin)
 * [ ] Add error handling for failed policy fetches
 * [ ] Connect promotion button to handlePromote
 * [ ] Update API calls to match your backend
 * [ ] Test with various policy states
 *
 * API Endpoints Required:
 * - GET /v1/policies/{cpid} - Fetch policy checks
 * - POST /v1/policies/{cpid}/override - Override policy
 * - POST /v1/promotions/{cpid} - Execute promotion
 */
