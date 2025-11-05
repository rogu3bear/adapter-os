import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { logger, toError } from '../utils/logger';
import type { MetricsSnapshotResponse } from '../api/types';
import { 
  Activity, 
  Shield, 
  CheckCircle, 
  Code,
  Eye,
  Download,
  Bell,
  Zap,
  Play,
  FileText,
  TrendingUp
} from 'lucide-react';
import { MLPipelineWidget } from './dashboard/MLPipelineWidget';
import { NextStepsWidget } from './dashboard/NextStepsWidget';
import { AdapterStatusWidget } from './dashboard/AdapterStatusWidget';
import { ComplianceScoreWidget } from './dashboard/ComplianceScoreWidget';
import { ActiveAlertsWidget } from './dashboard/ActiveAlertsWidget';
import { MultiModelStatusWidget } from './dashboard/MultiModelStatusWidget';
import { BaseModelWidget } from './dashboard/BaseModelWidget';
import { ReportingSummaryWidget } from './dashboard/ReportingSummaryWidget';
import { ServiceStatusWidget } from './dashboard/ServiceStatusWidget';
import { useAuth, useTenant } from '@/layout/LayoutProvider';
import { useNavigate } from 'react-router-dom';
import type { UserRole, User, SystemMetrics } from '@/api/types';
import apiClient from '../api/client';
import { useAnnounce, useKeyboardShortcuts } from '@/utils/accessibility';
import { usePolling } from '../hooks/usePolling';

interface DashboardProps {
  user?: User;
  selectedTenant?: string;
  onNavigate?: (tab: string) => void;
}

interface DashboardWidget {
  id: string;
  component: React.ComponentType<any>;
  priority: number;
}

interface DashboardLayout {
  widgets: DashboardWidget[];
  quickActions: Array<{
    label: string;
    icon: any;
    route: string;
    variant?: 'default' | 'outline' | 'secondary';
  }>;
}

// Simple system health widget for all roles
function SystemHealthWidget() {
  const announce = useAnnounce();

  const { data: metrics, isLoading: loading } = usePolling(
    () => apiClient.getSystemMetrics(),
    'normal',
    {
      operationName: 'SystemHealthWidget.getSystemMetrics',
      showLoadingIndicator: false,
      onSuccess: (data) => {
        const metrics = data as MetricsSnapshotResponse;
        if (metrics) {
          announce(`Metrics updated. Active sessions ${metrics.gauges?.active_sessions ?? 0}, latency ${metrics.gauges?.latency_p95_ms ?? 0} milliseconds`);
        }
      },
      onError: (err) => {
        logger.error('Failed to fetch system metrics', { component: 'SystemHealthWidget' }, err);
      }
    }
  );

  if (loading) {
    return (
      <Card aria-labelledby="sys-health-title">
        <CardHeader>
          <CardTitle id="sys-health-title">System Health</CardTitle>
        </CardHeader>
        <CardContent aria-busy={true}>
          <div role="status" aria-live="polite" className="h-20 animate-pulse bg-muted rounded">
            <span className="sr-only">Loading system health...</span>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card aria-labelledby="sys-health-title">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Activity className="h-5 w-5" aria-hidden="true" />
          System Health
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <p className="text-sm text-muted-foreground">Memory Usage</p>
            <p className="text-2xl font-bold">{metrics?.memory_usage_pct || 0}%</p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">Active Sessions</p>
            <p className="text-2xl font-bold">{metrics?.active_sessions || 0}</p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">Tokens/sec</p>
            <p className="text-2xl font-bold">{metrics?.tokens_per_second || 0}</p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground">P95 Latency</p>
            <p className="text-2xl font-bold">{metrics?.latency_p95_ms || 0}ms</p>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

// Role-specific dashboard configurations
const dashboardLayouts: Record<UserRole, DashboardLayout> = {
  admin: {
    widgets: [
      { id: 'service-status', component: ServiceStatusWidget, priority: 1 },
      { id: 'multi-model-status', component: MultiModelStatusWidget, priority: 2 },
      { id: 'system-health', component: SystemHealthWidget, priority: 3 },
      { id: 'active-alerts', component: ActiveAlertsWidget, priority: 4 },
      { id: 'compliance-score', component: ComplianceScoreWidget, priority: 5 },
      { id: 'reporting-summary', component: ReportingSummaryWidget, priority: 6 },
      { id: 'base-model', component: BaseModelWidget, priority: 7 },
    ],
    quickActions: [
      { label: 'System Health', icon: Activity, route: '/monitoring' },
      { label: 'Review Policies', icon: Shield, route: '/policies' },
      { label: 'View Telemetry', icon: Eye, route: '/telemetry' },
      { label: 'Manage Adapters', icon: Code, route: '/adapters' },
      { label: 'Reports', icon: FileText, route: '/reports' }
    ]
  },
  operator: {
    widgets: [
      { id: 'service-status', component: ServiceStatusWidget, priority: 1 },
      { id: 'ml-pipeline', component: MLPipelineWidget, priority: 2 },
      { id: 'adapter-status', component: AdapterStatusWidget, priority: 3 },
      { id: 'next-steps', component: NextStepsWidget, priority: 4 },
      { id: 'active-alerts', component: ActiveAlertsWidget, priority: 5 },
      { id: 'base-model', component: BaseModelWidget, priority: 6 },
    ],
    quickActions: [
      { label: 'Start Training', icon: Zap, route: '/training', variant: 'default' },
      { label: 'Test Adapter', icon: CheckCircle, route: '/testing' },
      { label: 'Run Inference', icon: Play, route: '/inference' },
      { label: 'View Routing', icon: TrendingUp, route: '/routing' },
    ]
  },
  sre: {
    widgets: [
      { id: 'service-status', component: ServiceStatusWidget, priority: 1 },
      { id: 'multi-model-status', component: MultiModelStatusWidget, priority: 2 },
      { id: 'active-alerts', component: ActiveAlertsWidget, priority: 3 },
      { id: 'system-health', component: SystemHealthWidget, priority: 4 },
      { id: 'adapter-status', component: AdapterStatusWidget, priority: 5 }
    ],
    quickActions: [
      { label: 'View Alerts', icon: Bell, route: '/monitoring', variant: 'default' },
      { label: 'System Logs', icon: FileText, route: '/telemetry' },
      { label: 'Routing Inspector', icon: TrendingUp, route: '/routing' },
      { label: 'Adapter Health', icon: Activity, route: '/adapters' }
    ]
  },
  compliance: {
    widgets: [
      { id: 'compliance-score', component: ComplianceScoreWidget, priority: 1 },
      { id: 'system-health', component: SystemHealthWidget, priority: 2 },
      { id: 'active-alerts', component: ActiveAlertsWidget, priority: 3 },
      { id: 'next-steps', component: NextStepsWidget, priority: 4 }
    ],
    quickActions: [
      { label: 'Review Policies', icon: Shield, route: '/policies', variant: 'default' },
      { label: 'Audit Trails', icon: FileText, route: '/audit' },
      { label: 'Export Telemetry', icon: Download, route: '/telemetry' },
      { label: 'Compliance Report', icon: CheckCircle, route: '/policies' }
    ]
  },
  auditor: {
    widgets: [
      { id: 'compliance-score', component: ComplianceScoreWidget, priority: 1 },
      { id: 'system-health', component: SystemHealthWidget, priority: 2 },
      { id: 'next-steps', component: NextStepsWidget, priority: 3 }
    ],
    quickActions: [
      { label: 'Audit Trails', icon: FileText, route: '/audit', variant: 'default' },
      { label: 'Verify Bundles', icon: Shield, route: '/telemetry' },
      { label: 'Export Audit', icon: Download, route: '/telemetry' },
      { label: 'Policy Review', icon: Shield, route: '/policies' }
    ]
  },
  viewer: {
    widgets: [
      { id: 'reporting-summary', component: ReportingSummaryWidget, priority: 1 },
      { id: 'system-health', component: SystemHealthWidget, priority: 2 },
      { id: 'adapter-status', component: AdapterStatusWidget, priority: 3 },
      { id: 'active-alerts', component: ActiveAlertsWidget, priority: 4 }
    ],
    quickActions: [
      { label: 'View Reports', icon: FileText, route: '/reports' },
      { label: 'Inference Playground', icon: Play, route: '/inference' },
      { label: 'System Metrics', icon: Activity, route: '/monitoring' },
      { label: 'Adapter Status', icon: Code, route: '/adapters' }
    ]
  }
};

export function Dashboard({ user: userProp, selectedTenant: tenantProp, onNavigate }: DashboardProps) {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const navigate = useNavigate();
  const effectiveUser = userProp ?? user!;

  if (!effectiveUser) {
    return null;
  }

  // Get layout for the user's role (validation happens in auth provider)
  let layout = dashboardLayouts[effectiveUser.role];

  // Safety check - this should never happen with proper role validation
  if (!layout) {
    logger.error('Critical: Valid user role has no dashboard layout', {
      component: 'Dashboard',
      userRole: effectiveUser.role,
      availableLayouts: Object.keys(dashboardLayouts)
    });
    // Emergency fallback to prevent crash
    layout = dashboardLayouts.viewer;
  }

  // Global shortcuts for search/help (announced via live region)
  const announce = useAnnounce();
  useKeyboardShortcuts({
    onSearch: () => announce('Search shortcut pressed'),
    onHelp: () => announce('Help shortcut pressed'),
  });

  return (
    <div className="space-y-6">

      {/* Widgets Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {layout.widgets.map((widget) => {
          const WidgetComponent = widget.component;
          return <WidgetComponent key={widget.id} selectedTenant={selectedTenant} />;
        })}
      </div>

      {/* Quick Actions */}
      <Card>
        <CardHeader>
          <CardTitle>Quick Actions</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-3" aria-label="Quick actions" role="list">
            {layout.quickActions.map((action, index) => {
              const Icon = action.icon;
              return (
                <Button
                  key={`${action.label}-${index}`}
                  variant={action.variant || 'outline'}
                  className="justify-start h-auto py-4"
                  aria-label={`Quick action: ${action.label}`}
                  onClick={() => {
                    if (onNavigate) {
                      onNavigate(action.route);
                    } else {
                      navigate(action.route);
                    }
                  }}
                >
                  <div className="flex items-center gap-3">
                    <Icon className="h-5 w-5" aria-hidden="true" />
                    <span className="font-medium">{action.label}</span>
                  </div>
                </Button>
              );
            })}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
