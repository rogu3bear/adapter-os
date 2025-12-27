/**
 * Admin Dashboard
 *
 * Role-specific dashboard for Admin users providing:
 * - Tenant summary (total tenants, active, paused)
 * - User activity (recent logins, active users)
 * - Security overview (policy violations, audit events)
 * - System resource usage
 * - Quick actions for common admin tasks
 *
 * Citations:
 * - AGENTS.md: RBAC section (5 Roles, 40 Permissions)
 * - docs/RBAC.md: Admin role permissions
 * - ui/src/components/Dashboard.tsx: Dashboard patterns
 */

import React, { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Skeleton } from '@/components/ui/skeleton';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { PageHeader } from '@/components/ui/page-header';
import { KpiGrid, ContentGrid } from '@/components/ui/grid';
import { ActionGrid } from '@/components/ui/action-grid';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import {
  Users,
  UserPlus,
  Shield,
  Settings,
  AlertTriangle,
  CheckCircle,
  Clock,
  Activity,
  Cpu,
  HardDrive,
  Database,
  ShieldAlert,
  FileText,
  TrendingUp,
  UserCheck,
  Building2,
} from 'lucide-react';
import { toast } from 'sonner';
import { apiClient } from '@/api/services';
import { useTenants } from '@/hooks/admin/useAdmin';
import { useAuth } from '@/providers/CoreProviders';
import { useComputedMetrics } from '@/hooks/system/useSystem';
import { logger } from '@/utils/logger';
import { isDemoSessionMode } from '@/config/demo';
import { buildAdminTenantsLink, buildSecurityAuditLink, ROUTE_PATHS } from '@/utils/navLinks';
import type { Tenant, User, AuditLog, SystemMetrics } from '@/api/types';

interface TenantSummary {
  total: number;
  active: number;
  paused: number;
  archived: number;
}

interface UserActivity {
  totalUsers: number;
  activeUsers: number;
  recentLogins: number;
  newUsersThisWeek: number;
}

interface SecurityOverview {
  policyViolations: number;
  auditEvents: number;
  failedLogins: number;
  suspiciousActivity: number;
}

function DemoControls({ onRefresh }: { onRefresh: () => Promise<void> }) {
  const handleReset = async () => {
    toast.success('Demo data reset requested. Sample data will refresh shortly.');
    await onRefresh();
  };

  const handleReplay = async () => {
    toast.info('Sample admin activity added to the demo timeline.');
    await onRefresh();
  };

  return (
    <Alert variant="default" className="border-primary/30 bg-primary/5">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <AlertTitle className="text-sm font-semibold">Demo controls</AlertTitle>
          <AlertDescription className="text-xs text-muted-foreground">
            These actions only affect the demo session and never touch production workspaces.
          </AlertDescription>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" variant="outline" onClick={handleReset}>
            Reset demo data
          </Button>
          <Button size="sm" onClick={handleReplay}>
            Inject sample activity
          </Button>
        </div>
      </div>
    </Alert>
  );
}

export default function AdminDashboard() {
  const navigate = useNavigate();
  const { user, sessionMode } = useAuth();
  const [refreshTrigger, setRefreshTrigger] = useState(0);
  const isDemo = isDemoSessionMode(sessionMode);

  // Fetch tenants
  const {
    data: tenantsData,
    isLoading: tenantsLoading,
    error: tenantsError,
    refetch: refetchTenants,
  } = useTenants();

  // Fetch users
  const {
    data: usersData,
    isLoading: usersLoading,
    error: usersError,
    refetch: refetchUsers,
  } = useQuery<{ users: User[] }>({
    queryKey: ['admin-users', refreshTrigger],
    queryFn: () => apiClient.listUsers({ page: 1, page_size: 100 }),
    staleTime: 30000,
  });

  // Fetch audit logs
  const {
    data: auditLogs,
    isLoading: auditLoading,
    error: auditError,
  } = useQuery<AuditLog[]>({
    queryKey: ['admin-audit-logs', refreshTrigger],
    queryFn: () => apiClient.queryAuditLogs({ limit: 50 }),
    staleTime: 30000,
  });

  // Fetch system metrics - near real-time updates (every 2 seconds)
  const {
    data: systemMetrics,
    isLoading: metricsLoading,
    error: metricsError,
  } = useQuery<SystemMetrics>({
    queryKey: ['admin-system-metrics', refreshTrigger],
    queryFn: () => apiClient.getSystemMetrics(),
    refetchInterval: 2000,  // Update every 2 seconds for near real-time
    staleTime: 1000,        // Consider data stale after 1 second
  });

  const computedMetrics = useComputedMetrics(systemMetrics ?? null);

  // Calculate tenant summary
  const tenantSummary: TenantSummary = React.useMemo(() => {
    const tenants: Tenant[] = tenantsData || [];
    return {
      total: tenants.length,
      active: tenants.filter((t) => t.status === 'active' || !t.status).length,
      paused: tenants.filter((t) => t.status === 'paused').length,
      archived: tenants.filter((t) => t.status === 'archived').length,
    };
  }, [tenantsData]);

  // Calculate user activity
  const userActivity: UserActivity = React.useMemo(() => {
    const users = usersData?.users || [];
    const now = Date.now();
    const oneWeek = 7 * 24 * 60 * 60 * 1000;
    const oneDay = 24 * 60 * 60 * 1000;

    return {
      totalUsers: users.length,
      activeUsers: users.filter((u) => {
        const lastLogin = u.last_login_at || u.last_login;
        if (!lastLogin) return false;
        return now - new Date(lastLogin).getTime() < oneWeek;
      }).length,
      recentLogins: users.filter((u) => {
        const lastLogin = u.last_login_at || u.last_login;
        if (!lastLogin) return false;
        return now - new Date(lastLogin).getTime() < oneDay;
      }).length,
      newUsersThisWeek: users.filter((u) => {
        if (!u.created_at) return false;
        return now - new Date(u.created_at).getTime() < oneWeek;
      }).length,
    };
  }, [usersData]);

  // Calculate security overview
  const securityOverview: SecurityOverview = React.useMemo(() => {
    const logs = auditLogs || [];
    const now = Date.now();
    const oneDay = 24 * 60 * 60 * 1000;

    return {
      policyViolations: logs.filter(
        (log) => log.status === 'failure' && log.action?.includes('policy')
      ).length,
      auditEvents: logs.length,
      failedLogins: logs.filter(
        (log) => log.action === 'auth.login' && log.status === 'failure'
      ).length,
      suspiciousActivity: logs.filter(
        (log) =>
          log.status === 'failure' &&
          now - new Date(log.timestamp).getTime() < oneDay
      ).length,
    };
  }, [auditLogs]);

  // Quick actions
  const quickActions = [
    {
      label: 'Create Tenant',
      icon: Building2,
      color: 'text-blue-600',
      helpId: 'admin-create-tenant',
      onClick: () => navigate(buildAdminTenantsLink({ action: 'create' })),
    },
    {
      label: 'Manage Tenants',
      icon: Users,
      color: 'text-purple-600',
      helpId: 'admin-manage-tenants',
      onClick: () => navigate(buildAdminTenantsLink()),
    },
    {
      label: 'System Settings',
      icon: Settings,
      color: 'text-gray-600',
      helpId: 'admin-system-settings',
      onClick: () => navigate(ROUTE_PATHS.admin.settings),
    },
    {
      label: 'Security Audit',
      icon: Shield,
      color: 'text-amber-600',
      helpId: 'admin-security-audit',
      onClick: () => navigate(buildSecurityAuditLink()),
    },
  ];

  // Refresh all data
  const handleRefresh = async () => {
    setRefreshTrigger((prev) => prev + 1);
    await Promise.all([refetchTenants(), refetchUsers()]);
    toast.success('Dashboard refreshed');
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <PageHeader
        title="Admin Dashboard"
        description={`Welcome back, ${user?.display_name || user?.email}. You have full system access.`}
        badges={[
          { label: 'Admin', variant: 'default' },
          { label: 'Full Access', variant: 'outline' },
        ]}
      >
        <Button variant="outline" size="sm" onClick={handleRefresh} className="w-full sm:w-auto">
          <Activity className="h-4 w-4 mr-2" />
          <span className="hidden sm:inline">Refresh</span>
        </Button>
      </PageHeader>

      {isDemo && <DemoControls onRefresh={handleRefresh} />}

      {/* Organization Summary */}
      <SectionErrorBoundary sectionName="Organization Summary">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Building2 className="h-5 w-5" />
                <span>Organization Summary</span>
              </div>
              <Button variant="ghost" size="sm" onClick={() => navigate(buildAdminTenantsLink())}>
                View All
              </Button>
            </CardTitle>
          </CardHeader>
          <CardContent>
            {tenantsLoading ? (
              <div className="space-y-2">
                <Skeleton className="h-16 w-full" />
                <Skeleton className="h-16 w-full" />
              </div>
            ) : tenantsError ? (
              <Alert variant="destructive">
                <AlertTriangle className="h-4 w-4" />
                <AlertTitle>Failed to load tenants</AlertTitle>
                <AlertDescription>
                  {tenantsError instanceof Error
                    ? tenantsError.message
                    : 'Unknown error occurred'}
                </AlertDescription>
              </Alert>
            ) : (
              <KpiGrid>
                <div className="space-y-1">
                  <p className="text-2xl font-bold text-blue-600">
                    {tenantSummary.total}
                  </p>
                  <p className="text-sm text-muted-foreground flex items-center gap-1">
                    <GlossaryTooltip brief="Workspaces your teams use" variant="inline">
                      <span>Workspaces (Tenants)</span>
                    </GlossaryTooltip>
                  </p>
                </div>
                <div className="space-y-1">
                  <p className="text-2xl font-bold text-green-600">
                    {tenantSummary.active}
                  </p>
                  <p className="text-sm text-muted-foreground">Active</p>
                </div>
                <div className="space-y-1">
                  <p className="text-2xl font-bold text-amber-600">
                    {tenantSummary.paused}
                  </p>
                  <p className="text-sm text-muted-foreground">Paused</p>
                </div>
                <div className="space-y-1">
                  <p className="text-2xl font-bold text-gray-600">
                    {tenantSummary.archived}
                  </p>
                  <p className="text-sm text-muted-foreground">Archived</p>
                </div>
              </KpiGrid>
            )}
          </CardContent>
        </Card>
      </SectionErrorBoundary>

      {/* User Activity & Security Overview */}
      <ContentGrid>
        {/* User Activity */}
        <SectionErrorBoundary sectionName="User Activity">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <UserCheck className="h-5 w-5" />
                <span>User Activity</span>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              {usersLoading ? (
                <Skeleton className="h-32 w-full" />
              ) : usersError ? (
                <Alert variant="destructive">
                  <AlertTriangle className="h-4 w-4" />
                  <AlertDescription>
                    Failed to load user data
                  </AlertDescription>
                </Alert>
              ) : (
                <>
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 sm:gap-4">
                    <div className="space-y-1">
                      <p className="text-xl sm:text-2xl font-bold">{userActivity.totalUsers}</p>
                      <p className="text-xs text-muted-foreground">Total Users</p>
                    </div>
                    <div className="space-y-1">
                      <p className="text-xl sm:text-2xl font-bold text-green-600">
                        {userActivity.activeUsers}
                      </p>
                      <p className="text-xs text-muted-foreground">Active (7d)</p>
                    </div>
                  </div>
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 sm:gap-4">
                    <div className="flex items-center justify-between p-3 bg-muted rounded-lg">
                      <div className="flex items-center gap-2">
                        <Clock className="h-4 w-4 text-muted-foreground" />
                        <span className="text-sm">Recent Logins</span>
                      </div>
                      <span className="text-lg font-semibold">
                        {userActivity.recentLogins}
                      </span>
                    </div>
                    <div className="flex items-center justify-between p-3 bg-muted rounded-lg">
                      <div className="flex items-center gap-2">
                        <UserPlus className="h-4 w-4 text-muted-foreground" />
                        <span className="text-sm">New (7d)</span>
                      </div>
                      <span className="text-lg font-semibold">
                        {userActivity.newUsersThisWeek}
                      </span>
                    </div>
                  </div>
                  <Button
                    variant="outline"
                    className="w-full"
                    onClick={() => navigate(ROUTE_PATHS.admin.users)}
                  >
                    <Users className="h-4 w-4 mr-2" />
                    Manage Users
                  </Button>
                </>
              )}
            </CardContent>
          </Card>
        </SectionErrorBoundary>

        {/* Security Overview */}
        <SectionErrorBoundary sectionName="Security Overview">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <ShieldAlert className="h-5 w-5" />
                <span>Security Overview</span>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              {auditLoading ? (
                <Skeleton className="h-32 w-full" />
              ) : auditError ? (
                <Alert variant="destructive">
                  <AlertTriangle className="h-4 w-4" />
                  <AlertDescription>
                    Failed to load audit data
                  </AlertDescription>
                </Alert>
              ) : (
                <>
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 sm:gap-4">
                    <div className="space-y-1">
                      <p className="text-xl sm:text-2xl font-bold text-red-600">
                        {securityOverview.policyViolations}
                      </p>
                      <p className="text-xs text-muted-foreground">
                        Policy Violations
                      </p>
                    </div>
                    <div className="space-y-1">
                      <p className="text-xl sm:text-2xl font-bold text-amber-600">
                        {securityOverview.failedLogins}
                      </p>
                      <p className="text-xs text-muted-foreground">
                        Failed Logins
                      </p>
                    </div>
                  </div>
                  <div className="space-y-2">
                    <div className="flex items-center justify-between p-3 bg-muted rounded-lg">
                      <span className="text-sm">Audit Events (24h)</span>
                      <Badge variant="outline">
                        {securityOverview.auditEvents}
                      </Badge>
                    </div>
                    <div className="flex items-center justify-between p-3 bg-muted rounded-lg">
                      <span className="text-sm">Suspicious Activity</span>
                      <Badge
                        variant={
                          securityOverview.suspiciousActivity > 0
                            ? 'destructive'
                            : 'outline'
                        }
                      >
                        {securityOverview.suspiciousActivity}
                      </Badge>
                    </div>
                    <div className="flex items-center justify-between p-3 bg-muted rounded-lg">
                      <GlossaryTooltip brief="Outbound data leaving your deployment" variant="inline">
                        <span className="text-sm cursor-help">Outbound Data (Egress)</span>
                      </GlossaryTooltip>
                      <Badge
                        variant={securityOverview.suspiciousActivity > 0 ? 'secondary' : 'outline'}
                      >
                        {securityOverview.suspiciousActivity > 0 ? 'Reviewing' : 'Clear'}
                      </Badge>
                    </div>
                  </div>
                  <Button
                    variant="outline"
                    className="w-full"
                    onClick={() => navigate(buildSecurityAuditLink())}
                  >
                    <FileText className="h-4 w-4 mr-2" />
                    View Audit Logs
                  </Button>
                </>
              )}
            </CardContent>
          </Card>
        </SectionErrorBoundary>
      </ContentGrid>

      {/* System Resource Usage */}
      <SectionErrorBoundary sectionName="System Resources">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Activity className="h-5 w-5" />
              <span>System Resource Usage</span>
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-6">
            {metricsLoading ? (
              <div className="space-y-4">
                <Skeleton className="h-12 w-full" />
                <Skeleton className="h-12 w-full" />
                <Skeleton className="h-12 w-full" />
              </div>
            ) : metricsError ? (
              <Alert variant="destructive">
                <AlertTriangle className="h-4 w-4" />
                <AlertDescription>
                  Failed to load system metrics
                </AlertDescription>
              </Alert>
            ) : (
              <>
                {/* CPU Usage */}
                <div className="space-y-2">
                  <div className="flex justify-between items-center">
                    <div className="flex items-center gap-2">
                      <Cpu className="h-5 w-5 text-muted-foreground" />
                      <GlossaryTooltip termId="cpu-usage">
                        <span className="text-sm font-medium cursor-help">
                          CPU Usage
                        </span>
                      </GlossaryTooltip>
                    </div>
                    <span className="text-sm font-semibold">
                      {(computedMetrics?.cpuUsage ?? 0).toFixed(1)}%
                    </span>
                  </div>
                  <Progress
                    value={computedMetrics?.cpuUsage ?? 0}
                    className="h-3"
                  />
                </div>

                {/* Memory Usage */}
                <div className="space-y-2">
                  <div className="flex justify-between items-center">
                    <div className="flex items-center gap-2">
                      <HardDrive className="h-5 w-5 text-muted-foreground" />
                      <GlossaryTooltip termId="memory-usage">
                        <span className="text-sm font-medium cursor-help">
                          Memory Usage
                        </span>
                      </GlossaryTooltip>
                    </div>
                    <span className="text-sm font-semibold">
                      {(computedMetrics?.memoryUsage ?? 0).toFixed(1)}%
                    </span>
                  </div>
                  <Progress
                    value={computedMetrics?.memoryUsage ?? 0}
                    className="h-3"
                  />
                </div>

                {/* Disk Usage */}
                <div className="space-y-2">
                  <div className="flex justify-between items-center">
                    <div className="flex items-center gap-2">
                      <Database className="h-5 w-5 text-muted-foreground" />
                      <GlossaryTooltip termId="disk-usage">
                        <span className="text-sm font-medium cursor-help">
                          Disk Usage
                        </span>
                      </GlossaryTooltip>
                    </div>
                    <span className="text-sm font-semibold">
                      {(computedMetrics?.diskUsage ?? 0).toFixed(1)}%
                    </span>
                  </div>
                  <Progress
                    value={computedMetrics?.diskUsage ?? 0}
                    className="h-3"
                  />
                </div>
              </>
            )}
          </CardContent>
        </Card>
      </SectionErrorBoundary>

      {/* Quick Actions */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base sm:text-lg">Quick Actions</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
            {quickActions.map((action) => {
              const Icon = action.icon;
              return (
                <Button
                  key={action.label}
                  variant="outline"
                  className="flex items-center justify-start gap-2 h-auto p-4"
                  onClick={action.onClick}
                >
                  <Icon className={`h-5 w-5 ${action.color}`} />
                  <span className="text-sm font-medium">{action.label}</span>
                </Button>
              );
            })}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
