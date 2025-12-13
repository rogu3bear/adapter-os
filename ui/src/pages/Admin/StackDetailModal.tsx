import { useState, useEffect } from 'react';
import { Modal } from '@/components/shared/Modal';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { Progress } from '@/components/ui/progress';
import type { AdapterStack, LifecycleHistoryEvent, PolicyPreflightResponse } from '@/api/types';
import { Layers, Calendar, History, ArrowRight, MessageSquare, Power, PowerOff, AlertTriangle, HardDrive, Shield, CheckCircle2, XCircle, AlertCircle, Trash2 } from 'lucide-react';
import apiClient from '@/api/client';
import { formatDistanceToNow, parseISO } from 'date-fns';
import { useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { toast } from 'sonner';
import { calculateTotalMemory } from '@/utils/memoryEstimation';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { logger } from '@/utils/logger';
import { useChatSessions } from '@/hooks/useChatSessions';
import { useTenant } from '@/providers/FeatureProviders';
import { PolicyPreflightDialog } from '@/components/PolicyPreflightDialog';
import { useStackPolicyStream } from '@/hooks/useStreamingEndpoints';
import { useClearStackAdapters } from '@/hooks/useAdmin';
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

const lifecycleStateConfig: Record<string, { variant: 'default' | 'secondary' | 'outline'; className: string }> = {
  active: { variant: 'default', className: 'bg-success text-white hover:bg-success/90' },
  deprecated: { variant: 'secondary', className: 'bg-warning text-white hover:bg-warning/90' },
  retired: { variant: 'outline', className: 'bg-muted text-white hover:bg-muted/90' },
  draft: { variant: 'secondary', className: 'bg-info text-white hover:bg-info/90' },
};

const LifecycleBadge = ({ state }: { state: string }) => {
  const config = lifecycleStateConfig[state.toLowerCase()] || lifecycleStateConfig.active;
  return (
    <Badge variant={config.variant} className={`text-xs ${config.className}`}>
      {state.charAt(0).toUpperCase() + state.slice(1)}
    </Badge>
  );
};

const StackHeader = ({
  stack,
  isActive,
  isDeactivating,
  isActivating,
  onDeactivate,
  onActivate,
  onUseInChat,
  onClearAdapters,
  isClearingAdapters,
}: {
  stack: AdapterStack;
  isActive: boolean;
  isDeactivating: boolean;
  isActivating: boolean;
  onDeactivate: () => void;
  onActivate: () => void;
  onUseInChat: () => void;
  onClearAdapters: () => void;
  isClearingAdapters: boolean;
}) => (
  <div className="flex w-full items-center justify-between">
    <div>
      <h2 className="text-lg font-semibold leading-none">Adapter Stack: {stack.name}</h2>
      <p className="mt-2 text-sm text-muted-foreground">
        Stack ID: <span className="font-mono">{stack.id}</span>
      </p>
    </div>
    <div className="flex items-center gap-2">
      {isActive ? (
        <Button variant="outline" onClick={onDeactivate} disabled={isDeactivating}>
          <PowerOff className="mr-2 h-4 w-4" />
          {isDeactivating ? 'Deactivating...' : 'Deactivate'}
        </Button>
      ) : (
        <Button onClick={onActivate} disabled={isActivating}>
          <Power className="mr-2 h-4 w-4" />
          {isActivating ? 'Activating...' : 'Activate'}
        </Button>
      )}
      <Button
        variant="destructive"
        onClick={onClearAdapters}
        disabled={isClearingAdapters || stack.adapters.length === 0}
      >
        <Trash2 className="mr-2 h-4 w-4" />
        {isClearingAdapters ? 'Clearing...' : 'Detach All'}
      </Button>
      <Button onClick={onUseInChat}>
        <MessageSquare className="mr-2 h-4 w-4" />
        Use in Chat
      </Button>
    </div>
  </div>
);

const MemoryWarnings = ({ warnings }: { warnings: string[] }) =>
  warnings.length > 0 ? (
    <Alert variant="destructive">
      <AlertTriangle className="h-4 w-4" />
      <AlertTitle>Memory Warnings</AlertTitle>
      <AlertDescription>
        <ul className="list-inside list-disc space-y-1">
          {warnings.map((warning, idx) => (
            <li key={idx}>{warning}</li>
          ))}
        </ul>
      </AlertDescription>
    </Alert>
  ) : null;

const GeneralInfoCard = ({ stack }: { stack: AdapterStack }) => (
  <Card>
    <CardHeader>
      <CardTitle>General Information</CardTitle>
    </CardHeader>
    <CardContent className="grid gap-4">
      <div className="grid grid-cols-2 gap-4">
        <div>
          <p className="text-sm font-medium text-muted-foreground">Created</p>
          <p className="mt-1 text-sm">{new Date(stack.created_at).toLocaleString()}</p>
        </div>
        <div>
          <p className="text-sm font-medium text-muted-foreground">Last Updated</p>
          <p className="mt-1 text-sm">{new Date(stack.updated_at).toLocaleString()}</p>
        </div>
        <div>
          <p className="text-sm font-medium text-muted-foreground">Lifecycle State</p>
          <div className="mt-1">
            <LifecycleBadge state={stack.lifecycle_state || 'active'} />
          </div>
        </div>
        <div>
          <p className="text-sm font-medium text-muted-foreground">Version</p>
          <p className="mt-1 font-mono text-sm">{stack.version ?? 1}</p>
        </div>
      </div>
      {stack.description && (
        <div>
          <p className="text-sm font-medium text-muted-foreground">Description</p>
          <p className="mt-1 text-sm text-foreground">{stack.description}</p>
        </div>
      )}
    </CardContent>
  </Card>
);

const AdapterRow = ({
  index,
  adapterId,
  gate,
}: {
  index: number;
  adapterId: string;
  gate?: number;
}) => (
  <div className="flex items-center justify-between rounded-lg border p-3">
    <div className="flex items-center gap-3">
      <Badge variant="outline" className="font-mono">
        {index + 1}
      </Badge>
      <span className="font-medium">{adapterId}</span>
    </div>
    {gate !== undefined && (
      <div className="flex items-center gap-2">
        <span className="text-sm text-muted-foreground">Confidence:</span>
        <Badge variant="secondary">{gate}</Badge>
      </div>
    )}
  </div>
);

const AdaptersCard = ({ stack }: { stack: AdapterStack }) => (
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
          const adapterId = typeof adapter === 'string' ? adapter : adapter.adapter_id;
          const gate =
            typeof adapter === 'object' && 'gate' in adapter ? adapter.gate : undefined;
          return <AdapterRow key={adapterId ?? index} index={index} adapterId={adapterId} gate={gate} />;
        })}
      </div>
    </CardContent>
  </Card>
);

const MemoryUsageCard = ({
  adapterIds,
  availableAdapters,
  capacity,
}: {
  adapterIds: string[];
  availableAdapters: Awaited<ReturnType<typeof apiClient.listAdapters>> | undefined;
  capacity: Awaited<ReturnType<typeof apiClient.getCapacity>> | undefined;
}) => {
  if (!availableAdapters || !capacity) return null;
  const { totalBytes, estimated } = calculateTotalMemory(adapterIds, availableAdapters);
  const totalMemoryMB = totalBytes / (1024 * 1024);
  const totalRAMMB = (capacity.total_ram_bytes || 0) / (1024 * 1024);
  const memoryUsagePercent =
    capacity.total_ram_bytes > 0 ? (totalBytes / capacity.total_ram_bytes) * 100 : 0;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <HardDrive className="h-5 w-5" />
          Memory Usage
        </CardTitle>
      </CardHeader>
      <CardContent>
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
            <span
              className={`text-sm font-medium ${
                memoryUsagePercent > 85
                  ? 'text-destructive'
                  : memoryUsagePercent > 70
                  ? 'text-warning'
                  : ''
              }`}
            >
              {memoryUsagePercent.toFixed(1)}%
            </span>
          </div>
        </div>
      </CardContent>
    </Card>
  );
};

const ComplianceSummary = ({ stackPolicies }: { stackPolicies: StackPoliciesResponse }) => (
  <div className="rounded-lg border bg-muted/30 p-4">
    <div className="mb-2 flex items-center justify-between">
      <span className="text-sm font-medium">Overall Compliance</span>
      <div className="flex items-center gap-2">
        {stackPolicies.compliance.status === 'compliant' && (
          <CheckCircle2 className="h-4 w-4 text-success" />
        )}
        {stackPolicies.compliance.status === 'warning' && (
          <AlertCircle className="h-4 w-4 text-warning" />
        )}
        {stackPolicies.compliance.status === 'non_compliant' && (
          <XCircle className="h-4 w-4 text-destructive" />
        )}
        <Badge
          variant="outline"
          className={
            stackPolicies.compliance.status === 'compliant'
              ? 'border-success text-success'
              : stackPolicies.compliance.status === 'warning'
              ? 'border-warning text-warning'
              : 'border-destructive text-destructive'
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
            ? '[&>div]:bg-success'
            : stackPolicies.compliance.overall_score >= 70
            ? '[&>div]:bg-warning'
            : '[&>div]:bg-destructive'
        }`}
      />
    </div>
    {Object.entries(stackPolicies.compliance.by_category).length > 0 && (
      <div className="mt-3 space-y-2 border-t pt-3">
        <span className="text-xs uppercase tracking-wide text-muted-foreground">By Category</span>
        <div className="grid grid-cols-2 gap-2">
          {Object.entries(stackPolicies.compliance.by_category).map(([category, score]) => (
            <div
              key={category}
              className="flex items-center justify-between rounded bg-background p-2 text-sm"
            >
              <span className="capitalize">{category}</span>
              <span
                className={`font-mono text-xs ${
                  score.score >= 90
                    ? 'text-success'
                    : score.score >= 70
                    ? 'text-warning'
                    : 'text-destructive'
                }`}
              >
                {Math.round(score.score)}%
              </span>
            </div>
          ))}
        </div>
      </div>
    )}
  </div>
);

const PolicyAssignments = ({ stackPolicies }: { stackPolicies: StackPoliciesResponse }) =>
  stackPolicies.assignments.length > 0 ? (
    <div>
      <div className="mb-2 flex items-center justify-between">
        <span className="text-sm font-medium">Assigned Policies</span>
        <Badge variant="outline">{stackPolicies.assignments.length}</Badge>
      </div>
      <div className="max-h-40 space-y-2 overflow-y-auto">
        {stackPolicies.assignments.map((assignment) => (
          <div
            key={assignment.id}
            className="flex items-center justify-between rounded border p-2 text-sm"
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
                    ? 'border-success text-success'
                    : 'border-muted text-muted-foreground'
                }
              >
                {assignment.status}
              </Badge>
            </div>
          </div>
        ))}
      </div>
    </div>
  ) : (
    <p className="py-4 text-center text-sm text-muted-foreground">No policies assigned to this stack</p>
  );

const PolicyViolations = ({ stackPolicies }: { stackPolicies: StackPoliciesResponse }) =>
  stackPolicies.recent_violations.length > 0 ? (
    <div>
      <div className="mb-2 flex items-center justify-between">
        <span className="text-sm font-medium text-destructive">Recent Violations</span>
        <Badge variant="destructive">{stackPolicies.recent_violations.length}</Badge>
      </div>
      <div className="max-h-40 space-y-2 overflow-y-auto">
        {sortViolationsBySeverity(stackPolicies.recent_violations).map((violation) => (
          <Alert
            key={violation.id}
            variant={
              violation.severity === 'critical' || violation.severity === 'high'
                ? 'destructive'
                : 'default'
            }
            className="py-2"
          >
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle className="flex items-center gap-2 text-sm">
              {violation.policy_name}
              <Badge
                variant="outline"
                className={`text-xs ${
                  violation.severity === 'critical' || violation.severity === 'high'
                    ? 'border-destructive text-destructive'
                    : violation.severity === 'medium'
                    ? 'border-warning text-warning'
                    : 'border-muted text-muted-foreground'
                }`}
              >
                {violation.severity}
              </Badge>
            </AlertTitle>
            <AlertDescription className="mt-1 text-xs">
              {violation.message}
              <span className="mt-1 block text-muted-foreground">
                {formatDistanceToNow(parseISO(violation.detected_at), { addSuffix: true })}
              </span>
            </AlertDescription>
          </Alert>
        ))}
      </div>
    </div>
  ) : null;

const PolicyComplianceCard = ({
  stackPolicies,
  loadingPolicies,
}: {
  stackPolicies?: StackPoliciesResponse;
  loadingPolicies: boolean;
}) => (
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
          <ComplianceSummary stackPolicies={stackPolicies} />
          <PolicyAssignments stackPolicies={stackPolicies} />
          <PolicyViolations stackPolicies={stackPolicies} />
        </div>
      ) : (
        <p className="text-sm text-muted-foreground">Unable to load policy information</p>
      )}
    </CardContent>
  </Card>
);

const SessionRow = ({
  session,
  stackId,
  onClose,
  navigateToChat,
}: {
  session: ReturnType<typeof useChatSessions>['sessions'][number];
  stackId: string;
  onClose: () => void;
  navigateToChat: (path: string) => void;
}) => (
  <div
    className="flex cursor-pointer items-center justify-between rounded-lg border p-2 hover:bg-muted/50"
    onClick={() => {
      onClose();
      navigateToChat(`/chat?stack=${stackId}&session=${session.id}`);
    }}
  >
    <div className="min-w-0 flex-1">
      <p className="truncate text-sm font-medium">{session.name}</p>
      <p className="text-xs text-muted-foreground">
        {session.messages.length} message{session.messages.length !== 1 ? 's' : ''}
      </p>
    </div>
    <Button variant="ghost" size="sm">
      Open
    </Button>
  </div>
);

const HistoryEntry = ({ event }: { event: LifecycleHistoryEvent }) => {
  const config =
    lifecycleStateConfig[event.lifecycle_state.toLowerCase()] || lifecycleStateConfig.active;
  const prevConfig = event.previous_lifecycle_state
    ? lifecycleStateConfig[event.previous_lifecycle_state.toLowerCase()] || lifecycleStateConfig.active
    : null;

  return (
    <div className="space-y-2 rounded-lg border p-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          {prevConfig && (
            <>
              <Badge variant={prevConfig.variant} className={`text-xs ${prevConfig.className}`}>
                {event.previous_lifecycle_state?.charAt(0).toUpperCase() +
                  (event.previous_lifecycle_state?.slice(1) ?? '')}
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
        {event.reason && <div className="mt-1">Reason: {event.reason}</div>}
        <div className="mt-1 text-xs">{new Date(event.created_at).toLocaleString()}</div>
      </div>
    </div>
  );
};

const UsedInCard = ({
  sessions,
  stackId,
  onClose,
  navigateToChat,
}: {
  sessions: ReturnType<typeof useChatSessions>['sessions'];
  stackId: string;
  onClose: () => void;
  navigateToChat: (path: string) => void;
}) => (
  <Card>
    <CardHeader>
      <CardTitle className="flex items-center gap-2">
        <MessageSquare className="h-5 w-5" />
        Used In
      </CardTitle>
    </CardHeader>
    <CardContent>
      <div className="space-y-3">
        <div>
          <div className="mb-2 flex items-center justify-between">
            <span className="text-sm font-medium">Chat Sessions</span>
            <Badge variant="outline">{sessions.length}</Badge>
          </div>
          {sessions.length > 0 ? (
            <div className="space-y-1">
              {sessions.slice(0, 5).map((session) => (
                <SessionRow
                  key={session.id}
                  session={session}
                  stackId={stackId}
                  onClose={onClose}
                  navigateToChat={navigateToChat}
                />
              ))}
              {sessions.length > 5 && (
                <p className="text-center text-xs text-muted-foreground">
                  +{sessions.length - 5} more session{sessions.length - 5 !== 1 ? 's' : ''}
                </p>
              )}
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">No active chat sessions</p>
          )}
        </div>
        <div className="border-t pt-2">
          <Button
            variant="outline"
            size="sm"
            className="w-full"
            onClick={() => {
              onClose();
              navigateToChat(`/chat?stack=${stackId}`);
            }}
          >
            <MessageSquare className="mr-2 h-4 w-4" />
            Use in Chat
          </Button>
        </div>
      </div>
    </CardContent>
  </Card>
);

const HistoryCard = ({
  history,
  loadingHistory,
}: {
  history: LifecycleHistoryEvent[];
  loadingHistory: boolean;
}) => (
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
          {history.map((event) => (
            <HistoryEntry key={event.id} event={event} />
          ))}
        </div>
      )}
    </CardContent>
  </Card>
);

export function StackDetailModal({ stack, open, onClose }: StackDetailModalProps) {
  const navigate = useNavigate();
  const { selectedTenant } = useTenant();
  const [history, setHistory] = useState<LifecycleHistoryEvent[]>([]);
  const [loadingHistory, setLoadingHistory] = useState(false);
  const [isActivating, setIsActivating] = useState(false);
  const [isDeactivating, setIsDeactivating] = useState(false);
  const [showPreflightDialog, setShowPreflightDialog] = useState(false);
  const [preflightData, setPreflightData] = useState<PolicyPreflightResponse | null>(null);
  const clearStackAdaptersMutation = useClearStackAdapters();

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

  // Fetch stack policies
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

  const handleClearAdapters = async () => {
    if (stack.adapters.length === 0) {
      toast.info('Stack has no adapters to clear');
      return;
    }

    try {
      await clearStackAdaptersMutation.mutateAsync(stack.id);
      onClose(); // Close modal to refresh data
    } catch (error) {
      // Error handling is done in the mutation hook
      logger.error('Failed to clear stack adapters', {
        component: 'StackDetailModal',
        stackId: stack.id,
      }, error instanceof Error ? error : new Error('Unknown error'));
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
    <>
      <Modal
        open={open}
        onOpenChange={onClose}
        title={`Adapter Stack: ${stack.name}`}
        description={
          <>
            Stack ID: <span className="font-mono">{stack.id}</span>
          </>
        }
        header={
          <StackHeader
            stack={stack}
            isActive={isActive}
            isDeactivating={isDeactivating}
            isActivating={isActivating}
            onDeactivate={handleDeactivate}
            onActivate={handleActivate}
            onUseInChat={() => {
              onClose();
              navigate(`/chat?stack=${stack.id}`);
            }}
            onClearAdapters={handleClearAdapters}
            isClearingAdapters={clearStackAdaptersMutation.isPending}
          />
        }
        size="xl"
        className="max-h-[80vh] max-w-2xl overflow-y-auto"
      >
        <div className="space-y-4">
          <MemoryWarnings warnings={memoryWarnings} />
          <GeneralInfoCard stack={stack} />
          <AdaptersCard stack={stack} />
          <MemoryUsageCard
            adapterIds={adapterIds}
            availableAdapters={availableAdapters}
            capacity={capacity}
          />
          <PolicyComplianceCard stackPolicies={stackPolicies} loadingPolicies={loadingPolicies} />
          <UsedInCard
            sessions={sessionsUsingStack}
            stackId={stack.id}
            onClose={onClose}
            navigateToChat={(path) => navigate(path)}
          />
          <HistoryCard history={history} loadingHistory={loadingHistory} />
        </div>
      </Modal>

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
    </>
  );
}
