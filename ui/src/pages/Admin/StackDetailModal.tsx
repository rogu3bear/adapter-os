import { useState, useEffect } from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { Progress } from '@/components/ui/progress';
import type { AdapterStack, LifecycleHistoryEvent, PolicyPreflightResponse } from '@/api/types';
import { Layers, Calendar, History, ArrowRight, MessageSquare, Power, PowerOff, AlertTriangle, HardDrive, Shield, CheckCircle2, XCircle, AlertCircle } from 'lucide-react';
import apiClient from '@/api/client';
import { formatDistanceToNow, parseISO } from 'date-fns';
import { useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { toast } from 'sonner';
import { calculateTotalMemory } from '@/utils/memoryEstimation';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { logger } from '@/utils/logger';
import { useChatSessions } from '@/hooks/useChatSessions';
import { useTenant } from '@/layout/LayoutProvider';
import { PolicyPreflightDialog } from '@/components/PolicyPreflightDialog';
import { useStackPolicyStream } from '@/hooks/useStreamingEndpoints';
import {
  getComplianceStatusColor,
  getComplianceStatusLabel,
  formatComplianceScore,
  sortViolationsBySeverity,
  type StackPoliciesResponse,
  type PolicySeverity,
} from '@/api/policyTypes';

interface StackDetailModalProps {
  stack: AdapterStack;
  open: boolean;
  onClose: () => void;
}

export function StackDetailModal({ stack, open, onClose }: StackDetailModalProps) {
  const navigate = useNavigate();
  const { selectedTenant } = useTenant();
  const [history, setHistory] = useState<LifecycleHistoryEvent[]>([]);
  const [loadingHistory, setLoadingHistory] = useState(false);
  const [isActivating, setIsActivating] = useState(false);
  const [isDeactivating, setIsDeactivating] = useState(false);
  const [showPreflightDialog, setShowPreflightDialog] = useState(false);
  const [preflightData, setPreflightData] = useState<PolicyPreflightResponse | null>(null);

  // Get tenant ID from context
  const tenantId = selectedTenant || 'default';
  const { sessions } = useChatSessions(tenantId);
  
  // Find sessions using this stack
  const sessionsUsingStack = sessions.filter(s => s.stackId === stack.id);
  
  // Note: Inference endpoint tracking would require backend API support
  // Currently, we only track chat sessions. Inference endpoints using stacks
  // would need to be tracked via audit logs or a dedicated endpoint.
  // This is a known limitation - see docs/ARCHITECTURE_PATTERNS.md for details.

  // Fetch adapters for memory calculation
  const { data: availableAdapters } = useQuery({
    queryKey: ['adapters'],
    queryFn: () => apiClient.listAdapters(),
    enabled: open,
  });

  // Fetch capacity for memory warnings
  const { data: capacity } = useQuery({
    queryKey: ['capacity'],
    queryFn: () => apiClient.getCapacity(),
    enabled: open,
  });

  // Fetch stack policies (PRD-GOV-01)
  const {
    data: stackPolicies,
    isLoading: loadingPolicies,
    refetch: refetchPolicies,
  } = useQuery({
    queryKey: ['stack-policies', stack.id],
    queryFn: () => apiClient.getStackPolicies(stack.id),
    enabled: open && !!stack.id,
    staleTime: 30000, // 30 seconds
  });

  // Subscribe to real-time policy events
  const { data: policyEvent } = useStackPolicyStream(stack.id, {
    enabled: open && !!stack.id,
    onMessage: (event) => {
      // Show toast for important events
      if (event.event_type === 'violation_detected') {
        const violationEvent = event as { severity: PolicySeverity; message: string };
        const severity = violationEvent.severity;
        if (severity === 'critical' || severity === 'high') {
          toast.error(`Policy Violation: ${violationEvent.message}`, {
            duration: 10000,
          });
        }
      } else if (event.event_type === 'compliance_changed') {
        // Refetch policies when compliance changes
        refetchPolicies();
      }
    },
  });

  // Calculate memory usage
  const adapterIds = stack.adapter_ids || stack.adapters?.map(a => typeof a === 'string' ? a : a.adapter_id) || [];
  const memoryWarnings = availableAdapters && capacity ? (() => {
    const { totalBytes, estimated, missing } = calculateTotalMemory(adapterIds, availableAdapters);
    const warnings: string[] = [];
    
    if (missing.length > 0) {
      warnings.push(`${missing.length} adapter(s) not found for memory calculation`);
    }
    
    if (estimated) {
      warnings.push('Memory estimate may be inaccurate');
    }
    
    const totalMemoryMB = totalBytes / (1024 * 1024);
    const totalRAMMB = (capacity.total_ram_bytes || 0) / (1024 * 1024);
    const memoryUsagePercent = capacity.total_ram_bytes > 0 ? (totalBytes / capacity.total_ram_bytes) * 100 : 0;
    
    if (memoryUsagePercent > 85) {
      warnings.push(`Memory usage (${totalMemoryMB.toFixed(1)} MB) exceeds 85% of capacity (${totalRAMMB.toFixed(1)} MB)`);
    } else if (memoryUsagePercent > 70) {
      warnings.push(`Memory usage (${totalMemoryMB.toFixed(1)} MB) is high (${memoryUsagePercent.toFixed(1)}% of capacity)`);
    }
    
    return warnings;
  })() : [];

  // Check if stack is active (lifecycle_state === 'active')
  const isActive = (stack.lifecycle_state || 'active').toLowerCase() === 'active';

  const handleActivate = async () => {
    setIsActivating(true);
    try {
      // Run preflight policy checks
      const preflight = await apiClient.preflightStackActivation(stack.id);
      setPreflightData(preflight);

      if (!preflight.can_proceed || preflight.checks.some(c => !c.passed)) {
        // Show preflight dialog if there are concerns
        setShowPreflightDialog(true);
        setIsActivating(false);
      } else {
        // All checks passed, proceed with activation
        await doActivateStack();
      }
    } catch (error) {
      toast.error(`Failed to run preflight checks: ${error instanceof Error ? error.message : 'Unknown error'}`);
      setIsActivating(false);
    }
  };

  const doActivateStack = async () => {
    setIsActivating(true);
    try {
      await apiClient.activateAdapterStack(stack.id);
      toast.success('Stack activated successfully');
      onClose(); // Close modal to refresh data
    } catch (error) {
      toast.error(`Failed to activate stack: ${error instanceof Error ? error.message : 'Unknown error'}`);
    } finally {
      setIsActivating(false);
    }
  };

  const handleDeactivate = async () => {
    setIsDeactivating(true);
    try {
      // Note: deactivateAdapterStack() is a global deactivate endpoint
      // It deactivates the currently active stack, not a specific stack
      // This may need backend changes to support stack-specific deactivation
      await apiClient.deactivateAdapterStack();
      toast.success('Stack deactivated successfully');
      onClose(); // Close modal to refresh data
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      toast.error(`Failed to deactivate stack: ${errorMessage}`);
      logger.error('Failed to deactivate stack', {
        component: 'StackDetailModal',
        stackId: stack.id,
      }, error instanceof Error ? error : new Error(errorMessage));
    } finally {
      setIsDeactivating(false);
    }
  };

  useEffect(() => {
    if (open && stack.id) {
      setLoadingHistory(true);
      apiClient
        .getAdapterStackHistory(stack.id)
        .then(setHistory)
        .catch(() => setHistory([]))
        .finally(() => setLoadingHistory(false));
    }
  }, [open, stack.id]);
  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="max-w-2xl max-h-[80vh] overflow-y-auto">
        <DialogHeader>
          <div className="flex items-center justify-between">
            <div>
              <DialogTitle>Adapter Stack: {stack.name}</DialogTitle>
              <DialogDescription>
                Stack ID: <span className="font-mono">{stack.id}</span>
              </DialogDescription>
            </div>
            <div className="flex items-center gap-2">
              {isActive ? (
                <Button
                  variant="outline"
                  onClick={handleDeactivate}
                  disabled={isDeactivating}
                >
                  <PowerOff className="h-4 w-4 mr-2" />
                  {isDeactivating ? 'Deactivating...' : 'Deactivate'}
                </Button>
              ) : (
                <Button
                  onClick={handleActivate}
                  disabled={isActivating}
                >
                  <Power className="h-4 w-4 mr-2" />
                  {isActivating ? 'Activating...' : 'Activate'}
                </Button>
              )}
              <Button
                onClick={() => {
                  onClose();
                  navigate(`/chat?stack=${stack.id}`);
                }}
              >
                <MessageSquare className="h-4 w-4 mr-2" />
                Use in Chat
              </Button>
            </div>
          </div>
        </DialogHeader>

        <div className="space-y-4">
          {/* Memory Warnings */}
          {memoryWarnings.length > 0 && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertTitle>Memory Warnings</AlertTitle>
              <AlertDescription>
                <ul className="list-disc list-inside space-y-1">
                  {memoryWarnings.map((warning, idx) => (
                    <li key={idx}>{warning}</li>
                  ))}
                </ul>
              </AlertDescription>
            </Alert>
          )}

          <Card>
            <CardHeader>
              <CardTitle>General Information</CardTitle>
            </CardHeader>
            <CardContent className="grid gap-4">
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <p className="text-sm font-medium text-muted-foreground">Created</p>
                  <p className="text-sm mt-1">
                    {new Date(stack.created_at).toLocaleString()}
                  </p>
                </div>
                <div>
                  <p className="text-sm font-medium text-muted-foreground">Last Updated</p>
                  <p className="text-sm mt-1">
                    {new Date(stack.updated_at).toLocaleString()}
                  </p>
                </div>
                <div>
                  <p className="text-sm font-medium text-muted-foreground">Lifecycle State</p>
                  <div className="mt-1">
                    {(() => {
                      const state = stack.lifecycle_state || 'active';
                      const stateConfig: Record<string, { variant: 'default' | 'secondary' | 'outline'; className: string }> = {
                        active: { variant: 'default', className: 'bg-green-500 text-white hover:bg-green-600' },
                        deprecated: { variant: 'secondary', className: 'bg-yellow-500 text-white hover:bg-yellow-600' },
                        retired: { variant: 'outline', className: 'bg-gray-500 text-white hover:bg-gray-600' },
                        draft: { variant: 'secondary', className: 'bg-blue-500 text-white hover:bg-blue-600' },
                      };
                      const config = stateConfig[state.toLowerCase()] || stateConfig.active;
                      return (
                        <Badge variant={config.variant} className={`text-xs ${config.className}`}>
                          {state.charAt(0).toUpperCase() + state.slice(1)}
                        </Badge>
                      );
                    })()}
                  </div>
                </div>
                <div>
                  <p className="text-sm font-medium text-muted-foreground">Version</p>
                  <p className="text-sm mt-1 font-mono">
                    {stack.version ?? 1}
                  </p>
                </div>
              </div>

              {stack.description && (
                <div>
                  <p className="text-sm font-medium text-muted-foreground">Description</p>
                  <p className="text-sm mt-1 text-foreground">{stack.description}</p>
                </div>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Layers className="h-5 w-5" />
                Adapters ({stack.adapters.length})
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-3">
                {stack.adapters.map((adapter, index) => {
                  const adapterId =
                    typeof adapter === 'string' ? adapter : adapter.adapter_id;
                  const gate =
                    typeof adapter === 'object' && 'gate' in adapter
                      ? adapter.gate
                      : undefined;

                  return (
                    <div
                      key={index}
                      className="flex items-center justify-between p-3 border rounded-lg"
                    >
                      <div className="flex items-center gap-3">
                        <Badge variant="outline" className="font-mono">
                          {index + 1}
                        </Badge>
                        <span className="font-medium">{adapterId}</span>
                      </div>
                      {gate !== undefined && (
                        <div className="flex items-center gap-2">
                          <span className="text-sm text-muted-foreground">Gate:</span>
                          <Badge variant="secondary">{gate}</Badge>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </CardContent>
          </Card>

          {/* Memory Usage */}
          {availableAdapters && capacity && (
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <HardDrive className="h-5 w-5" />
                  Memory Usage
                </CardTitle>
              </CardHeader>
              <CardContent>
                {(() => {
                  const { totalBytes, estimated } = calculateTotalMemory(adapterIds, availableAdapters);
                  const totalMemoryMB = totalBytes / (1024 * 1024);
                  const totalRAMMB = (capacity.total_ram_bytes || 0) / (1024 * 1024);
                  const memoryUsagePercent = capacity.total_ram_bytes > 0 ? (totalBytes / capacity.total_ram_bytes) * 100 : 0;

                  return (
                    <div className="space-y-2">
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">Stack Memory</span>
                        <span className="text-sm font-medium">
                          {totalMemoryMB.toFixed(1)} MB {estimated && '(estimated)'}
                        </span>
                      </div>
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">Node Capacity</span>
                        <span className="text-sm font-medium">{totalRAMMB.toFixed(1)} MB</span>
                      </div>
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">Usage</span>
                        <span className={`text-sm font-medium ${memoryUsagePercent > 85 ? 'text-destructive' : memoryUsagePercent > 70 ? 'text-yellow-600' : ''}`}>
                          {memoryUsagePercent.toFixed(1)}%
                        </span>
                      </div>
                    </div>
                  );
                })()}
              </CardContent>
            </Card>
          )}

          {/* Policy Compliance Section (PRD-GOV-01) */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Shield className="h-5 w-5" />
                Policy Compliance
              </CardTitle>
            </CardHeader>
            <CardContent>
              {loadingPolicies ? (
                <div className="space-y-3">
                  <Skeleton className="h-8 w-full" />
                  <Skeleton className="h-16 w-full" />
                  <Skeleton className="h-12 w-full" />
                </div>
              ) : stackPolicies ? (
                <div className="space-y-4">
                  {/* Compliance Score Summary */}
                  <div className="p-4 rounded-lg border bg-muted/30">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-sm font-medium">Overall Compliance</span>
                      <div className="flex items-center gap-2">
                        {stackPolicies.compliance.status === 'compliant' && (
                          <CheckCircle2 className="h-4 w-4 text-green-500" />
                        )}
                        {stackPolicies.compliance.status === 'warning' && (
                          <AlertCircle className="h-4 w-4 text-yellow-500" />
                        )}
                        {stackPolicies.compliance.status === 'non_compliant' && (
                          <XCircle className="h-4 w-4 text-red-500" />
                        )}
                        <Badge
                          variant="outline"
                          className={
                            stackPolicies.compliance.status === 'compliant'
                              ? 'border-green-500 text-green-700'
                              : stackPolicies.compliance.status === 'warning'
                              ? 'border-yellow-500 text-yellow-700'
                              : 'border-red-500 text-red-700'
                          }
                        >
                          {getComplianceStatusLabel(stackPolicies.compliance.status)}
                        </Badge>
                      </div>
                    </div>
                    <div className="space-y-1">
                      <div className="flex items-center justify-between text-sm">
                        <span className="text-muted-foreground">Score</span>
                        <span className="font-mono font-medium">
                          {formatComplianceScore(stackPolicies.compliance.overall_score)}
                        </span>
                      </div>
                      <Progress
                        value={stackPolicies.compliance.overall_score}
                        className={`h-2 ${
                          stackPolicies.compliance.overall_score >= 90
                            ? '[&>div]:bg-green-500'
                            : stackPolicies.compliance.overall_score >= 70
                            ? '[&>div]:bg-yellow-500'
                            : '[&>div]:bg-red-500'
                        }`}
                      />
                    </div>

                    {/* Category breakdown */}
                    {Object.entries(stackPolicies.compliance.by_category).length > 0 && (
                      <div className="mt-3 pt-3 border-t space-y-2">
                        <span className="text-xs text-muted-foreground uppercase tracking-wide">
                          By Category
                        </span>
                        <div className="grid grid-cols-2 gap-2">
                          {Object.entries(stackPolicies.compliance.by_category).map(
                            ([category, score]) => (
                              <div
                                key={category}
                                className="flex items-center justify-between text-sm p-2 rounded bg-background"
                              >
                                <span className="capitalize">{category}</span>
                                <span
                                  className={`font-mono text-xs ${
                                    score.score >= 90
                                      ? 'text-green-600'
                                      : score.score >= 70
                                      ? 'text-yellow-600'
                                      : 'text-red-600'
                                  }`}
                                >
                                  {Math.round(score.score)}%
                                </span>
                              </div>
                            )
                          )}
                        </div>
                      </div>
                    )}
                  </div>

                  {/* Assigned Policies */}
                  {stackPolicies.assignments.length > 0 && (
                    <div>
                      <div className="flex items-center justify-between mb-2">
                        <span className="text-sm font-medium">Assigned Policies</span>
                        <Badge variant="outline">{stackPolicies.assignments.length}</Badge>
                      </div>
                      <div className="space-y-2 max-h-40 overflow-y-auto">
                        {stackPolicies.assignments.map((assignment) => (
                          <div
                            key={assignment.id}
                            className="flex items-center justify-between p-2 border rounded text-sm"
                          >
                            <div className="flex items-center gap-2">
                              <Shield className="h-4 w-4 text-muted-foreground" />
                              <span>{assignment.policy_name}</span>
                            </div>
                            <div className="flex items-center gap-2">
                              {assignment.enforced && (
                                <Badge variant="secondary" className="text-xs">
                                  Enforced
                                </Badge>
                              )}
                              <Badge
                                variant="outline"
                                className={
                                  assignment.status === 'active'
                                    ? 'border-green-500 text-green-700'
                                    : 'border-gray-500 text-gray-700'
                                }
                              >
                                {assignment.status}
                              </Badge>
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Recent Violations */}
                  {stackPolicies.recent_violations.length > 0 && (
                    <div>
                      <div className="flex items-center justify-between mb-2">
                        <span className="text-sm font-medium text-red-600">
                          Recent Violations
                        </span>
                        <Badge variant="destructive">
                          {stackPolicies.recent_violations.length}
                        </Badge>
                      </div>
                      <div className="space-y-2 max-h-40 overflow-y-auto">
                        {sortViolationsBySeverity(stackPolicies.recent_violations).map(
                          (violation) => (
                            <Alert
                              key={violation.id}
                              variant={
                                violation.severity === 'critical' ||
                                violation.severity === 'high'
                                  ? 'destructive'
                                  : 'default'
                              }
                              className="py-2"
                            >
                              <AlertTriangle className="h-4 w-4" />
                              <AlertTitle className="text-sm flex items-center gap-2">
                                {violation.policy_name}
                                <Badge
                                  variant="outline"
                                  className={`text-xs ${
                                    violation.severity === 'critical'
                                      ? 'border-red-600 text-red-600'
                                      : violation.severity === 'high'
                                      ? 'border-orange-500 text-orange-600'
                                      : violation.severity === 'medium'
                                      ? 'border-yellow-500 text-yellow-600'
                                      : 'border-gray-500 text-gray-600'
                                  }`}
                                >
                                  {violation.severity}
                                </Badge>
                              </AlertTitle>
                              <AlertDescription className="text-xs mt-1">
                                {violation.message}
                                <span className="block text-muted-foreground mt-1">
                                  {formatDistanceToNow(parseISO(violation.detected_at), {
                                    addSuffix: true,
                                  })}
                                </span>
                              </AlertDescription>
                            </Alert>
                          )
                        )}
                      </div>
                    </div>
                  )}

                  {/* No policies assigned */}
                  {stackPolicies.assignments.length === 0 && (
                    <p className="text-sm text-muted-foreground text-center py-4">
                      No policies assigned to this stack
                    </p>
                  )}
                </div>
              ) : (
                <p className="text-sm text-muted-foreground">
                  Unable to load policy information
                </p>
              )}
            </CardContent>
          </Card>

          {/* Used In Section */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <MessageSquare className="h-5 w-5" />
                Used In
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-3">
                {/* Chat Sessions */}
                <div>
                  <div className="flex items-center justify-between mb-2">
                    <span className="text-sm font-medium">Chat Sessions</span>
                    <Badge variant="outline">{sessionsUsingStack.length}</Badge>
                  </div>
                  {sessionsUsingStack.length > 0 ? (
                    <div className="space-y-1">
                      {sessionsUsingStack.slice(0, 5).map(session => (
                        <div
                          key={session.id}
                          className="flex items-center justify-between p-2 border rounded-lg hover:bg-muted/50 cursor-pointer"
                          onClick={() => {
                            onClose();
                            navigate(`/chat?stack=${stack.id}&session=${session.id}`);
                          }}
                        >
                          <div className="flex-1 min-w-0">
                            <p className="text-sm font-medium truncate">{session.name}</p>
                            <p className="text-xs text-muted-foreground">
                              {session.messages.length} message{session.messages.length !== 1 ? 's' : ''}
                            </p>
                          </div>
                          <Button variant="ghost" size="sm">
                            Open
                          </Button>
                        </div>
                      ))}
                      {sessionsUsingStack.length > 5 && (
                        <p className="text-xs text-muted-foreground text-center">
                          +{sessionsUsingStack.length - 5} more session{sessionsUsingStack.length - 5 !== 1 ? 's' : ''}
                        </p>
                      )}
                    </div>
                  ) : (
                    <p className="text-sm text-muted-foreground">No active chat sessions</p>
                  )}
                </div>

                {/* Quick Actions */}
                <div className="pt-2 border-t">
                  <Button
                    variant="outline"
                    size="sm"
                    className="w-full"
                    onClick={() => {
                      onClose();
                      navigate(`/chat?stack=${stack.id}`);
                    }}
                  >
                    <MessageSquare className="h-4 w-4 mr-2" />
                    Use in Chat
                  </Button>
                </div>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <History className="h-5 w-5" />
                Version History
              </CardTitle>
            </CardHeader>
            <CardContent>
              {loadingHistory ? (
                <div className="space-y-3">
                  <Skeleton className="h-16 w-full" />
                  <Skeleton className="h-16 w-full" />
                  <Skeleton className="h-16 w-full" />
                </div>
              ) : history.length === 0 ? (
                <p className="text-sm text-muted-foreground">No version history available</p>
              ) : (
                <div className="space-y-3">
                  {history.map((event, index) => {
                    const stateConfig: Record<string, { variant: 'default' | 'secondary' | 'outline'; className: string }> = {
                      active: { variant: 'default', className: 'bg-green-500 text-white hover:bg-green-600' },
                      deprecated: { variant: 'secondary', className: 'bg-yellow-500 text-white hover:bg-yellow-600' },
                      retired: { variant: 'outline', className: 'bg-gray-500 text-white hover:bg-gray-600' },
                      draft: { variant: 'secondary', className: 'bg-blue-500 text-white hover:bg-blue-600' },
                    };
                    const config = stateConfig[event.lifecycle_state.toLowerCase()] || stateConfig.active;
                    const prevConfig = event.previous_lifecycle_state
                      ? stateConfig[event.previous_lifecycle_state.toLowerCase()] || stateConfig.active
                      : null;

                    return (
                      <div key={event.id} className="border rounded-lg p-4 space-y-2">
                        <div className="flex items-center justify-between">
                          <div className="flex items-center gap-2">
                            {event.previous_lifecycle_state && (
                              <>
                                <Badge variant={prevConfig.variant} className={`text-xs ${prevConfig.className}`}>
                                  {event.previous_lifecycle_state.charAt(0).toUpperCase() + event.previous_lifecycle_state.slice(1)}
                                </Badge>
                                <ArrowRight className="h-4 w-4 text-muted-foreground" />
                              </>
                            )}
                            <Badge variant={config.variant} className={`text-xs ${config.className}`}>
                              {event.lifecycle_state.charAt(0).toUpperCase() + event.lifecycle_state.slice(1)}
                            </Badge>
                            <Badge variant="outline" className="text-xs font-mono">
                              v{event.version}
                            </Badge>
                          </div>
                          <span className="text-xs text-muted-foreground">
                            {formatDistanceToNow(parseISO(event.created_at), { addSuffix: true })}
                          </span>
                        </div>
                        <div className="text-sm text-muted-foreground">
                          <div>By: {event.initiated_by}</div>
                          {event.reason && (
                            <div className="mt-1">Reason: {event.reason}</div>
                          )}
                          <div className="mt-1 text-xs">
                            {new Date(event.created_at).toLocaleString()}
                          </div>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </DialogContent>

      {/* Policy Preflight Dialog */}
      {preflightData && (
        <PolicyPreflightDialog
          open={showPreflightDialog}
          onOpenChange={setShowPreflightDialog}
          title="Policy Validation - Activate Stack"
          description={`Review policy checks before activating stack "${stack.name}"`}
          checks={preflightData.checks}
          canProceed={preflightData.can_proceed}
          onProceed={async () => {
            setShowPreflightDialog(false);
            await doActivateStack();
          }}
          onCancel={() => {
            setShowPreflightDialog(false);
            setIsActivating(false);
          }}
          isAdmin={false} // TODO: Get from user context
          isLoading={isActivating}
        />
      )}
    </Dialog>
  );
}
