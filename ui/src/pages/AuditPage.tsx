// Audit Page - Security and system audit events with RBAC and real-time polling
import React, { useState, useEffect, useCallback } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { apiClient } from '@/api/client';
import { TelemetryEvent } from '@/api/types';
import { DensityProvider, useDensity } from '@/contexts/DensityContext';
import { DensityControls } from '@/components/ui/density-controls';
import { AdvancedFilter, type FilterConfig, type FilterValues } from '@/components/ui/advanced-filter';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { HelpTooltip } from '@/components/ui/help-tooltip';
import { usePolling } from '@/hooks/usePolling';
import { Download, RefreshCw, ChevronLeft, ChevronRight } from 'lucide-react';

function AuditPageInner() {
  const { density, setDensity } = useDensity();
  const { can, userRole } = useRBAC();
  const [auditLogs, setAuditLogs] = useState<TelemetryEvent[]>([]);
  const [allAuditLogs, setAllAuditLogs] = useState<TelemetryEvent[]>([]);
  const [limit, setLimit] = useState(50);
  const [offset, setOffset] = useState(0);

  // Filtering state
  const [filterValues, setFilterValues] = useState<FilterValues>({});

  // RBAC: Check if user has audit:view permission
  if (!can('audit:view')) {
    return (
      <FeatureLayout title="Audit Log" description="Security and system audit events">
        <ErrorRecovery
          error="You do not have permission to view audit logs. This page requires the audit:view permission (Admin, SRE, or Compliance role)."
          onRetry={() => window.location.reload()}
        />
      </FeatureLayout>
    );
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
      label: 'Organization ID',
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

  const formatTimestamp = (timestamp: string) => {
    return new Date(timestamp).toLocaleString();
  };

  const getSeverityColor = (level: string) => {
    switch (level?.toLowerCase()) {
      case 'error': return 'destructive';
      case 'warn': case 'warning': return 'secondary';
      case 'info': return 'default';
      case 'debug': return 'outline';
      default: return 'default';
    }
  };

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

  return (
    <FeatureLayout
      title="Audit Log"
      description="Security and system audit events"
      headerActions={<DensityControls density={density} onDensityChange={setDensity} />}
    >
      <div className="space-y-6">
        {/* Advanced Filters */}
        <AdvancedFilter
          configs={auditFilterConfigs}
          values={filterValues}
          onChange={setFilterValues}
          className="mb-4"
          title="Filter Audit Logs"
        />

        {/* Basic Controls */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              Controls
              <HelpTooltip helpId="audit-controls">
                <span className="cursor-help text-muted-foreground">(?)</span>
              </HelpTooltip>
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex gap-4 items-center flex-wrap">
              <div className="flex items-center gap-2">
                <HelpTooltip helpId="audit-items-per-page">
                  <label className="text-sm font-medium cursor-help">Items per page:</label>
                </HelpTooltip>
                <Select value={limit.toString()} onValueChange={(value) => setLimit(parseInt(value))}>
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
              <HelpTooltip helpId="audit-refresh">
                <Button onClick={loadAuditLogs} disabled={loading} variant="outline">
                  <RefreshCw className={`h-4 w-4 mr-2 ${loading ? 'animate-spin' : ''}`} />
                  Refresh
                </Button>
              </HelpTooltip>
              <HelpTooltip helpId="audit-export">
                <Button
                  onClick={handleExportLogs}
                  disabled={!can('audit:view') || allAuditLogs.length === 0}
                  variant="outline"
                >
                  <Download className="h-4 w-4 mr-2" />
                  Export
                </Button>
              </HelpTooltip>
              {lastUpdated && (
                <span className="text-xs text-muted-foreground">
                  Last updated: {lastUpdated.toLocaleTimeString()}
                </span>
              )}
            </div>
          </CardContent>
        </Card>

        {/* Audit Logs Table */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              Audit Events
              <HelpTooltip helpId="audit-events">
                <span className="cursor-help text-muted-foreground">(?)</span>
              </HelpTooltip>
              {filteredAuditLogs.length !== allAuditLogs.length && (
                <span className="ml-2 text-sm font-normal text-muted-foreground">
                  ({filteredAuditLogs.length} of {allAuditLogs.length} total)
                </span>
              )}
              {filteredAuditLogs.length === allAuditLogs.length && allAuditLogs.length > 0 && (
                <span className="ml-2 text-sm font-normal text-muted-foreground">
                  ({allAuditLogs.length} total)
                </span>
              )}
            </CardTitle>
          </CardHeader>
          <CardContent>
            {error && errorRecoveryTemplates.genericError(error, loadAuditLogs)}

            {loading && allAuditLogs.length === 0 ? (
              <div className="flex justify-center py-8">
                <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
              </div>
            ) : filteredAuditLogs.length === 0 ? (
              <div className="text-center py-8 text-muted-foreground">
                {allAuditLogs.length === 0 ? 'No audit events found' : 'No audit events match the current filters'}
              </div>
            ) : auditLogs.length === 0 ? (
              <div className="text-center py-8 text-muted-foreground">
                No results on this page
              </div>
            ) : (
              <div className="overflow-x-auto">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>
                        <HelpTooltip helpId="audit-timestamp">
                          <div className="flex items-center gap-1 cursor-help">
                            Timestamp
                          </div>
                        </HelpTooltip>
                      </TableHead>
                      <TableHead>
                        <HelpTooltip helpId="audit-level">
                          <div className="flex items-center gap-1 cursor-help">
                            Level
                          </div>
                        </HelpTooltip>
                      </TableHead>
                      <TableHead>
                        <HelpTooltip helpId="audit-event">
                          <div className="flex items-center gap-1 cursor-help">
                            Event
                          </div>
                        </HelpTooltip>
                      </TableHead>
                      <TableHead>
                        <HelpTooltip helpId="audit-user">
                          <div className="flex items-center gap-1 cursor-help">
                            User
                          </div>
                        </HelpTooltip>
                      </TableHead>
                      <TableHead>
                        <HelpTooltip helpId="audit-details">
                          <div className="flex items-center gap-1 cursor-help">
                            Details
                          </div>
                        </HelpTooltip>
                      </TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {auditLogs.map((log, index) => (
                      <TableRow key={index}>
                        <TableCell className="font-mono text-sm">
                          {formatTimestamp(log.timestamp)}
                        </TableCell>
                        <TableCell>
                          <Badge variant={getSeverityColor(log.level)}>
                            {log.level?.toUpperCase()}
                          </Badge>
                        </TableCell>
                        <TableCell className="font-medium">
                          {log.event_type || 'Unknown'}
                        </TableCell>
                        <TableCell>
                          {log.user_id || 'System'}
                        </TableCell>
                        <TableCell className="max-w-md truncate">
                          {log.metadata
                            ? JSON.stringify(log.metadata)
                            : 'No metadata'
                          }
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>
            )}

            {/* Pagination */}
            {filteredAuditLogs.length > limit && (
              <div className="flex justify-between items-center mt-4">
                <HelpTooltip helpId="audit-pagination-prev">
                  <Button
                    variant="outline"
                    onClick={() => setOffset(Math.max(0, offset - limit))}
                    disabled={offset === 0}
                  >
                    <ChevronLeft className="h-4 w-4 mr-1" />
                    Previous
                  </Button>
                </HelpTooltip>
                <span className="text-sm text-muted-foreground">
                  Showing {offset + 1} - {Math.min(offset + limit, filteredAuditLogs.length)} of {filteredAuditLogs.length}
                </span>
                <HelpTooltip helpId="audit-pagination-next">
                  <Button
                    variant="outline"
                    onClick={() => setOffset(offset + limit)}
                    disabled={offset + limit >= filteredAuditLogs.length}
                  >
                    Next
                    <ChevronRight className="h-4 w-4 ml-1" />
                  </Button>
                </HelpTooltip>
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </FeatureLayout>
  );
}

export default function AuditPage() {
  return (
    <DensityProvider pageKey="audit">
      <AuditPageInner />
    </DensityProvider>
  );
}
