import React, { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Skeleton } from '@/components/ui/skeleton';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  ChartConfig,
} from '@/components/ui/chart';
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  ResponsiveContainer,
  BarChart,
  Bar,
} from 'recharts@2.15.2';
import {
  Shield,
  FileText,
  Download,
  AlertTriangle,
  CheckCircle,
  XCircle,
  TrendingUp,
  Search,
  Eye,
} from 'lucide-react';
import apiClient from '@/api/client';
import { PageHeader } from '@/components/ui/page-header';
import { KpiGrid, ContentGrid } from '@/components/ui/grid';
import { ActionGrid } from '@/components/ui/action-grid';
import { HelpTooltip } from '@/components/ui/help-tooltip';
import { logger } from '@/utils/logger';
import type { AuditLog, ComplianceAuditResponse } from '@/api/api-types';
import { CANONICAL_POLICIES, PolicyCategory, POLICY_CATEGORIES } from '@/api/policyTypes';

interface ComplianceDashboardProps {
  selectedTenant?: string;
}

// Chart configuration for audit trends
const auditTrendsChartConfig = {
  success: {
    label: 'Success',
    color: 'hsl(var(--chart-1))',
  },
  failure: {
    label: 'Failure',
    color: 'hsl(var(--chart-2))',
  },
  warning: {
    label: 'Warning',
    color: 'hsl(var(--chart-3))',
  },
} satisfies ChartConfig;

// Chart configuration for policy compliance
const policyComplianceChartConfig = {
  compliant: {
    label: 'Compliant',
    color: 'hsl(var(--chart-1))',
  },
  violations: {
    label: 'Violations',
    color: 'hsl(var(--chart-2))',
  },
} satisfies ChartConfig;

export default function ComplianceDashboard({ selectedTenant = 'default' }: ComplianceDashboardProps) {
  // Fetch compliance audit data
  const {
    data: complianceData,
    isLoading: complianceLoading,
    error: complianceError,
    refetch: refetchCompliance,
  } = useQuery<ComplianceAuditResponse>({
    queryKey: ['compliance-audit', selectedTenant],
    queryFn: () => apiClient.getComplianceAudit(),
    refetchInterval: 60000, // Refresh every minute
    staleTime: 30000,
  });

  // Fetch recent audit logs
  const {
    data: auditLogs = [],
    isLoading: auditLogsLoading,
    error: auditLogsError,
    refetch: refetchAuditLogs,
  } = useQuery<AuditLog[]>({
    queryKey: ['audit-logs', selectedTenant],
    queryFn: () =>
      apiClient.queryAuditLogs({
        limit: 50,
        tenant_id: selectedTenant,
      }),
    refetchInterval: 30000,
    staleTime: 15000,
  });

  // Calculate compliance score
  const complianceScore = useMemo(() => {
    if (!complianceData?.findings) return 0;

    const total = complianceData.findings.length;
    if (total === 0) return 100;

    const passed = complianceData.findings.filter((f) => f.status === 'passed').length;
    return Math.round((passed / total) * 100);
  }, [complianceData]);

  // Aggregate policy pack status by category
  const policyPackStats = useMemo(() => {
    const stats: Record<
      PolicyCategory,
      { total: number; passed: number; failed: number; warnings: number }
    > = {
      security: { total: 0, passed: 0, failed: 0, warnings: 0 },
      quality: { total: 0, passed: 0, failed: 0, warnings: 0 },
      compliance: { total: 0, passed: 0, failed: 0, warnings: 0 },
      performance: { total: 0, passed: 0, failed: 0, warnings: 0 },
    };

    if (!complianceData?.findings) return stats;

    // Map findings to canonical policies
    CANONICAL_POLICIES.forEach((policy) => {
      const finding = complianceData.findings.find((f) => f.rule === policy.id);
      const category = policy.category;

      stats[category].total += 1;

      if (finding) {
        if (finding.status === 'passed') {
          stats[category].passed += 1;
        } else if (finding.status === 'failed') {
          stats[category].failed += 1;
        } else if (finding.status === 'warning') {
          stats[category].warnings += 1;
        }
      } else {
        // No finding means not checked, treat as warning
        stats[category].warnings += 1;
      }
    });

    return stats;
  }, [complianceData]);

  // Generate audit trend data (mock data for demonstration)
  const auditTrendData = useMemo(() => {
    const now = Date.now();
    const dayMs = 24 * 60 * 60 * 1000;

    return Array.from({ length: 7 }, (_, i) => {
      const date = new Date(now - (6 - i) * dayMs);
      const dateStr = date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });

      // Count audit logs for this day
      const dayStart = new Date(date);
      dayStart.setHours(0, 0, 0, 0);
      const dayEnd = new Date(date);
      dayEnd.setHours(23, 59, 59, 999);

      const logsForDay = auditLogs.filter((log) => {
        const logDate = new Date(log.timestamp);
        return logDate >= dayStart && logDate <= dayEnd;
      });

      return {
        date: dateStr,
        success: logsForDay.filter((l) => l.status === 'success').length,
        failure: logsForDay.filter((l) => l.status === 'failure').length,
        warning: logsForDay.filter((l) => l.status === 'error').length,
      };
    });
  }, [auditLogs]);

  // Recent violations (failures from audit logs)
  const recentViolations = useMemo(() => {
    return auditLogs
      .filter((log) => log.status === 'failure' || log.status === 'error')
      .slice(0, 10)
      .map((log) => ({
        id: log.id,
        timestamp: log.timestamp,
        action: log.action,
        resource: log.resource,
        user: log.user_id,
        status: log.status,
      }));
  }, [auditLogs]);

  // Quick actions for Compliance role
  const quickActions = useMemo(
    () => [
      {
        label: 'Audit Logs',
        icon: FileText,
        color: 'text-blue-600',
        helpId: 'compliance-audit-logs',
        onClick: () => window.open('/admin/audit', '_blank'),
      },
      {
        label: 'Policy Report',
        icon: Shield,
        color: 'text-purple-600',
        helpId: 'compliance-policy-report',
        onClick: () => refetchCompliance(),
      },
      {
        label: 'Export Compliance',
        icon: Download,
        color: 'text-green-600',
        helpId: 'compliance-export',
        onClick: async () => {
          try {
            const data = {
              compliance: complianceData,
              auditLogs: auditLogs,
              exportedAt: new Date().toISOString(),
            };
            const blob = new Blob([JSON.stringify(data, null, 2)], {
              type: 'application/json',
            });
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = `compliance-export-${new Date().toISOString().split('T')[0]}.json`;
            a.click();
            URL.revokeObjectURL(url);
          } catch (err) {
            logger.error(
              'Failed to export compliance data',
              { component: 'ComplianceDashboard', operation: 'exportCompliance' },
              err instanceof Error ? err : new Error(String(err))
            );
          }
        },
      },
      {
        label: 'Review Evidence',
        icon: Eye,
        color: 'text-amber-600',
        helpId: 'compliance-review-evidence',
        onClick: () => window.open('/evidence', '_blank'),
      },
    ],
    [complianceData, auditLogs, refetchCompliance]
  );

  // Compliance status badge color
  const getStatusColor = (status: string) => {
    switch (status) {
      case 'passed':
        return 'text-green-600 bg-green-50 border-green-200';
      case 'failed':
        return 'text-red-600 bg-red-50 border-red-200';
      case 'warning':
        return 'text-yellow-600 bg-yellow-50 border-yellow-200';
      default:
        return 'text-gray-600 bg-gray-50 border-gray-200';
    }
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <PageHeader
        title="Compliance Dashboard"
        description={`Compliance monitoring and audit tracking for tenant: ${selectedTenant}`}
        badges={[
          {
            label: `Score: ${complianceScore}%`,
            variant: complianceScore >= 80 ? 'default' : 'destructive',
          },
          {
            label: complianceData?.status || 'Unknown',
            variant:
              complianceData?.status === 'passed'
                ? 'default'
                : complianceData?.status === 'warning'
                  ? 'secondary'
                  : 'destructive',
          },
        ]}
      />

      {/* Error Alerts */}
      {complianceError && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>Compliance Data Error</AlertTitle>
          <AlertDescription>
            Failed to load compliance data. Please try again.
          </AlertDescription>
        </Alert>
      )}

      {auditLogsError && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>Audit Logs Error</AlertTitle>
          <AlertDescription>Failed to load audit logs. Please try again.</AlertDescription>
        </Alert>
      )}

      {/* KPI Cards - Compliance Score Overview */}
      <KpiGrid>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <HelpTooltip helpId="compliance-score">
              <CardTitle className="text-sm font-medium cursor-help">
                Compliance Score
              </CardTitle>
            </HelpTooltip>
            <Shield className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {complianceLoading ? (
              <Skeleton className="h-10 w-24" />
            ) : (
              <>
                <div className="text-3xl font-bold">{complianceScore}%</div>
                <Progress value={complianceScore} className="mt-2 h-2" />
                <p className="text-xs text-muted-foreground mt-2">
                  {complianceData?.findings
                    ? `${complianceData.findings.filter((f) => f.status === 'passed').length}/${complianceData.findings.length} checks passed`
                    : 'No data'}
                </p>
              </>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <HelpTooltip helpId="policy-violations">
              <CardTitle className="text-sm font-medium cursor-help">
                Policy Violations
              </CardTitle>
            </HelpTooltip>
            <XCircle className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {complianceLoading ? (
              <Skeleton className="h-10 w-16" />
            ) : (
              <>
                <div className="text-3xl font-bold text-red-600">
                  {complianceData?.violations?.length || 0}
                </div>
                <p className="text-xs text-muted-foreground mt-2">Active violations</p>
              </>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <HelpTooltip helpId="audit-events">
              <CardTitle className="text-sm font-medium cursor-help">Audit Events</CardTitle>
            </HelpTooltip>
            <FileText className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {auditLogsLoading ? (
              <Skeleton className="h-10 w-16" />
            ) : (
              <>
                <div className="text-3xl font-bold">{auditLogs.length}</div>
                <p className="text-xs text-muted-foreground mt-2">Last 50 events</p>
              </>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <HelpTooltip helpId="compliance-trend">
              <CardTitle className="text-sm font-medium cursor-help">
                Compliance Trend
              </CardTitle>
            </HelpTooltip>
            <TrendingUp className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold text-green-600">
              {complianceScore >= 80 ? '+' : ''}
              {complianceScore - 75}%
            </div>
            <p className="text-xs text-muted-foreground mt-2">vs. baseline (75%)</p>
          </CardContent>
        </Card>
      </KpiGrid>

      {/* Content Grid */}
      <ContentGrid>
        {/* Policy Pack Status Grid */}
        <Card className="lg:col-span-2">
          <CardHeader>
            <CardTitle>Policy Pack Status</CardTitle>
          </CardHeader>
          <CardContent>
            {complianceLoading ? (
              <Skeleton className="h-64 w-full" />
            ) : (
              <div className="space-y-4">
                {(Object.keys(POLICY_CATEGORIES) as PolicyCategory[]).map((category) => {
                  const stats = policyPackStats[category];
                  const categoryInfo = POLICY_CATEGORIES[category];
                  const passRate =
                    stats.total > 0 ? Math.round((stats.passed / stats.total) * 100) : 0;

                  return (
                    <div key={category} className="space-y-2">
                      <div className="flex items-center justify-between">
                        <div>
                          <h4 className="text-sm font-medium">{categoryInfo.label}</h4>
                          <p className="text-xs text-muted-foreground">
                            {categoryInfo.description}
                          </p>
                        </div>
                        <div className="text-right">
                          <div className="text-lg font-bold">{passRate}%</div>
                          <div className="text-xs text-muted-foreground">
                            {stats.passed}/{stats.total} passed
                          </div>
                        </div>
                      </div>
                      <Progress value={passRate} className="h-2" />
                      <div className="flex gap-2 text-xs">
                        <Badge variant="outline" className="bg-green-50">
                          <CheckCircle className="h-3 w-3 mr-1 text-green-600" />
                          {stats.passed} passed
                        </Badge>
                        {stats.failed > 0 && (
                          <Badge variant="outline" className="bg-red-50">
                            <XCircle className="h-3 w-3 mr-1 text-red-600" />
                            {stats.failed} failed
                          </Badge>
                        )}
                        {stats.warnings > 0 && (
                          <Badge variant="outline" className="bg-yellow-50">
                            <AlertTriangle className="h-3 w-3 mr-1 text-yellow-600" />
                            {stats.warnings} warnings
                          </Badge>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </CardContent>
        </Card>

        {/* Audit Trends Chart */}
        <Card className="lg:col-span-2">
          <CardHeader>
            <CardTitle>Audit Trends (Last 7 Days)</CardTitle>
          </CardHeader>
          <CardContent>
            {auditLogsLoading ? (
              <Skeleton className="h-64 w-full" />
            ) : (
              <ChartContainer config={auditTrendsChartConfig} className="h-64">
                <LineChart data={auditTrendData}>
                  <CartesianGrid strokeDasharray="3 3" />
                  <XAxis dataKey="date" />
                  <YAxis />
                  <ChartTooltip content={<ChartTooltipContent />} />
                  <Line
                    type="monotone"
                    dataKey="success"
                    stroke="var(--color-success)"
                    strokeWidth={2}
                  />
                  <Line
                    type="monotone"
                    dataKey="failure"
                    stroke="var(--color-failure)"
                    strokeWidth={2}
                  />
                  <Line
                    type="monotone"
                    dataKey="warning"
                    stroke="var(--color-warning)"
                    strokeWidth={2}
                  />
                </LineChart>
              </ChartContainer>
            )}
          </CardContent>
        </Card>

        {/* Recent Violations List */}
        <Card className="lg:col-span-2">
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle>Recent Violations</CardTitle>
              <Button variant="outline" size="sm" asChild>
                <Link to="/admin/audit">View All</Link>
              </Button>
            </div>
          </CardHeader>
          <CardContent>
            {auditLogsLoading ? (
              <Skeleton className="h-64 w-full" />
            ) : recentViolations.length === 0 ? (
              <div className="text-center py-8 text-muted-foreground">
                <CheckCircle className="h-12 w-12 mx-auto mb-2 text-green-600" />
                <p>No recent violations detected</p>
              </div>
            ) : (
              <div className="space-y-2">
                {recentViolations.map((violation) => (
                  <div
                    key={violation.id}
                    className="flex items-start justify-between gap-4 p-3 rounded-lg border bg-muted/40"
                  >
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 mb-1">
                        <Badge
                          variant="outline"
                          className={getStatusColor(violation.status)}
                        >
                          {violation.status}
                        </Badge>
                        <span className="text-xs text-muted-foreground">
                          {new Date(violation.timestamp).toLocaleString()}
                        </span>
                      </div>
                      <p className="text-sm font-medium truncate">{violation.action}</p>
                      <p className="text-xs text-muted-foreground">
                        Resource: {violation.resource} | User: {violation.user}
                      </p>
                    </div>
                    <Button variant="ghost" size="sm" asChild>
                      <Link to={`/admin/audit?id=${violation.id}`}>
                        <Search className="h-4 w-4" />
                      </Link>
                    </Button>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      </ContentGrid>

      {/* Quick Actions */}
      <Card>
        <CardHeader>
          <HelpTooltip helpId="compliance-quick-actions">
            <CardTitle className="cursor-help">Quick Actions</CardTitle>
          </HelpTooltip>
        </CardHeader>
        <CardContent>
          <ActionGrid actions={quickActions} columns={4} />
        </CardContent>
      </Card>
    </div>
  );
}
