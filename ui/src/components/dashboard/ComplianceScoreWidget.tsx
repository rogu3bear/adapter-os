import React from 'react';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Shield, CheckCircle, AlertTriangle, XCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useNavigate } from 'react-router-dom';
import { Skeleton } from '@/components/ui/skeleton';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { usePolicies, useComplianceAudit } from '@/hooks/security/useSecurity';
import { DashboardWidgetFrame, type DashboardWidgetState } from './DashboardWidgetFrame';
import { buildSecurityPoliciesLink } from '@/utils/navLinks';

interface PolicyPackStatus {
  name: string;
  compliant: boolean;
  violations: number;
}

export function ComplianceScoreWidget() {
  const navigate = useNavigate();
  const {
    policies,
    isLoading: isPoliciesLoading,
    error: policiesError,
    refetch: refetchPolicies,
    lastUpdated: policiesUpdatedAt
  } = usePolicies();
  const {
    complianceAudit,
    isLoading: isComplianceLoading,
    error: complianceError,
    refetch: refetchCompliance,
    lastUpdated: complianceUpdatedAt
  } = useComplianceAudit();

  const isLoading = isPoliciesLoading || isComplianceLoading;
  const error = policiesError || complianceError;

  const lastUpdated = [policiesUpdatedAt, complianceUpdatedAt]
    .filter((d): d is Date => Boolean(d))
    .sort((a, b) => b.getTime() - a.getTime())[0] || null;

  const handleRefresh = async (): Promise<void> => {
    await Promise.all([refetchPolicies(), refetchCompliance()]);
  };

  // Calculate compliance metrics from API data
  const totalPacks = policies?.length || 0;
  const enabledPolicies = policies?.filter(p => p.enabled !== false) || [];
  const violations = complianceAudit?.violations?.length || 0;

  // Calculate compliant packs (enabled policies with no violations)
  const violatedPolicyNames = new Set(
    complianceAudit?.violations?.map(v => v.rule) || []
  );
  const compliantPacks = enabledPolicies.filter(
    p => !violatedPolicyNames.has(p.name)
  ).length;

  // Calculate overall score (percentage of compliant packs)
  const overallScore = totalPacks > 0
    ? Math.round((compliantPacks / totalPacks) * 100)
    : 100;

  // Transform policies to PolicyPackStatus for display (show top 5)
  const policyPacks: PolicyPackStatus[] = (policies || [])
    .slice(0, 5)
    .map(policy => {
      const policyViolations = complianceAudit?.violations?.filter(
        v => v.rule === policy.name
      ) || [];
      return {
        name: policy.name,
        compliant: policyViolations.length === 0 && policy.enabled !== false,
        violations: policyViolations.length,
      };
    });

  const getScoreColor = (score: number) => {
    if (score >= 95) return 'text-green-600';
    if (score >= 80) return 'text-yellow-600';
    return 'text-red-600';
  };

  const getScoreBadge = (score: number) => {
    if (score >= 95) return 'Excellent';
    if (score >= 80) return 'Good';
    return 'Needs Attention';
  };

  const state: DashboardWidgetState = error
    ? 'error'
    : isLoading
      ? 'loading'
      : totalPacks === 0
        ? 'empty'
        : 'ready';

  return (
    <DashboardWidgetFrame
      title={
        <div className="flex items-center gap-2">
          <Shield className="h-5 w-5" />
          <span>Compliance Score</span>
        </div>
      }
      subtitle="Policy compliance overview"
      state={state}
      onRefresh={handleRefresh}
      onRetry={handleRefresh}
      lastUpdated={lastUpdated}
      errorMessage={error ? (error instanceof Error ? error.message : String(error)) : undefined}
      emptyMessage="No policies configured"
      emptyAction={
        <Button variant="outline" size="sm" onClick={() => navigate(buildSecurityPoliciesLink())}>
          Add policies
        </Button>
      }
      headerRight={
        !isLoading && !error ? (
          <Badge variant={overallScore >= 95 ? 'default' : 'destructive'}>
            {getScoreBadge(overallScore)}
          </Badge>
        ) : null
      }
      loadingContent={
        <>
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-2 w-full" />
          <Skeleton className="h-32 w-full" />
        </>
      }
    >
      <div className="text-center">
        <div className={`text-4xl font-bold ${getScoreColor(overallScore)}`}>
          {overallScore}%
        </div>
        <p className="text-sm text-muted-foreground mt-1">
          {compliantPacks}/{totalPacks} Policies Compliant
        </p>
      </div>

      <div>
        <Progress value={overallScore} className="h-2" />
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between text-sm">
          <span className="text-muted-foreground">Policies</span>
          {violations > 0 && (
            <span className="text-gray-700 font-medium">{violations} violations</span>
          )}
        </div>
        {policyPacks.length > 0 ? (
          <div className="space-y-1">
            {policyPacks.map((pack) => (
              <div key={pack.name} className="flex items-center justify-between text-sm p-2 rounded hover:bg-muted">
                <div className="flex items-center gap-2">
                  {pack.compliant ? (
                    <CheckCircle className="h-4 w-4 text-gray-600" />
                  ) : (
                    <XCircle className="h-4 w-4 text-gray-700" />
                  )}
                  <span>{pack.name}</span>
                </div>
                {pack.violations > 0 && (
                  <Badge variant="destructive" className="text-xs">
                    {pack.violations}
                  </Badge>
                )}
              </div>
            ))}
          </div>
        ) : (
          <div className="text-center py-4">
            <p className="text-sm text-muted-foreground">No policies to display</p>
          </div>
        )}
      </div>

      {violations > 0 && (
        <Button
          variant="outline"
          size="sm"
          className="w-full"
          onClick={() => navigate(buildSecurityPoliciesLink())}
        >
          <AlertTriangle className="h-4 w-4 mr-2" />
          Review Violations
        </Button>
      )}
    </DashboardWidgetFrame>
  );
}

