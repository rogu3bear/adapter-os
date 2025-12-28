// Audit Page - Security and system audit events with RBAC and real-time polling
import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import PageWrapper from '@/layout/PageWrapper';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { apiClient } from '@/api/services';
import { PolicyAuditChainVerification, PolicyAuditDecision, TelemetryEvent } from '@/api/types';
import { useDensity } from '@/contexts/DensityContext';
import { DensityControls } from '@/components/ui/density-controls';
import { AdvancedFilter, type FilterConfig, type FilterValues } from '@/components/ui/advanced-filter';
import { useRBAC } from '@/hooks/security/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { PermissionDenied } from '@/components/ui/permission-denied';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { usePolling } from '@/hooks/realtime/usePolling';
import { Download, RefreshCw, ChevronLeft, ChevronRight } from 'lucide-react';
import { formatTimestamp } from '@/lib/formatters';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { Link } from 'react-router-dom';
import PageTable from '@/components/ui/PageTable';
import { toast } from 'sonner';
import { buildReplayLink } from '@/utils/navLinks';
import { cn } from '@/lib/utils';

type BadgeVariant = NonNullable<React.ComponentProps<typeof Badge>['variant']>;

const LoadingSpinner = () => (
  <div className="flex justify-center py-8">
    <div className="h-8 w-8 animate-spin rounded-full border-b-2 border-primary"></div>
  </div>
);

const hashPreview = (value?: string | null) => {
  if (!value) return '—';
  return `${value.substring(0, 12)}…`;
};

function PermissionDeniedView() {
  return (
    <PageWrapper pageKey="audit-log" title="Audit Log" description="Security and system audit events">
      <PermissionDenied
        requiredPermission="audit:view"
        requiredRoles={['admin', 'sre', 'compliance', 'developer', 'auditor']}
      />
    </PageWrapper>
  );
}

function ItemsPerPageSelect({
  limit,
  onChange,
}: {
  limit: number;
  onChange: (value: number) => void;
}) {
  return (
    <div className="flex items-center gap-2">
      <GlossaryTooltip termId="audit-items-per-page">
        <label className="cursor-help text-sm font-medium">Items per page:</label>
      </GlossaryTooltip>
      <Select value={limit.toString()} onValueChange={(value) => onChange(parseInt(value))}>
        <SelectTrigger className="w-24">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="25">25</SelectItem>
          <SelectItem value="50">50</SelectItem>
          <SelectItem value="100">100</SelectItem>
          <SelectItem value="200">200</SelectItem>
        </SelectContent>
      </Select>
    </div>
  );
}

function ControlsCard({
  limit,
  onLimitChange,
  onRefresh,
  loading,
  onExport,
  canExport,
  lastUpdated,
}: {
  limit: number;
  onLimitChange: (value: number) => void;
  onRefresh: () => void;
  loading: boolean;
  onExport: () => void;
  canExport: boolean;
  lastUpdated?: Date;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          Controls
          <GlossaryTooltip termId="audit-controls">
            <span className="cursor-help text-muted-foreground">(?)</span>
          </GlossaryTooltip>
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="flex flex-wrap items-center gap-4">
          <ItemsPerPageSelect limit={limit} onChange={onLimitChange} />
          <GlossaryTooltip termId="audit-refresh">
            <Button onClick={onRefresh} disabled={loading} variant="outline">
              <RefreshCw className={`h-4 w-4 mr-2 ${loading ? 'animate-spin' : ''}`} />
              Refresh
            </Button>
          </GlossaryTooltip>
          <GlossaryTooltip termId="audit-export">
            <Button onClick={onExport} disabled={!canExport} variant="outline">
              <Download className="h-4 w-4 mr-2" />
              Export
            </Button>
          </GlossaryTooltip>
          {lastUpdated && (
            <span className="text-xs text-muted-foreground">
              Last updated: {lastUpdated.toLocaleTimeString()}
            </span>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

const TableHeaderCell = ({ termId, children }: { termId: string; children: React.ReactNode }) => (
  <TableHead>
    <GlossaryTooltip termId={termId}>
      <div className="flex cursor-help items-center gap-1">{children}</div>
    </GlossaryTooltip>
  </TableHead>
);

function AuditTableRow({
  log,
  getSeverityColor,
}: {
  log: TelemetryEvent;
  getSeverityColor: (level: string) => BadgeVariant;
}) {
  return (
    <TableRow>
      <TableCell className="font-mono text-sm">{formatTimestamp(log.timestamp, 'long')}</TableCell>
      <TableCell>
        <Badge variant={getSeverityColor(log.level ?? '')}>{(log.level ?? '').toUpperCase()}</Badge>
      </TableCell>
      <TableCell className="font-medium">{log.event_type || 'Unknown'}</TableCell>
      <TableCell>{log.user_id || 'System'}</TableCell>
      <TableCell className="max-w-md truncate">
        {log.metadata ? JSON.stringify(log.metadata) : 'No metadata'}
      </TableCell>
    </TableRow>
  );
}

function AuditTableContent({
  auditLogs,
  getSeverityColor,
}: {
  auditLogs: TelemetryEvent[];
  getSeverityColor: (level: string) => BadgeVariant;
}) {
  return (
    <PageTable minWidth="md">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHeaderCell termId="audit-timestamp">Timestamp</TableHeaderCell>
            <TableHeaderCell termId="audit-level">Level</TableHeaderCell>
            <TableHeaderCell termId="audit-event">Event</TableHeaderCell>
            <TableHeaderCell termId="audit-user">User</TableHeaderCell>
            <TableHeaderCell termId="audit-details">Details</TableHeaderCell>
          </TableRow>
        </TableHeader>
        <TableBody>
          {auditLogs.map((log, index) => (
            <AuditTableRow key={index} log={log} getSeverityColor={getSeverityColor} />
          ))}
        </TableBody>
      </Table>
    </PageTable>
  );
}

function PaginationControls({
  total,
  limit,
  offset,
  onOffsetChange,
}: {
  total: number;
  limit: number;
  offset: number;
  onOffsetChange: (value: number) => void;
}) {
  const prev = () => onOffsetChange(Math.max(0, offset - limit));
  const next = () => onOffsetChange(offset + limit);
  return (
    <div className="mt-4 flex items-center justify-between">
      <GlossaryTooltip termId="audit-pagination-prev">
        <Button variant="outline" onClick={prev} disabled={offset === 0}>
          <ChevronLeft className="mr-1 h-4 w-4" />
          Previous
        </Button>
      </GlossaryTooltip>
      <span className="text-sm text-muted-foreground">
        Showing {offset + 1} - {Math.min(offset + limit, total)} of {total}
      </span>
      <GlossaryTooltip termId="audit-pagination-next">
        <Button variant="outline" onClick={next} disabled={offset + limit >= total}>
          Next
          <ChevronRight className="ml-1 h-4 w-4" />
        </Button>
      </GlossaryTooltip>
    </div>
  );
}

function AuditTableCard({
  auditLogs,
  filteredAuditLogs,
  allAuditLogs,
  loading,
  error,
  onRetry,
  getSeverityColor,
  limit,
  offset,
  onOffsetChange,
}: {
  auditLogs: TelemetryEvent[];
  filteredAuditLogs: TelemetryEvent[];
  allAuditLogs: TelemetryEvent[];
  loading: boolean;
  error: string | null;
  onRetry: () => void;
  getSeverityColor: (level: string) => BadgeVariant;
  limit: number;
  offset: number;
  onOffsetChange: (value: number) => void;
}) {
  const showCounts = filteredAuditLogs.length !== allAuditLogs.length && filteredAuditLogs.length > 0;
  const showTotalOnly = filteredAuditLogs.length === allAuditLogs.length && allAuditLogs.length > 0;
  const hasPagination = filteredAuditLogs.length > limit;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          Audit Events
          <GlossaryTooltip termId="audit-events">
            <span className="cursor-help text-muted-foreground">(?)</span>
          </GlossaryTooltip>
          {showCounts && (
            <span className="ml-2 text-sm font-normal text-muted-foreground">
              ({filteredAuditLogs.length} of {allAuditLogs.length} total)
            </span>
          )}
          {showTotalOnly && (
            <span className="ml-2 text-sm font-normal text-muted-foreground">
              ({allAuditLogs.length} total)
            </span>
          )}
        </CardTitle>
      </CardHeader>
      <CardContent>
        {error && errorRecoveryTemplates.genericError(error, onRetry)}
        {loading && allAuditLogs.length === 0 && <LoadingSpinner />}
        {!loading && filteredAuditLogs.length === 0 && (
          <div className="py-8 text-center text-muted-foreground">
            {allAuditLogs.length === 0 ? 'No audit events found' : 'No audit events match the current filters'}
          </div>
        )}
        {!loading && filteredAuditLogs.length > 0 && auditLogs.length === 0 && (
          <div className="py-8 text-center text-muted-foreground">No results on this page</div>
        )}
        {!loading && auditLogs.length > 0 && (
          <>
            <AuditTableContent auditLogs={auditLogs} getSeverityColor={getSeverityColor} />
            {hasPagination && (
              <PaginationControls
                total={filteredAuditLogs.length}
                limit={limit}
                offset={offset}
                onOffsetChange={onOffsetChange}
              />
            )}
          </>
        )}
      </CardContent>
    </Card>
  );
}

function AuditPageInner() {
  const { density, setDensity } = useDensity();
  const { can } = useRBAC();
  const [auditLogs, setAuditLogs] = useState<TelemetryEvent[]>([]);
  const [allAuditLogs, setAllAuditLogs] = useState<TelemetryEvent[]>([]);
  const [limit, setLimit] = useState(50);
  const [offset, setOffset] = useState(0);
  const [policyDecisions, setPolicyDecisions] = useState<PolicyAuditDecision[]>([]);
  const [chainStatus, setChainStatus] = useState<PolicyAuditChainVerification | null>(null);
  const [chainLoading, setChainLoading] = useState(false);
  const [chainError, setChainError] = useState<string | null>(null);
  const [lastChainUpdated, setLastChainUpdated] = useState<Date | undefined>(undefined);
  const [diverging, setDiverging] = useState(false);
  const isE2EMode = import.meta.env.VITE_E2E_MODE === '1';
  const prevChainValid = useRef<boolean | null>(null);
  const [highlightedSeq, setHighlightedSeq] = useState<number | null>(null);
  const highlightTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Scroll to and highlight the first broken entry when user clicks toast action
  const scrollToFirstBrokenEntry = useCallback(() => {
    if (!chainStatus?.broken_links?.length) return;

    // Clear any existing timeout
    if (highlightTimeoutRef.current) {
      clearTimeout(highlightTimeoutRef.current);
    }

    const firstBrokenSeq = chainStatus.broken_links[0].sequence;

    // Find the row by data-seq attribute
    const rowElement = document.querySelector(
      `[data-cy="policy-audit-row"][data-seq="${firstBrokenSeq}"]`
    );

    if (rowElement) {
      rowElement.scrollIntoView({ behavior: 'smooth', block: 'center' });
      setHighlightedSeq(firstBrokenSeq);

      // Clear highlight after 3 seconds
      highlightTimeoutRef.current = setTimeout(() => {
        setHighlightedSeq(null);
      }, 3000);
    }
  }, [chainStatus]);

  // Alert when chain transitions from healthy to diverged
  useEffect(() => {
    if (chainStatus === null) return;

    const wasValid = prevChainValid.current;
    const isValid = chainStatus.valid;

    // Detect transition from healthy (true) to diverged (false)
    if (wasValid === true && isValid === false) {
      toast.error('Policy audit chain diverged! Integrity violation detected.', {
        duration: 10000,
        action: {
          label: 'Jump to Issue',
          onClick: scrollToFirstBrokenEntry,
        },
      });
    }

    prevChainValid.current = isValid;
  }, [chainStatus, scrollToFirstBrokenEntry]);

  // Cleanup timeout on unmount
  useEffect(() => {
    return () => {
      if (highlightTimeoutRef.current) {
        clearTimeout(highlightTimeoutRef.current);
      }
    };
  }, []);

  // Filtering state
  const [filterValues, setFilterValues] = useState<FilterValues>({});

  // RBAC: Check if user has audit:view permission
  if (!can('audit:view')) {
    return <PermissionDeniedView />;
  }

  // Filter configurations for audit logs
  const auditFilterConfigs: FilterConfig[] = [
    {
      id: 'search',
      label: 'Search',
      type: 'text',
      placeholder: 'Search event type, user, or metadata...',
    },
    {
      id: 'level',
      label: 'Log Level',
      type: 'multiSelect',
      options: [
        { value: 'debug', label: 'Debug' },
        { value: 'info', label: 'Info' },
        { value: 'warn', label: 'Warning' },
        { value: 'error', label: 'Error' },
        { value: 'critical', label: 'Critical' },
      ],
    },
    {
      id: 'eventType',
      label: 'Event Type',
      type: 'text',
      placeholder: 'Filter by event type...',
    },
    {
      id: 'userId',
      label: 'User ID',
      type: 'text',
      placeholder: 'Filter by user ID...',
    },
    {
      id: 'tenantId',
      label: 'Workspace ID',
      type: 'text',
      placeholder: 'Filter by organization ID...',
    },
    {
      id: 'component',
      label: 'Component',
      type: 'text',
      placeholder: 'Filter by component...',
    },
    {
      id: 'traceId',
      label: 'Trace ID',
      type: 'text',
      placeholder: 'Filter by trace ID...',
    },
    {
      id: 'dateRange',
      label: 'Timestamp Range',
      type: 'dateRange',
    },
  ];

  // Filter audit logs based on filter values
  const filteredAuditLogs = allAuditLogs.filter(log => {
    // Search filter
    if (filterValues.search) {
      const searchLower = String(filterValues.search).toLowerCase();
      const matchesSearch =
        (log.event_type?.toLowerCase().includes(searchLower)) ||
        (log.user_id?.toLowerCase().includes(searchLower)) ||
        (log.tenant_id?.toLowerCase().includes(searchLower)) ||
        (log.component?.toLowerCase().includes(searchLower)) ||
        (log.trace_id && String(log.trace_id).toLowerCase().includes(searchLower)) ||
        (log.metadata && JSON.stringify(log.metadata).toLowerCase().includes(searchLower));

      if (!matchesSearch) {
        return false;
      }
    }

    // Level filter (multi-select)
    if (filterValues.level && Array.isArray(filterValues.level) && filterValues.level.length > 0) {
      if (!filterValues.level.includes(log.level?.toLowerCase() || '')) {
        return false;
      }
    }

    // Event type filter
    if (filterValues.eventType && log.event_type !== filterValues.eventType) {
      return false;
    }

    // User ID filter
    if (filterValues.userId && log.user_id !== filterValues.userId) {
      return false;
    }

    // Tenant ID filter
    if (filterValues.tenantId && log.tenant_id !== filterValues.tenantId) {
      return false;
    }

    // Component filter
    if (filterValues.component && log.component !== filterValues.component) {
      return false;
    }

    // Trace ID filter
    if (filterValues.traceId && log.trace_id && String(log.trace_id) !== filterValues.traceId) {
      return false;
    }

    // Date range filter
    if (filterValues.dateRange && typeof filterValues.dateRange === 'object') {
      const range = filterValues.dateRange as { start?: string; end?: string };
      const logDate = new Date(log.timestamp);
      if (range.start && logDate < new Date(range.start)) {
        return false;
      }
      if (range.end) {
        const endDate = new Date(range.end);
        endDate.setHours(23, 59, 59, 999); // Include entire end day
        if (logDate > endDate) {
          return false;
        }
      }
    }

    return true;
  });

  // Use polling for real-time audit log updates
  const fetchAuditLogs = useCallback(async () => {
    const logs = await apiClient.getTelemetryLogs({
      category: 'audit',
      limit: 500, // Load more for filtering
      offset: 0,
    });
    return logs;
  }, []);

  const {
    data: polledLogs,
    isLoading: loading,
    error: pollingError,
    refetch: loadAuditLogs,
    lastUpdated
  } = usePolling(
    fetchAuditLogs,
    'slow', // Audit logs update slowly (30s)
    {
      enabled: true,
      operationName: 'fetchAuditLogs',
      onSuccess: (data) => {
        setAllAuditLogs(data as TelemetryEvent[]);
      },
    }
  );

  const error = pollingError?.message || null;

  // Update displayed logs when filters or pagination change
  useEffect(() => {
    const start = offset;
    const end = offset + limit;
    setAuditLogs(filteredAuditLogs.slice(start, end));
    // Reset offset if filtered results are less than current offset
    if (offset >= filteredAuditLogs.length && filteredAuditLogs.length > 0) {
      setOffset(0);
    }
  }, [filteredAuditLogs, offset, limit]);

  // Update allAuditLogs when polled data changes
  useEffect(() => {
    if (polledLogs) {
      setAllAuditLogs(polledLogs);
    }
  }, [polledLogs]);

  const severityVariantMap: Record<string, BadgeVariant> = {
    critical: 'destructive',
    error: 'error',
    warn: 'warning',
    warning: 'warning',
    info: 'info',
    debug: 'outline',
  };

  const getSeverityColor = (level: string): BadgeVariant =>
    severityVariantMap[level?.toLowerCase()] ?? 'default';

  // Export audit logs as JSON
  const handleExportLogs = useCallback(() => {
    const dataToExport = filteredAuditLogs.length > 0 ? filteredAuditLogs : allAuditLogs;
    const blob = new Blob([JSON.stringify(dataToExport, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `audit-logs-${new Date().toISOString().split('T')[0]}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }, [filteredAuditLogs, allAuditLogs]);

  const fetchPolicyAuditChain = useCallback(async () => {
    setChainLoading(true);
    try {
      const [decisions, verification] = await Promise.all([
        apiClient.getPolicyAuditDecisions({ limit: 200 }),
        apiClient.verifyPolicyAuditChain(),
      ]);
      const ordered = decisions.slice().sort((a, b) => a.chain_sequence - b.chain_sequence);
      setPolicyDecisions(ordered);
      setChainStatus(verification);
      setChainError(null);
      setLastChainUpdated(new Date());
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to load policy audit chain';
      setChainError(message);
      toast.error(message);
    } finally {
      setChainLoading(false);
    }
  }, []);

  // Fetch chain status on mount and poll every 30 seconds for real-time integrity monitoring
  // Only polls when tab is visible to save resources
  useEffect(() => {
    fetchPolicyAuditChain();

    const CHAIN_POLL_INTERVAL_MS = 30000; // 30 seconds
    let intervalId: ReturnType<typeof setInterval> | null = null;

    const startPolling = () => {
      if (!intervalId) {
        intervalId = setInterval(() => {
          fetchPolicyAuditChain();
        }, CHAIN_POLL_INTERVAL_MS);
      }
    };

    const stopPolling = () => {
      if (intervalId) {
        clearInterval(intervalId);
        intervalId = null;
      }
    };

    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible') {
        // Refetch immediately when tab becomes visible, then resume polling
        fetchPolicyAuditChain();
        startPolling();
      } else {
        stopPolling();
      }
    };

    // Start polling if tab is visible
    if (document.visibilityState === 'visible') {
      startPolling();
    }

    document.addEventListener('visibilitychange', handleVisibilityChange);

    return () => {
      stopPolling();
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, [fetchPolicyAuditChain]);

  const handleTriggerDivergence = useCallback(async () => {
    if (!isE2EMode) {
      toast.error('E2E_MODE=1 required to force divergence');
      return;
    }
    setDiverging(true);
    try {
      await apiClient.triggerAuditDivergence();
      toast.error('Audit chain forcibly diverged for testing; actions should now fail closed.');
      await fetchPolicyAuditChain();
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to trigger audit divergence';
      toast.error(message);
    } finally {
      setDiverging(false);
    }
  }, [fetchPolicyAuditChain, isE2EMode]);

  const brokenSequences = useMemo(
    () => new Set(chainStatus?.broken_links.map((link) => link.sequence) ?? []),
    [chainStatus],
  );

  return (
    <PageWrapper
      pageKey="audit"
      title="Audit Log"
      description="Security and system audit events"
      contentPadding="default"
      maxWidth="xl"
      headerActions={
        <div className="flex items-center gap-2">
          <DensityControls density={density} onDensityChange={setDensity} />
          <Link to={buildReplayLink('runs')} className="text-xs underline underline-offset-4">
            Open related replay
          </Link>
        </div>
      }
    >
      <SectionErrorBoundary sectionName="Audit Log">
        <div className="space-y-6">
          <AdvancedFilter
            configs={auditFilterConfigs}
            values={filterValues}
            onChange={setFilterValues}
            className="mb-4"
            title="Filter Audit Logs"
          />
          <ControlsCard
            limit={limit}
            onLimitChange={setLimit}
            onRefresh={loadAuditLogs}
            loading={loading}
            onExport={handleExportLogs}
            canExport={can('audit:view') && allAuditLogs.length > 0}
            lastUpdated={lastUpdated ?? undefined}
          />
          <Card>
            <CardHeader>
              <CardTitle className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
                <div className="flex items-center gap-2">
                  Policy Audit Chain
                  <Badge variant={chainStatus?.valid === false ? 'destructive' : 'default'}>
                    {chainStatus?.valid === false ? 'Diverged' : 'Healthy'}
                  </Badge>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  {lastChainUpdated && (
                    <span className="text-xs text-muted-foreground">
                      Last checked: {lastChainUpdated.toLocaleTimeString()}
                    </span>
                  )}
                  <Button variant="outline" onClick={fetchPolicyAuditChain} disabled={chainLoading}>
                    <RefreshCw className={`mr-2 h-4 w-4 ${chainLoading ? 'animate-spin' : ''}`} />
                    Refresh chain
                  </Button>
                  {isE2EMode && (
                    <Button variant="destructive" onClick={handleTriggerDivergence} disabled={diverging}>
                      {diverging ? 'Forcing…' : 'Force divergence'}
                    </Button>
                  )}
                </div>
              </CardTitle>
            </CardHeader>
          <CardContent className="space-y-3" data-cy="audit-chain-status">
              {chainError && (
                <div className="text-sm text-destructive">
                  {chainError}
                </div>
              )}
              {chainStatus && (
                <div className="text-sm text-muted-foreground">
                  {chainStatus.valid
                    ? 'Chain verified'
                    : 'Chain verification failed'}{' '}
                  ({chainStatus.verified_entries} of {chainStatus.total_entries} linked){' '}
                  {chainStatus.broken_links.length > 0 && (
                    <span className="text-destructive">
                      • {chainStatus.broken_links.length} broken link(s) detected
                    </span>
                  )}
                </div>
              )}
              {chainStatus?.broken_links.length ? (
                <div className="rounded-md border border-destructive/40 bg-destructive/5 p-3 text-sm">
                  <div className="font-semibold text-destructive">Broken links</div>
                  <ul className="mt-2 space-y-1">
                    {chainStatus.broken_links.map((link) => (
                      <li key={`${link.sequence}-${link.entry_id}`} className="font-mono text-xs text-destructive">
                        seq {link.sequence}: expected {link.expected_hash.substring(0, 12)}… got {link.actual_hash.substring(0, 12)}…
                      </li>
                    ))}
                  </ul>
                </div>
              ) : null}
              <PageTable minWidth="lg">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Seq</TableHead>
                      <TableHead>Entry Hash</TableHead>
                      <TableHead>Previous Hash</TableHead>
                      <TableHead>Policy</TableHead>
                      <TableHead>Hook</TableHead>
                      <TableHead>Decision</TableHead>
                      <TableHead>Timestamp</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {policyDecisions.length === 0 ? (
                      <TableRow>
                        <TableCell colSpan={7} className="py-4 text-center text-sm text-muted-foreground">
                          No policy audit entries
                        </TableCell>
                      </TableRow>
                    ) : (
                      policyDecisions.map((entry) => (
                        <TableRow
                          key={entry.id}
                          className={cn(
                            brokenSequences.has(entry.chain_sequence) && 'bg-destructive/5',
                            highlightedSeq === entry.chain_sequence && 'ring-2 ring-destructive animate-pulse'
                          )}
                          data-cy="policy-audit-row"
                          data-seq={entry.chain_sequence}
                        >
                          <TableCell className="font-mono text-xs">{entry.chain_sequence}</TableCell>
                          <TableCell className="font-mono text-xs">{hashPreview(entry.entry_hash)}</TableCell>
                          <TableCell className="font-mono text-xs">{hashPreview(entry.previous_hash)}</TableCell>
                          <TableCell className="text-xs">{entry.policy_pack_id}</TableCell>
                          <TableCell className="text-xs">{entry.hook}</TableCell>
                          <TableCell>
                            <Badge variant={entry.decision === 'deny' ? 'destructive' : 'outline'}>
                              {entry.decision.toUpperCase()}
                            </Badge>
                          </TableCell>
                          <TableCell className="text-xs text-muted-foreground">
                            {formatTimestamp(entry.timestamp, 'long')}
                          </TableCell>
                        </TableRow>
                      ))
                    )}
                  </TableBody>
                </Table>
              </PageTable>
            </CardContent>
          </Card>
          <AuditTableCard
            auditLogs={auditLogs}
            filteredAuditLogs={filteredAuditLogs}
            allAuditLogs={allAuditLogs}
            loading={loading}
            error={error}
            onRetry={loadAuditLogs}
            getSeverityColor={getSeverityColor}
            limit={limit}
            offset={offset}
            onOffsetChange={setOffset}
          />
        </div>
      </SectionErrorBoundary>
    </PageWrapper>
  );
}

export default function AuditPage() {
  return <AuditPageInner />;
}
