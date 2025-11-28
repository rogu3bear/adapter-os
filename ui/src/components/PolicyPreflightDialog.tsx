/**
 * PolicyPreflightDialog - Shows policy check results before loading adapters or activating stacks
 *
 * Used in adapter loading workflows and stack activation to enforce 23 canonical policies.
 * Supports admin override for non-critical policy violations.
 *
 * Citation: [2025-11-25†ui†policy-preflight-dialog]
 */

import React, { useMemo, useRef, useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Alert, AlertDescription, AlertTitle } from './ui/alert';
import { AlertTriangle, CheckCircle, XCircle, Shield, Info } from 'lucide-react';
import { cn } from './ui/utils';

export interface PolicyCheck {
  policy_id: string;
  policy_name: string;
  passed: boolean;
  severity: 'error' | 'warning' | 'info';
  message: string;
  can_override?: boolean;
  details?: string;
}

export interface PolicyPreflightDialogProps {
  /** Whether the dialog is open */
  open: boolean;
  /** Callback when dialog open state changes */
  onOpenChange: (open: boolean) => void;
  /** Dialog title */
  title: string;
  /** Optional description */
  description?: string;
  /** Policy check results */
  checks: PolicyCheck[];
  /** Whether user can proceed (all critical checks passed) */
  canProceed: boolean;
  /** Callback when user clicks Proceed */
  onProceed: () => void;
  /** Callback when user clicks Cancel */
  onCancel: () => void;
  /** Whether current user is admin (enables override) */
  isAdmin?: boolean;
  /** Loading state for proceed action */
  isLoading?: boolean;
}

/**
 * Get icon for severity level
 */
function getSeverityIcon(severity: 'error' | 'warning' | 'info') {
  switch (severity) {
    case 'error':
      return <XCircle className="w-4 h-4" />;
    case 'warning':
      return <AlertTriangle className="w-4 h-4" />;
    case 'info':
      return <Info className="w-4 h-4" />;
  }
}

/**
 * Get badge variant for severity level
 */
function getSeverityBadgeVariant(severity: 'error' | 'warning' | 'info'): 'error' | 'warning' | 'info' {
  return severity;
}

/**
 * PolicyPreflightDialog component
 *
 * Displays policy check results in a modal dialog before allowing user to proceed with
 * adapter loading or stack activation. Enforces 23 canonical policies with admin override
 * support for non-critical violations.
 *
 * @example
 * ```tsx
 * <PolicyPreflightDialog
 *   open={showPreflight}
 *   onOpenChange={setShowPreflight}
 *   title="Policy Validation - Load Adapter"
 *   description="The following policies will be enforced when loading this adapter"
 *   checks={policyChecks}
 *   canProceed={allCriticalChecksPassed}
 *   onProceed={handleLoadAdapter}
 *   onCancel={() => setShowPreflight(false)}
 *   isAdmin={userRole === 'admin'}
 * />
 * ```
 */
export function PolicyPreflightDialog({
  open,
  onOpenChange,
  title,
  description,
  checks,
  canProceed,
  onProceed,
  onCancel,
  isAdmin = false,
  isLoading = false,
}: PolicyPreflightDialogProps) {
  const [overriddenPolicies, setOverriddenPolicies] = useState<Set<string>>(new Set());

  // Calculate statistics
  const stats = useMemo(() => {
    const total = checks.length;
    const passed = checks.filter(c => c.passed).length;
    const failed = checks.filter(c => !c.passed);
    const errors = failed.filter(c => c.severity === 'error').length;
    const warnings = failed.filter(c => c.severity === 'warning').length;
    const info = failed.filter(c => c.severity === 'info').length;

    return { total, passed, errors, warnings, info };
  }, [checks]);

  // Categorize checks
  const { passedChecks, failedChecks } = useMemo(() => {
    const passed = checks.filter(c => c.passed);
    const failed = checks.filter(c => !c.passed);

    // Sort failed checks by severity (error > warning > info)
    const severityOrder = { error: 0, warning: 1, info: 2 };
    failed.sort((a, b) => severityOrder[a.severity] - severityOrder[b.severity]);

    return { passedChecks: passed, failedChecks: failed };
  }, [checks]);

  // Check if there are blocking errors (cannot override)
  const hasBlockingErrors = useMemo(() => {
    return failedChecks.some(c => c.severity === 'error' && !c.can_override);
  }, [failedChecks]);

  // Check if proceed is allowed (accounting for overrides)
  const canActuallyProceed = useMemo(() => {
    if (canProceed) return true;
    if (!isAdmin) return false;

    // Check if all failed checks are either overridden or overridable
    return failedChecks.every(c =>
      overriddenPolicies.has(c.policy_id) || c.can_override
    );
  }, [canProceed, isAdmin, failedChecks, overriddenPolicies]);

  const handleOverrideToggle = (policyId: string) => {
    const newOverrides = new Set(overriddenPolicies);
    if (newOverrides.has(policyId)) {
      newOverrides.delete(policyId);
    } else {
      newOverrides.add(policyId);
    }
    setOverriddenPolicies(newOverrides);
  };

  const handleProceed = () => {
    // Reset overrides on proceed
    setOverriddenPolicies(new Set());
    onProceed();
  };

  const handleCancel = () => {
    // Reset overrides on cancel
    setOverriddenPolicies(new Set());
    onCancel();
  };

  const summaryIdRef = useRef(`policy-summary-${Math.random().toString(36).slice(2)}`);
  const descriptionIdRef = useRef(`policy-description-${Math.random().toString(36).slice(2)}`);
  const summaryId = summaryIdRef.current;
  const descriptionId = descriptionIdRef.current;
  const ariaDescribedBy = [summaryId, description ? descriptionId : null].filter(Boolean).join(' ');
  const summaryText = `${stats.total} checks, ${stats.errors} errors, ${stats.warnings} warnings, ${stats.info} info checks.`;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-w-2xl max-h-[80vh] overflow-hidden flex flex-col"
        aria-describedby={ariaDescribedBy}
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Shield className="w-5 h-5 text-primary" />
            {title}
          </DialogTitle>
        </DialogHeader>
        {description && (
          <DialogDescription id={descriptionId}>{description}</DialogDescription>
        )}
        <DialogDescription id={summaryId} className="sr-only">
          {summaryText}
        </DialogDescription>

        {/* Summary Stats */}
        <div className="grid grid-cols-4 gap-3 py-3 border-y">
          <div className="text-center">
            <div className="text-2xl font-bold">{stats.total}</div>
            <div className="text-xs text-muted-foreground">Total</div>
          </div>
          <div className="text-center">
            <div className="text-2xl font-bold text-green-600">{stats.passed}</div>
            <div className="text-xs text-muted-foreground">Passed</div>
          </div>
          <div className="text-center">
            <div className="text-2xl font-bold text-red-600">{stats.errors}</div>
            <div className="text-xs text-muted-foreground">Errors</div>
          </div>
          <div className="text-center">
            <div className="text-2xl font-bold text-yellow-600">{stats.warnings}</div>
            <div className="text-xs text-muted-foreground">Warnings</div>
          </div>
        </div>

        {/* Blocking Error Alert */}
        {hasBlockingErrors && (
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle>Cannot Proceed</AlertTitle>
            <AlertDescription>
              Critical policy violations detected that cannot be overridden.
              Please resolve these issues before proceeding.
            </AlertDescription>
          </Alert>
        )}

        {/* Policy Checks List */}
        <div className="flex-1 overflow-y-auto space-y-3 pr-2">
          {/* Failed Checks */}
          {failedChecks.length > 0 && (
            <div className="space-y-2">
              <h3 className="text-sm font-semibold text-muted-foreground uppercase tracking-wide">
                Failed Checks ({failedChecks.length})
              </h3>
              {failedChecks.map(check => (
                <div
                  key={check.policy_id}
                  className={cn(
                    "border rounded-lg p-3 space-y-2 transition-colors",
                    check.severity === 'error'
                      ? "border-red-200 bg-red-50/50 dark:border-red-900 dark:bg-red-950/20"
                      : check.severity === 'warning'
                        ? "border-yellow-200 bg-yellow-50/50 dark:border-yellow-900 dark:bg-yellow-950/20"
                        : "border-blue-200 bg-blue-50/50 dark:border-blue-900 dark:bg-blue-950/20"
                  )}
                >
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex items-start gap-2 flex-1 min-w-0">
                      {getSeverityIcon(check.severity)}
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 flex-wrap">
                          <span className="font-medium text-sm">{check.policy_name}</span>
                          <Badge variant={getSeverityBadgeVariant(check.severity)} className="text-xs">
                            {check.severity}
                          </Badge>
                          {overriddenPolicies.has(check.policy_id) && (
                            <Badge variant="warning" className="text-xs">
                              Overridden
                            </Badge>
                          )}
                        </div>
                        <p className="text-sm text-muted-foreground mt-1">{check.message}</p>
                        {check.details && (
                          <p className="text-xs text-muted-foreground mt-1 font-mono bg-muted px-2 py-1 rounded">
                            {check.details}
                          </p>
                        )}
                      </div>
                    </div>
                    {/* Admin Override Toggle */}
                    {isAdmin && check.can_override && (
                      <Button
                        variant={overriddenPolicies.has(check.policy_id) ? "destructive" : "outline"}
                        size="sm"
                        onClick={() => handleOverrideToggle(check.policy_id)}
                        className="shrink-0"
                      >
                        {overriddenPolicies.has(check.policy_id) ? 'Undo Override' : 'Override'}
                      </Button>
                    )}
                    {!check.can_override && check.severity === 'error' && (
                      <Badge variant="error" className="text-xs shrink-0">
                        Cannot Override
                      </Badge>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}

          {/* Passed Checks (Collapsed) */}
          {passedChecks.length > 0 && (
            <details className="space-y-2">
              <summary className="text-sm font-semibold text-muted-foreground uppercase tracking-wide cursor-pointer hover:text-foreground transition-colors">
                Passed Checks ({passedChecks.length})
              </summary>
              <div className="space-y-2 mt-2">
                {passedChecks.map(check => (
                  <div
                    key={check.policy_id}
                    className="border border-green-200 bg-green-50/50 dark:border-green-900 dark:bg-green-950/20 rounded-lg p-3"
                  >
                    <div className="flex items-start gap-2">
                      <CheckCircle className="w-4 h-4 text-green-600 shrink-0 mt-0.5" />
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 flex-wrap">
                          <span className="font-medium text-sm">{check.policy_name}</span>
                          <Badge variant="success" className="text-xs">
                            passed
                          </Badge>
                        </div>
                        <p className="text-sm text-muted-foreground mt-1">{check.message}</p>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </details>
          )}
        </div>

        {/* Footer */}
        <DialogFooter className="border-t pt-4">
          <div className="flex items-center justify-between w-full gap-4">
            {/* Override warning for admins */}
            {isAdmin && overriddenPolicies.size > 0 && (
              <Alert variant="default" className="flex-1 py-2">
                <AlertTriangle className="h-4 w-4 text-yellow-600" />
                <AlertDescription className="text-xs">
                  {overriddenPolicies.size} policy override{overriddenPolicies.size !== 1 ? 's' : ''} active
                </AlertDescription>
              </Alert>
            )}
            <div className="flex gap-2 ml-auto">
              <Button
                variant="outline"
                onClick={handleCancel}
                disabled={isLoading}
              >
                Cancel
              </Button>
              <Button
                variant={canActuallyProceed ? 'default' : 'destructive'}
                onClick={handleProceed}
                disabled={!canActuallyProceed || isLoading}
              >
                {isLoading ? 'Loading...' : canProceed ? 'Proceed' : 'Proceed (Override)'}
              </Button>
            </div>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export default PolicyPreflightDialog;
