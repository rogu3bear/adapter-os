/**
 * PolicyViolationAlert - Displays policy violations with severity-based styling
 *
 * A reusable component for displaying policy violations across the UI.
 * Supports both individual violations and error responses from the API.
 *
 * Used in:
 * - StackDetailModal for showing recent violations
 * - API error handling for policy-blocked operations
 * - Real-time SSE violation notifications
 *
 * Citation: [2025-11-25†ui†policy-violation-alert] Policy violation alerts
 */

import React from 'react';
import { Alert, AlertDescription, AlertTitle } from './ui/alert';
import { Badge } from './ui/badge';
import { Button } from './ui/button';
import { AlertTriangle, XCircle, AlertCircle, Info, ChevronDown, ChevronUp, Shield } from 'lucide-react';
import { cn } from './ui/utils';
import { formatDistanceToNow, parseISO } from 'date-fns';
import type {
  PolicySeverity,
  PolicyViolationSummary,
  PolicyViolationErrorResponse,
  PolicyViolationItem,
} from '@/api/policyTypes';

// ============================================================================
// Types
// ============================================================================

export interface PolicyViolationAlertProps {
  /** Single violation to display */
  violation?: PolicyViolationSummary;
  /** Multiple violations (from error response) */
  violations?: PolicyViolationItem[];
  /** Error response from API */
  errorResponse?: PolicyViolationErrorResponse;
  /** Whether to show in compact mode */
  compact?: boolean;
  /** Whether to show remediation steps */
  showRemediation?: boolean;
  /** Callback when user wants to dismiss the alert */
  onDismiss?: () => void;
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Get icon for severity level
 */
function getSeverityIcon(severity: PolicySeverity) {
  switch (severity) {
    case 'critical':
      return <XCircle className="h-4 w-4" />;
    case 'high':
      return <AlertTriangle className="h-4 w-4" />;
    case 'medium':
      return <AlertCircle className="h-4 w-4" />;
    case 'low':
      return <Info className="h-4 w-4" />;
    default:
      return <AlertTriangle className="h-4 w-4" />;
  }
}

/**
 * Get badge color classes for severity
 */
function getSeverityBadgeClass(severity: PolicySeverity): string {
  switch (severity) {
    case 'critical':
      return 'border-red-600 text-red-600 bg-red-50';
    case 'high':
      return 'border-orange-500 text-orange-600 bg-orange-50';
    case 'medium':
      return 'border-yellow-500 text-yellow-600 bg-yellow-50';
    case 'low':
      return 'border-gray-500 text-gray-600 bg-gray-50';
    default:
      return 'border-gray-500 text-gray-600';
  }
}

/**
 * Get alert variant based on severity
 */
function getAlertVariant(severity: PolicySeverity): 'destructive' | 'default' {
  return severity === 'critical' || severity === 'high' ? 'destructive' : 'default';
}

// ============================================================================
// Single Violation Alert Component
// ============================================================================

interface SingleViolationProps {
  violation: PolicyViolationSummary;
  compact?: boolean;
}

function SingleViolationAlert({ violation, compact }: SingleViolationProps) {
  return (
    <Alert variant={getAlertVariant(violation.severity)} className={compact ? 'py-2' : ''}>
      {getSeverityIcon(violation.severity)}
      <AlertTitle className={cn('flex items-center gap-2', compact && 'text-sm')}>
        {violation.policy_name}
        <Badge variant="outline" className={cn('text-xs', getSeverityBadgeClass(violation.severity))}>
          {violation.severity}
        </Badge>
      </AlertTitle>
      <AlertDescription className={cn(compact ? 'text-xs' : 'text-sm', 'mt-1')}>
        {violation.message}
        {violation.detected_at && (
          <span className="block text-muted-foreground mt-1">
            {formatDistanceToNow(parseISO(violation.detected_at), { addSuffix: true })}
          </span>
        )}
        {violation.resolved_at && (
          <span className="block text-green-600 mt-1">
            Resolved {formatDistanceToNow(parseISO(violation.resolved_at), { addSuffix: true })}
            {violation.resolution_notes && `: ${violation.resolution_notes}`}
          </span>
        )}
      </AlertDescription>
    </Alert>
  );
}

// ============================================================================
// Multiple Violations List Component
// ============================================================================

interface ViolationsListProps {
  violations: PolicyViolationItem[];
  showRemediation?: boolean;
  compact?: boolean;
}

function ViolationsList({ violations, showRemediation, compact }: ViolationsListProps) {
  const [expanded, setExpanded] = React.useState(false);
  const displayViolations = expanded ? violations : violations.slice(0, 3);
  const hasMore = violations.length > 3;

  // Group by severity for summary
  const bySeverity = violations.reduce(
    (acc, v) => {
      acc[v.severity] = (acc[v.severity] || 0) + 1;
      return acc;
    },
    {} as Record<string, number>
  );

  // Determine overall severity for alert variant
  const overallSeverity: PolicySeverity = violations.some((v) => v.severity === 'critical')
    ? 'critical'
    : violations.some((v) => v.severity === 'high')
    ? 'high'
    : violations.some((v) => v.severity === 'medium')
    ? 'medium'
    : 'low';

  return (
    <Alert variant={getAlertVariant(overallSeverity)}>
      <Shield className="h-4 w-4" />
      <AlertTitle className="flex items-center gap-2">
        Policy Violations ({violations.length})
        <div className="flex gap-1">
          {bySeverity.critical && (
            <Badge variant="outline" className={cn('text-xs', getSeverityBadgeClass('critical'))}>
              {bySeverity.critical} critical
            </Badge>
          )}
          {bySeverity.high && (
            <Badge variant="outline" className={cn('text-xs', getSeverityBadgeClass('high'))}>
              {bySeverity.high} high
            </Badge>
          )}
        </div>
      </AlertTitle>
      <AlertDescription>
        <div className="mt-2 space-y-2">
          {displayViolations.map((violation, idx) => (
            <div
              key={`${violation.policy_id}-${idx}`}
              className={cn(
                'flex items-start gap-2 p-2 rounded border bg-background/50',
                compact && 'p-1.5'
              )}
            >
              {getSeverityIcon(violation.severity)}
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 flex-wrap">
                  <span className={cn('font-medium', compact ? 'text-xs' : 'text-sm')}>
                    {violation.policy_name}
                  </span>
                  <Badge
                    variant="outline"
                    className={cn('text-xs', getSeverityBadgeClass(violation.severity))}
                  >
                    {violation.severity}
                  </Badge>
                </div>
                <p className={cn('text-muted-foreground', compact ? 'text-xs' : 'text-sm')}>
                  {violation.message}
                </p>
                {showRemediation && violation.remediation && (
                  <p className="text-xs text-blue-600 mt-1">
                    <strong>Fix:</strong> {violation.remediation}
                  </p>
                )}
              </div>
            </div>
          ))}
        </div>

        {hasMore && (
          <Button
            variant="ghost"
            size="sm"
            className="mt-2 w-full"
            onClick={() => setExpanded(!expanded)}
          >
            {expanded ? (
              <>
                <ChevronUp className="h-4 w-4 mr-1" />
                Show less
              </>
            ) : (
              <>
                <ChevronDown className="h-4 w-4 mr-1" />
                Show {violations.length - 3} more
              </>
            )}
          </Button>
        )}
      </AlertDescription>
    </Alert>
  );
}

// ============================================================================
// Error Response Alert Component
// ============================================================================

interface ErrorResponseAlertProps {
  errorResponse: PolicyViolationErrorResponse;
  showRemediation?: boolean;
  onDismiss?: () => void;
}

function ErrorResponseAlert({ errorResponse, showRemediation, onDismiss }: ErrorResponseAlertProps) {
  const { details } = errorResponse;

  return (
    <div className="space-y-3">
      <Alert variant="destructive">
        <XCircle className="h-4 w-4" />
        <AlertTitle className="flex items-center justify-between">
          <span>Operation Blocked by Policy</span>
          {onDismiss && (
            <Button variant="ghost" size="sm" onClick={onDismiss}>
              Dismiss
            </Button>
          )}
        </AlertTitle>
        <AlertDescription>
          <p className="mb-2">
            The operation <strong>{details.operation}</strong> on stack{' '}
            <code className="text-xs bg-muted px-1 rounded">{details.stack_id}</code> was blocked
            due to policy violations.
          </p>
        </AlertDescription>
      </Alert>

      {details.violations.length > 0 && (
        <ViolationsList
          violations={details.violations}
          showRemediation={showRemediation}
          compact={false}
        />
      )}

      {showRemediation && details.remediation_steps.length > 0 && (
        <Alert>
          <Info className="h-4 w-4" />
          <AlertTitle>Remediation Steps</AlertTitle>
          <AlertDescription>
            <ol className="list-decimal list-inside space-y-1 mt-2">
              {details.remediation_steps.map((step, idx) => (
                <li key={idx} className="text-sm">
                  {step}
                </li>
              ))}
            </ol>
          </AlertDescription>
        </Alert>
      )}
    </div>
  );
}

// ============================================================================
// Main Component
// ============================================================================

/**
 * PolicyViolationAlert - Unified component for displaying policy violations
 *
 * Supports three modes:
 * 1. Single violation: Pass `violation` prop
 * 2. Multiple violations: Pass `violations` prop
 * 3. API error response: Pass `errorResponse` prop
 */
export function PolicyViolationAlert({
  violation,
  violations,
  errorResponse,
  compact = false,
  showRemediation = true,
  onDismiss,
  className,
}: PolicyViolationAlertProps) {
  return (
    <div className={cn('policy-violation-alert', className)}>
      {/* Single violation mode */}
      {violation && <SingleViolationAlert violation={violation} compact={compact} />}

      {/* Multiple violations mode */}
      {violations && violations.length > 0 && (
        <ViolationsList violations={violations} showRemediation={showRemediation} compact={compact} />
      )}

      {/* Error response mode */}
      {errorResponse && (
        <ErrorResponseAlert
          errorResponse={errorResponse}
          showRemediation={showRemediation}
          onDismiss={onDismiss}
        />
      )}

      {/* Empty state - no violations */}
      {!violation && !violations?.length && !errorResponse && (
        <Alert>
          <Shield className="h-4 w-4 text-green-500" />
          <AlertTitle>No Policy Violations</AlertTitle>
          <AlertDescription>All policy checks passed successfully.</AlertDescription>
        </Alert>
      )}
    </div>
  );
}

export default PolicyViolationAlert;
