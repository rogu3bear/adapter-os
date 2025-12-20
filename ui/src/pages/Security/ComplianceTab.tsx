/**
 * ComplianceTab - Compliance dashboard and reporting
 *
 * Features:
 * - Compliance audit status overview
 * - Compliance controls list
 * - Policy violations tracking
 * - Compliance findings and recommendations
 */

import React from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { DataTable } from '@/components/shared/DataTable/DataTable';
import type { ColumnDef } from '@/components/shared/DataTable/types';
import {
  CheckCircle,
  XCircle,
  AlertTriangle,
  RefreshCw,
  Shield,
  AlertCircle,
  FileText,
} from 'lucide-react';

import { useComplianceAudit } from '@/hooks/security/useSecurity';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import type { ComplianceControl, PolicyViolationRecord } from '@/api/types';
import { Link } from 'react-router-dom';
import { buildReplayLink } from '@/utils/navLinks';

export function ComplianceTab() {
  const { complianceAudit, isLoading, error, refetch } = useComplianceAudit();

  const getStatusIcon = (status: string) => {
    switch (status.toLowerCase()) {
      case 'passed':
        return <CheckCircle className="h-5 w-5 text-green-500" />;
      case 'failed':
        return <XCircle className="h-5 w-5 text-red-500" />;
      case 'warning':
        return <AlertTriangle className="h-5 w-5 text-yellow-500" />;
      default:
        return <AlertCircle className="h-5 w-5 text-gray-500" />;
    }
  };

  const getStatusBadge = (status: string) => {
    switch (status.toLowerCase()) {
      case 'passed':
        return <Badge variant="default" className="bg-green-500">Passed</Badge>;
      case 'failed':
        return <Badge variant="destructive">Failed</Badge>;
      case 'warning':
        return <Badge variant="secondary" className="bg-yellow-500">Warning</Badge>;
      default:
        return <Badge variant="outline">{status}</Badge>;
    }
  };

  const getSeverityBadge = (severity?: string) => {
    switch (severity?.toLowerCase()) {
      case 'critical':
        return <Badge variant="destructive">Critical</Badge>;
      case 'high':
        return <Badge variant="destructive" className="bg-orange-500">High</Badge>;
      case 'medium':
        return <Badge variant="secondary" className="bg-yellow-500">Medium</Badge>;
      case 'low':
        return <Badge variant="outline">Low</Badge>;
      default:
        return <Badge variant="outline">{severity || 'Unknown'}</Badge>;
    }
  };

  // Type for controls from compliance audit response with required _index
  type ComplianceControlSimple = { name: string; status: string; message?: string; _index: number };

  const controlsColumns: ColumnDef<ComplianceControlSimple>[] = [
    {
      id: 'name',
      header: 'Control',
      accessorKey: 'name',
      enableSorting: true,
    },
    {
      id: 'status',
      header: 'Status',
      accessorKey: 'status',
      cell: (context) => getStatusBadge(context.row.status),
      enableSorting: true,
    },
    {
      id: 'message',
      header: 'Message',
      accessorKey: 'message',
      cell: (context) => {
        const message = context.row.message;
        return message ? (
          <div className="text-sm text-muted-foreground max-w-md truncate">
            {message}
          </div>
        ) : (
          '-'
        );
      },
    },
  ];

  // Type for violations from compliance audit response with required _index
  type ComplianceViolation = { rule: string; message: string; severity?: string; _index: number };

  const violationsColumns: ColumnDef<ComplianceViolation>[] = [
    {
      id: 'rule',
      header: 'Rule',
      accessorKey: 'rule',
      enableSorting: true,
    },
    {
      id: 'severity',
      header: 'Severity',
      accessorKey: 'severity',
      cell: (context) => getSeverityBadge(context.row.severity),
      enableSorting: true,
    },
    {
      id: 'message',
      header: 'Message',
      accessorKey: 'message',
      enableSorting: true,
    },
  ];

  if (error) {
    return <ErrorRecovery error={error.message} onRetry={refetch} />;
  }

  const controls = complianceAudit?.controls || [];
  const violations = complianceAudit?.violations || [];
  const findings = complianceAudit?.findings || [];

  const passedControls = controls.filter((c) => c.status.toLowerCase() === 'passed').length;
  const failedControls = controls.filter((c) => c.status.toLowerCase() === 'failed').length;
  const warningControls = controls.filter((c) => c.status.toLowerCase() === 'warning').length;

  const unresolvedViolations = violations.length;
  const criticalViolations = violations.filter(
    (v) => v.severity === 'critical'
  ).length;

  return (
    <div className="space-y-6">
      {/* Action Bar */}
      <div className="flex items-center justify-end gap-2">
        <Button variant="outline" size="sm" asChild>
          <Link to={buildReplayLink('runs')}>Open related replay</Link>
        </Button>
        <Button variant="outline" size="sm" onClick={() => refetch()}>
          <RefreshCw className="h-4 w-4 mr-2" />
          Refresh
        </Button>
      </div>

      {/* Summary Cards */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Overall Status
            </CardTitle>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <Skeleton className="h-12 w-24" />
            ) : (
              <div className="flex items-center gap-2">
                {getStatusIcon(complianceAudit?.status || 'unknown')}
                <span className="text-2xl font-bold capitalize">
                  {complianceAudit?.status || 'Unknown'}
                </span>
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Passed Controls
            </CardTitle>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <Skeleton className="h-12 w-16" />
            ) : (
              <div className="flex items-center gap-2">
                <CheckCircle className="h-5 w-5 text-green-500" />
                <span className="text-2xl font-bold">
                  {passedControls} / {controls.length}
                </span>
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Failed Controls
            </CardTitle>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <Skeleton className="h-12 w-16" />
            ) : (
              <div className="flex items-center gap-2">
                <XCircle className="h-5 w-5 text-red-500" />
                <span className="text-2xl font-bold">{failedControls}</span>
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Unresolved Violations
            </CardTitle>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <Skeleton className="h-12 w-16" />
            ) : (
              <div className="flex items-center gap-2">
                <AlertTriangle className="h-5 w-5 text-yellow-500" />
                <span className="text-2xl font-bold">{unresolvedViolations}</span>
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Findings Card */}
      {findings.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <FileText className="h-5 w-5" />
              Findings
            </CardTitle>
            <CardDescription>
              Compliance audit findings and recommendations
            </CardDescription>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <div className="space-y-2">
                {[1, 2, 3].map((i) => (
                  <Skeleton key={i} className="h-16 w-full" />
                ))}
              </div>
            ) : (
              <div className="space-y-3">
                {findings.map((finding, idx) => (
                  <div
                    key={idx}
                    className="border rounded-md p-4 space-y-2"
                  >
                    <div className="flex items-start justify-between">
                      <div className="flex items-center gap-2">
                        {getStatusIcon(finding.status)}
                        <span className="font-medium">{finding.rule}</span>
                      </div>
                      {getStatusBadge(finding.status)}
                    </div>
                    <p className="text-sm text-muted-foreground">{finding.message}</p>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      )}

      {/* Compliance Controls Table */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Shield className="h-5 w-5" />
            Compliance Controls
          </CardTitle>
          <CardDescription>
            Status of all compliance controls and checks
          </CardDescription>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <div className="space-y-2">
              {[1, 2, 3, 4].map((i) => (
                <Skeleton key={i} className="h-12 w-full" />
              ))}
            </div>
          ) : controls.length > 0 ? (
            <DataTable
              data={controls.map((c, idx) => ({ ...c, _index: idx }))}
              columns={controlsColumns}
              getRowId={(row) => `control-${row._index}`}
              enableSorting
              enablePagination
              pagination={{ pageIndex: 0, pageSize: 10 }}
              emptyTitle="No controls found"
              emptyDescription="No compliance controls are currently configured."
            />
          ) : (
            <div className="text-center py-8 text-muted-foreground">
              No compliance controls configured
            </div>
          )}
        </CardContent>
      </Card>

      {/* Policy Violations Table */}
      {violations.length > 0 && (
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle className="flex items-center gap-2">
                  <AlertTriangle className="h-5 w-5" />
                  Policy Violations
                </CardTitle>
                <CardDescription>
                  Active and resolved policy violations
                </CardDescription>
              </div>
              {criticalViolations > 0 && (
                <Badge variant="destructive" className="text-sm">
                  {criticalViolations} Critical
                </Badge>
              )}
            </div>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <div className="space-y-2">
                {[1, 2, 3].map((i) => (
                  <Skeleton key={i} className="h-12 w-full" />
                ))}
              </div>
            ) : (
              <DataTable
                data={violations.map((v, idx) => ({ ...v, _index: idx }))}
                columns={violationsColumns}
                getRowId={(row) => `violation-${row._index}`}
                enableSorting
                enablePagination
                pagination={{ pageIndex: 0, pageSize: 10 }}
                emptyTitle="No violations found"
                emptyDescription="No policy violations have been recorded."
              />
            )}
          </CardContent>
        </Card>
      )}

      {/* Audit Metadata */}
      {complianceAudit && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Audit Information</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            <div className="flex justify-between">
              <span className="text-muted-foreground">Audit ID:</span>
              <span className="font-mono">{complianceAudit.audit_id}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Generated At:</span>
              <span>{new Date(complianceAudit.generated_at).toLocaleString()}</span>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
