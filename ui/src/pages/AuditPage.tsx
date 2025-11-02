// 【ui/src/contexts/DensityContext.tsx】 - Density context
import React, { useState, useEffect, useCallback } from 'react';
import { RequireAuth } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { apiClient } from '@/api/client';
import { TelemetryEvent } from '@/api/types';
import { DensityProvider, useDensity } from '@/contexts/DensityContext';
import { DensityControls } from '@/components/ui/density-controls';
import { AdvancedFilter, type FilterConfig, type FilterValues } from '@/components/ui/advanced-filter';

function AuditPageInner() {
  const { density, setDensity } = useDensity();
  const [auditLogs, setAuditLogs] = useState<TelemetryEvent[]>([]);
  const [allAuditLogs, setAllAuditLogs] = useState<TelemetryEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [limit, setLimit] = useState(50);
  const [offset, setOffset] = useState(0);
  
  // Filtering state
  const [filterValues, setFilterValues] = useState<FilterValues>({});
  
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
      label: 'Tenant ID',
      type: 'text',
      placeholder: 'Filter by tenant ID...',
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

  const loadAuditLogs = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      // Load more logs to enable client-side filtering
      const logs = await apiClient.getTelemetryLogs({
        category: 'audit',
        limit: 500, // Load more for filtering
        offset: 0,
      });
      setAllAuditLogs(logs);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load audit logs');
    } finally {
      setLoading(false);
    }
  }, []);
  
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

  useEffect(() => {
    loadAuditLogs();
  }, [loadAuditLogs]);

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

  return (
    <FeatureLayout 
      title="Audit Log" 
      description="Security and system audit events"
      right={<DensityControls density={density} onDensityChange={setDensity} />}
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
            <CardTitle>Controls</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex gap-4 items-center">
              <div className="flex items-center gap-2">
                <label className="text-sm font-medium">Items per page:</label>
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
              <Button onClick={loadAuditLogs} disabled={loading}>
                Refresh
              </Button>
            </div>
          </CardContent>
        </Card>

        {/* Audit Logs Table */}
        <Card>
          <CardHeader>
            <CardTitle>
              Audit Events
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
            {error && (
              <div className="text-red-600 mb-4 p-3 bg-red-50 rounded">
                {error}
              </div>
            )}

            {loading ? (
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
                      <TableHead>Timestamp</TableHead>
                      <TableHead>Level</TableHead>
                      <TableHead>Event</TableHead>
                      <TableHead>User</TableHead>
                      <TableHead>Details</TableHead>
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
                <Button
                  variant="outline"
                  onClick={() => setOffset(Math.max(0, offset - limit))}
                  disabled={offset === 0}
                >
                  Previous
                </Button>
                <span className="text-sm text-muted-foreground">
                  Showing {offset + 1} - {Math.min(offset + limit, filteredAuditLogs.length)} of {filteredAuditLogs.length}
                </span>
                <Button
                  variant="outline"
                  onClick={() => setOffset(offset + limit)}
                  disabled={offset + limit >= filteredAuditLogs.length}
                >
                  Next
                </Button>
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
