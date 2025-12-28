/**
 * AuditLogsTab - Audit logs query and viewing interface
 *
 * Features:
 * - Query audit logs with filters
 * - Advanced filtering (action, user, resource, status, time range)
 * - Export audit logs
 * - Detailed log entry viewer
 */

import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Skeleton } from '@/components/ui/skeleton';
import { Badge } from '@/components/ui/badge';
import { DataTable } from '@/components/shared/DataTable/DataTable';
import type { ColumnDef } from '@/components/shared/DataTable/types';
import {
  Download,
  RefreshCw,
  Filter,
  ChevronDown,
  ChevronRight,
  CheckCircle,
  XCircle,
  AlertCircle,
} from 'lucide-react';
import { toast } from 'sonner';

import { useAuditLogs, useExportAuditLogs } from '@/hooks/security/useSecurity';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import type { AuditLog, AuditLogFilters } from '@/api/types';

export function AuditLogsTab() {
  const [filters, setFilters] = useState<AuditLogFilters>({
    limit: 50,
    offset: 0,
  });
  const [showFilters, setShowFilters] = useState(false);
  const [expandedRows, setExpandedRows] = useState<Set<string>>(new Set());

  const { auditLogs, isLoading, error, refetch } = useAuditLogs(filters);
  const { exportAuditLogs, isExporting } = useExportAuditLogs();

  const handleFilterChange = (key: keyof AuditLogFilters, value: string | undefined) => {
    setFilters((prev) => ({
      ...prev,
      [key]: value || undefined,
      offset: 0, // Reset to first page when filters change
    }));
  };

  const handleExport = async () => {
    try {
      // Convert AuditLogFilters to export API format
      const exportParams = {
        format: 'csv' as const,
        startTime: filters.start_time,
        endTime: filters.end_time,
        tenantId: filters.tenant_id,
      };
      const blob = await exportAuditLogs(exportParams);
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `audit-logs-${new Date().toISOString()}.csv`;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      URL.revokeObjectURL(url);
      toast.success('Audit logs exported');
    } catch (err) {
      toast.error('Failed to export audit logs');
    }
  };

  const toggleRowExpansion = (id: string) => {
    setExpandedRows((prev) => {
      const newSet = new Set(prev);
      if (newSet.has(id)) {
        newSet.delete(id);
      } else {
        newSet.add(id);
      }
      return newSet;
    });
  };

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'success':
        return <CheckCircle className="h-4 w-4 text-green-500" />;
      case 'failure':
        return <XCircle className="h-4 w-4 text-red-500" />;
      case 'error':
        return <AlertCircle className="h-4 w-4 text-yellow-500" />;
      default:
        return null;
    }
  };

  const getStatusBadge = (status: string) => {
    switch (status) {
      case 'success':
        return <Badge variant="default" className="bg-green-500">Success</Badge>;
      case 'failure':
        return <Badge variant="destructive">Failure</Badge>;
      case 'error':
        return <Badge variant="secondary">Error</Badge>;
      default:
        return <Badge variant="outline">{status}</Badge>;
    }
  };

  const columns: ColumnDef<AuditLog>[] = [
    {
      id: 'expand',
      header: '',
      cell: (context) => {
        const rowId = context.row.original.id;
        const isExpanded = expandedRows.has(rowId);
        return (
          <Button
            variant="ghost"
            size="sm"
            onClick={(e) => {
              e.stopPropagation();
              toggleRowExpansion(rowId);
            }}
          >
            {isExpanded ? (
              <ChevronDown className="h-4 w-4" />
            ) : (
              <ChevronRight className="h-4 w-4" />
            )}
          </Button>
        );
      },
    },
    {
      id: 'timestamp',
      header: 'Timestamp',
      accessorKey: 'timestamp',
      cell: (context) => {
        const timestamp = context.row.original.timestamp;
        return (
          <div className="text-sm">
            {new Date(timestamp).toLocaleString()}
          </div>
        );
      },
      enableSorting: true,
    },
    {
      id: 'user',
      header: 'User',
      accessorKey: 'user_id',
      enableSorting: true,
    },
    {
      id: 'action',
      header: 'Action',
      accessorKey: 'action',
      enableSorting: true,
    },
    {
      id: 'resource',
      header: 'Resource',
      accessorKey: 'resource',
      cell: (context) => {
        const log = context.row.original;
        return (
          <div className="space-y-1">
            <div className="text-sm font-medium">{log.resource}</div>
            {log.resource_id && (
              <div className="text-xs text-muted-foreground font-mono">
                {log.resource_id}
              </div>
            )}
          </div>
        );
      },
      enableSorting: true,
    },
    {
      id: 'status',
      header: 'Status',
      accessorKey: 'status',
      cell: (context) => getStatusBadge(context.row.original.status),
      enableSorting: true,
    },
  ];

  if (error) {
    return <ErrorRecovery error={error.message} onRetry={refetch} />;
  }

  return (
    <div className="space-y-6">
      {/* Action Bar */}
      <div className="flex items-center justify-between">
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setShowFilters(!showFilters)}
          >
            <Filter className="h-4 w-4 mr-2" />
            {showFilters ? 'Hide Filters' : 'Show Filters'}
          </Button>
          <Button variant="outline" size="sm" onClick={() => refetch()}>
            <RefreshCw className="h-4 w-4 mr-2" />
            Refresh
          </Button>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={handleExport}
          disabled={isExporting}
        >
          <Download className="h-4 w-4 mr-2" />
          {isExporting ? 'Exporting...' : 'Export CSV'}
        </Button>
      </div>

      {/* Filters Card */}
      {showFilters && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Filters</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div className="space-y-2">
                <Label htmlFor="action">Action</Label>
                <Input
                  id="action"
                  placeholder="e.g., adapter.register"
                  value={filters.action || ''}
                  onChange={(e) => handleFilterChange('action', e.target.value)}
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="user_id">User ID</Label>
                <Input
                  id="user_id"
                  placeholder="e.g., user@example.com"
                  value={filters.user_id || ''}
                  onChange={(e) => handleFilterChange('user_id', e.target.value)}
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="resource">Resource</Label>
                <Input
                  id="resource"
                  placeholder="e.g., adapter"
                  value={filters.resource || ''}
                  onChange={(e) => handleFilterChange('resource', e.target.value)}
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="status">Status</Label>
                <Select
                  value={filters.status || 'all'}
                  onValueChange={(value) =>
                    handleFilterChange('status', value === 'all' ? undefined : value)
                  }
                >
                  <SelectTrigger id="status">
                    <SelectValue placeholder="All statuses" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">All</SelectItem>
                    <SelectItem value="success">Success</SelectItem>
                    <SelectItem value="failure">Failure</SelectItem>
                    <SelectItem value="error">Error</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-2">
                <Label htmlFor="start_time">Start Time</Label>
                <Input
                  id="start_time"
                  type="datetime-local"
                  value={filters.start_time || ''}
                  onChange={(e) => handleFilterChange('start_time', e.target.value)}
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="end_time">End Time</Label>
                <Input
                  id="end_time"
                  type="datetime-local"
                  value={filters.end_time || ''}
                  onChange={(e) => handleFilterChange('end_time', e.target.value)}
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="limit">Limit</Label>
                <Select
                  value={String(filters.limit || 50)}
                  onValueChange={(value) => handleFilterChange('limit', value)}
                >
                  <SelectTrigger id="limit">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="25">25</SelectItem>
                    <SelectItem value="50">50</SelectItem>
                    <SelectItem value="100">100</SelectItem>
                    <SelectItem value="250">250</SelectItem>
                    <SelectItem value="500">500</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-2">
                <Label htmlFor="tenant_id">Workspace ID</Label>
                <Input
                  id="tenant_id"
                  placeholder="e.g., acme-corp"
                  value={filters.tenant_id || ''}
                  onChange={(e) => handleFilterChange('tenant_id', e.target.value)}
                />
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Audit Logs Table */}
      {isLoading ? (
        <Card>
          <CardContent className="p-6">
            <div className="space-y-4">
              {[1, 2, 3, 4, 5].map((i) => (
                <Skeleton key={i} className="h-12 w-full" />
              ))}
            </div>
          </CardContent>
        </Card>
      ) : (
        <Card>
          <CardContent className="p-0">
            <DataTable
              data={auditLogs || []}
              columns={columns}
              getRowId={(row) => row.id}
              enableSorting
              enablePagination
              pagination={{ pageIndex: 0, pageSize: 25 }}
              pageSizes={[25, 50, 100]}
              emptyTitle="No audit logs found"
              emptyDescription="No audit logs match the current filters."
              className="border-0"
            />

            {/* Expanded Row Details */}
            {auditLogs?.map((log) =>
              expandedRows.has(log.id) ? (
                <div
                  key={`expanded-${log.id}`}
                  className="border-t bg-muted/50 p-4 space-y-3"
                >
                  <div className="grid grid-cols-2 gap-4 text-sm">
                    {log.ip_address && (
                      <div>
                        <span className="font-medium">IP Address:</span> {log.ip_address}
                      </div>
                    )}
                    {log.user_agent && (
                      <div>
                        <span className="font-medium">User Agent:</span>{' '}
                        <span className="text-xs font-mono">{log.user_agent}</span>
                      </div>
                    )}
                    {log.session_id && (
                      <div>
                        <span className="font-medium">Session ID:</span>{' '}
                        <span className="font-mono">{log.session_id}</span>
                      </div>
                    )}
                    {log.tenant_id && (
                      <div>
                        <span className="font-medium">Workspace:</span> {log.tenant_id}
                      </div>
                    )}
                  </div>

                  {log.details && (
                    <div>
                      <span className="font-medium text-sm">Details:</span>
                      <pre className="mt-2 bg-background p-3 rounded-md text-xs font-mono overflow-x-auto">
                        {JSON.stringify(log.details, null, 2)}
                      </pre>
                    </div>
                  )}
                </div>
              ) : null
            )}
          </CardContent>
        </Card>
      )}

      {/* Results Summary */}
      {!isLoading && auditLogs && (
        <div className="text-sm text-muted-foreground text-center">
          Showing {auditLogs.length} audit log entries
        </div>
      )}
    </div>
  );
}
