import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import {
  FileText,
  Download,
  RefreshCw,
  Search,
  Filter,
  Calendar,
  User,
  Activity,
  CheckCircle,
  XCircle,
  AlertCircle,
} from 'lucide-react';
import apiClient from '@/api/client';
import { AuditLog, AuditLogFilters } from '@/api/types';
import { toast } from 'sonner';
import { logger } from '@/utils/logger';

interface SecurityAuditTrailProps {
  tenantId?: string;
}

export default function SecurityAuditTrail({ tenantId }: SecurityAuditTrailProps) {
  const [logs, setLogs] = useState<AuditLog[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [filters, setFilters] = useState<AuditLogFilters>({
    limit: 50,
    tenant_id: tenantId,
  });

  // Filter state
  const [actionFilter, setActionFilter] = useState<string>('');
  const [userFilter, setUserFilter] = useState<string>('');
  const [resourceFilter, setResourceFilter] = useState<string>('');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [startDate, setStartDate] = useState<string>('');
  const [endDate, setEndDate] = useState<string>('');

  const loadAuditLogs = useCallback(async () => {
    setIsLoading(true);
    try {
      const queryFilters: AuditLogFilters = {
        ...filters,
        action: actionFilter || undefined,
        user_id: userFilter || undefined,
        resource: resourceFilter || undefined,
        status: statusFilter !== 'all' ? statusFilter : undefined,
        start_time: startDate ? new Date(startDate).toISOString() : undefined,
        end_time: endDate ? new Date(endDate).toISOString() : undefined,
      };

      const auditLogs = await apiClient.queryAuditLogs(queryFilters);
      setLogs(auditLogs);

      logger.info('Audit logs loaded', {
        component: 'SecurityAuditTrail',
        operation: 'loadAuditLogs',
        count: auditLogs.length,
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to load audit logs';
      logger.error('Failed to load audit logs', {
        component: 'SecurityAuditTrail',
        operation: 'loadAuditLogs',
        error: errorMessage,
      });
      toast.error(errorMessage);
    } finally {
      setIsLoading(false);
    }
  }, [filters, actionFilter, userFilter, resourceFilter, statusFilter, startDate, endDate]);

  useEffect(() => {
    loadAuditLogs();
  }, [loadAuditLogs]);

  const handleExport = async () => {
    try {
      logger.info('Exporting audit logs', {
        component: 'SecurityAuditTrail',
        operation: 'handleExport',
      });

      const blob = await apiClient.exportAuditLogs({
        format: 'json',
        startTime: startDate ? new Date(startDate).toISOString() : undefined,
        endTime: endDate ? new Date(endDate).toISOString() : undefined,
        tenantId: tenantId,
      });

      const timestamp = new Date().toISOString().slice(0, 19).replace(/:/g, '-');
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `audit-logs-${timestamp}.json`;
      a.click();
      URL.revokeObjectURL(url);

      toast.success('Audit logs exported successfully');
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to export';
      logger.error('Failed to export audit logs', {
        component: 'SecurityAuditTrail',
        operation: 'handleExport',
        error: errorMessage,
      });
      toast.error(errorMessage);
    }
  };

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'success':
        return <CheckCircle className="w-4 h-4 text-green-600" />;
      case 'failure':
        return <XCircle className="w-4 h-4 text-red-600" />;
      case 'error':
        return <AlertCircle className="w-4 h-4 text-amber-600" />;
      default:
        return <Activity className="w-4 h-4 text-muted-foreground" />;
    }
  };

  const getStatusBadge = (status: string) => {
    const variants: Record<string, string> = {
      success: 'bg-green-100 text-green-800',
      failure: 'bg-red-100 text-red-800',
      error: 'bg-amber-100 text-amber-800',
    };
    return (
      <Badge className={variants[status] || 'bg-gray-100 text-gray-800'}>
        {status}
      </Badge>
    );
  };

  const formatTimestamp = (timestamp: string) => {
    return new Date(timestamp).toLocaleString();
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold tracking-tight">Security Audit Trail</h2>
          <p className="text-muted-foreground">
            View and export security audit logs
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" onClick={handleExport}>
            <Download className="w-4 h-4 mr-2" />
            Export
          </Button>
          <Button onClick={loadAuditLogs} disabled={isLoading}>
            <RefreshCw className={`w-4 h-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
            Refresh
          </Button>
        </div>
      </div>

      {/* Filters */}
      <Card>
        <CardHeader>
          <CardTitle className="text-lg flex items-center gap-2">
            <Filter className="w-4 h-4" />
            Filters
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-3 lg:grid-cols-6 gap-4">
            {/* Action Filter */}
            <div className="space-y-2">
              <label className="text-sm font-medium">Action</label>
              <Input
                placeholder="e.g., adapter.register"
                value={actionFilter}
                onChange={(e) => setActionFilter(e.target.value)}
              />
            </div>

            {/* User Filter */}
            <div className="space-y-2">
              <label className="text-sm font-medium">User</label>
              <Input
                placeholder="User ID"
                value={userFilter}
                onChange={(e) => setUserFilter(e.target.value)}
              />
            </div>

            {/* Resource Filter */}
            <div className="space-y-2">
              <label className="text-sm font-medium">Resource</label>
              <Input
                placeholder="e.g., adapter"
                value={resourceFilter}
                onChange={(e) => setResourceFilter(e.target.value)}
              />
            </div>

            {/* Status Filter */}
            <div className="space-y-2">
              <label className="text-sm font-medium">Status</label>
              <Select value={statusFilter} onValueChange={setStatusFilter}>
                <SelectTrigger>
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

            {/* Start Date */}
            <div className="space-y-2">
              <label className="text-sm font-medium">Start Date</label>
              <Input
                type="datetime-local"
                value={startDate}
                onChange={(e) => setStartDate(e.target.value)}
              />
            </div>

            {/* End Date */}
            <div className="space-y-2">
              <label className="text-sm font-medium">End Date</label>
              <Input
                type="datetime-local"
                value={endDate}
                onChange={(e) => setEndDate(e.target.value)}
              />
            </div>
          </div>

          <div className="mt-4 flex justify-end">
            <Button
              variant="outline"
              onClick={() => {
                setActionFilter('');
                setUserFilter('');
                setResourceFilter('');
                setStatusFilter('all');
                setStartDate('');
                setEndDate('');
              }}
            >
              Clear Filters
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Audit Log Table */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <FileText className="w-5 h-5" />
            Audit Logs
          </CardTitle>
          <CardDescription>
            {logs.length} log entries found
          </CardDescription>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <div className="flex items-center justify-center py-12">
              <RefreshCw className="w-8 h-8 animate-spin text-muted-foreground" />
            </div>
          ) : logs.length === 0 ? (
            <div className="text-center py-12 text-muted-foreground">
              <FileText className="w-12 h-12 mx-auto mb-3 opacity-20" />
              <p>No audit logs found</p>
              <p className="text-sm">Try adjusting your filters</p>
            </div>
          ) : (
            <div className="overflow-x-auto">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Timestamp</TableHead>
                    <TableHead>Action</TableHead>
                    <TableHead>User</TableHead>
                    <TableHead>Resource</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Details</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {logs.map((log) => (
                    <TableRow key={log.id}>
                      <TableCell className="font-mono text-xs">
                        {formatTimestamp(log.timestamp)}
                      </TableCell>
                      <TableCell>
                        <code className="text-xs bg-muted px-1 py-0.5 rounded">
                          {log.action}
                        </code>
                      </TableCell>
                      <TableCell>
                        <div className="flex items-center gap-1">
                          <User className="w-3 h-3 text-muted-foreground" />
                          <span className="text-sm">{log.user_id}</span>
                        </div>
                      </TableCell>
                      <TableCell>
                        <span className="text-sm">
                          {log.resource}
                          {log.resource_id && (
                            <span className="text-muted-foreground">
                              /{log.resource_id.slice(0, 8)}
                            </span>
                          )}
                        </span>
                      </TableCell>
                      <TableCell>
                        <div className="flex items-center gap-2">
                          {getStatusIcon(log.status)}
                          {getStatusBadge(log.status)}
                        </div>
                      </TableCell>
                      <TableCell>
                        {log.details && Object.keys(log.details).length > 0 ? (
                          <code className="text-xs text-muted-foreground">
                            {JSON.stringify(log.details).slice(0, 50)}
                            {JSON.stringify(log.details).length > 50 ? '...' : ''}
                          </code>
                        ) : (
                          <span className="text-muted-foreground">-</span>
                        )}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
