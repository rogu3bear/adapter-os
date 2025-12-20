import React from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { AlertTriangle, Bell, CheckCircle, Clock, Loader2 } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useTenant } from '@/providers/FeatureProviders';
import { usePolling } from '@/hooks/realtime/usePolling';
import { useServiceStatus } from '@/hooks/system/useServiceStatus';
import { useRelativeTime } from '@/hooks/ui/useTimestamp';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { apiClient } from '@/api/services';
import type { Alert as ApiAlert } from '@/api/types';
import { logger, toError } from '@/utils/logger';
import { DashboardWidgetFrame, type DashboardWidgetState } from './DashboardWidgetFrame';
import { buildMetricsLink } from '@/utils/navLinks';
import { withSectionErrorBoundary } from '@/components/ui/section-error-boundary';

interface Alert {
  id: string;
  severity: 'critical' | 'high' | 'medium' | 'low';
  title: string;
  created_at: string; // Store ISO timestamp for dynamic relative time calculation
  acknowledged: boolean;
}

type EventType = 'all' | 'recovery' | 'policy' | 'build' | 'adapter' | 'telemetry' | 'security' | 'error' | 'collaboration';
type Severity = 'all' | 'critical' | 'high' | 'medium' | 'low' | 'info' | 'warning' | 'error';

/**
 * Maps API alert severity to widget severity
 */
function mapSeverity(apiSeverity: string): Alert['severity'] {
  switch (apiSeverity.toLowerCase()) {
    case 'critical':
      return 'critical';
    case 'error':
      return 'high';
    case 'warning':
      return 'medium';
    case 'info':
      return 'low';
    default:
      // Default to medium for unknown severities
      return 'medium';
  }
}

/**
 * Maps API Alert to widget Alert format
 * Note: Timestamp stored as ISO string, calculated at render time for freshness
 */
function mapApiAlertToWidgetAlert(apiAlert: ApiAlert): Alert {
  return {
    id: apiAlert.id,
    severity: mapSeverity(apiAlert.severity),
    title: apiAlert.title ?? 'Unknown Alert',
    created_at: apiAlert.created_at ?? new Date().toISOString(), // Store ISO timestamp for dynamic calculation
    acknowledged: apiAlert.status === 'acknowledged',
  };
}

function ActiveAlertsWidgetBase() {
  const navigate = useNavigate();
  const { selectedTenant } = useTenant();
  const [typeFilter, setTypeFilter] = React.useState<EventType>('all');
  const [severityFilter, setSeverityFilter] = React.useState<Severity>('all');

  // Fetch service status for service failure alerts (shared subscription)
  const { status } = useServiceStatus();

  // Fetch alerts from API with polling
  const {
    data: apiAlerts,
    isLoading,
    error,
    lastUpdated,
    refetch,
  } = usePolling<ApiAlert[]>(
    async () => {
      if (!selectedTenant) {
        return [];
      }
      // Fetch active (unacknowledged) alerts only - widget name implies "active" means unacknowledged
      return apiClient.listAlerts({
        tenant_id: selectedTenant,
        status: 'active', // Only show unacknowledged alerts
        limit: 10,
      });
    },
    'fast', // 2000ms polling interval for real-time updates
    {
      operationName: 'ActiveAlertsWidget.listAlerts',
      enabled: !!selectedTenant,
      showLoadingIndicator: false,
      onError: (err) => {
        logger.error('Failed to load alerts', {
          component: 'ActiveAlertsWidget',
          operation: 'loadAlerts',
          tenantId: selectedTenant,
        }, toError(err));
      },
    }
  );

  // Generate alerts from service failures
  const serviceAlerts: Alert[] = React.useMemo(() => {
    if (!status?.services) return [];

    return status.services
      .filter(s => s.state === 'failed')
      .map(service => ({
        id: `service-${service.id}`,
        severity: 'critical' as const,
        title: `Service Failed: ${service.name}`,
        created_at: new Date().toISOString(),
        acknowledged: false,
      }));
  }, [status]);

  // Map API alerts to widget format
  // Since we fetch status: 'active', all returned alerts are unacknowledged
  const apiAlertsMapped: Alert[] = React.useMemo(() => {
    if (!apiAlerts || apiAlerts.length === 0) {
      return [];
    }
    return apiAlerts.map(mapApiAlertToWidgetAlert);
  }, [apiAlerts]);

  // Merge API alerts and service alerts
  const alerts: Alert[] = React.useMemo(() => {
    return [...apiAlertsMapped, ...serviceAlerts];
  }, [apiAlertsMapped, serviceAlerts]);

  const filteredAlerts = React.useMemo(() => {
    return alerts.filter((alert) => {
      const severityOk = severityFilter === 'all' ? true : alert.severity === severityFilter;
      // Event types are not currently part of the alert payload; keep filter passive until types are wired.
      const typeOk = typeFilter === 'all';
      return severityOk && typeOk;
    });
  }, [alerts, severityFilter, typeFilter]);

  // All alerts from API are active (unacknowledged) since we filter by status: 'active'
  const activeAlerts = filteredAlerts;
  const criticalCount = activeAlerts.filter(a => a.severity === 'critical').length;

  const renderAlert = (alert: Alert, colorClass: string) => (
    <div
      key={alert.id}
      className={`border rounded p-3 flex flex-col gap-2 ${colorClass}`}
      role="listitem"
    >
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium">{alert.title}</span>
        </div>
        <Badge variant="outline" className="text-xs capitalize">
          {alert.severity}
        </Badge>
      </div>
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <Clock className="h-3 w-3" aria-hidden />
        <span>
          {useRelativeTime(new Date(alert.created_at).toISOString())}
        </span>
      </div>
    </div>
  );

  const getSeverityColor = (severity: Alert['severity']) => {
    switch (severity) {
      case 'critical':
        return 'bg-red-100 text-red-800 border-red-200';
      case 'high':
        return 'bg-orange-100 text-orange-800 border-orange-200';
      case 'medium':
        return 'bg-yellow-100 text-yellow-800 border-yellow-200';
      default:
        return 'bg-blue-100 text-blue-800 border-blue-200';
    }
  };

  const getSeverityIcon = (severity: Alert['severity']) => {
    switch (severity) {
      case 'critical':
      case 'high':
        return AlertTriangle;
      default:
        return Bell;
    }
  };

  const state: DashboardWidgetState = error
    ? 'error'
    : (isLoading && !apiAlerts) || isLoading
      ? 'loading'
      : activeAlerts.length === 0
        ? 'empty'
        : 'ready';

  return (
    <DashboardWidgetFrame
      title={
        <div className="flex items-center gap-2">
          <Bell className="h-5 w-5" aria-hidden="true" />
          <span>Active Alerts</span>
        </div>
      }
      subtitle="Unacknowledged alerts and failed services"
      state={state}
      onRefresh={async () => { await refetch(); }}
      onRetry={async () => { await refetch(); }}
      lastUpdated={lastUpdated}
      errorMessage={error ? 'Failed to load alerts' : undefined}
      emptyMessage="No active alerts"
      headerRight={
        <Badge variant={activeAlerts.length > 0 ? 'destructive' : 'default'}>
          {activeAlerts.length}
        </Badge>
      }
      toolbar={
        <div className="flex flex-wrap items-center gap-2">
          <Select value={typeFilter} onValueChange={(v) => setTypeFilter(v as EventType)}>
            <SelectTrigger className="w-[calc(var(--base-unit)*35)]" aria-label="Type filter">
              <SelectValue placeholder="Type" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All types</SelectItem>
              <SelectItem value="recovery">Recovery</SelectItem>
              <SelectItem value="policy">Policy</SelectItem>
              <SelectItem value="build">Build</SelectItem>
              <SelectItem value="adapter">Adapter</SelectItem>
              <SelectItem value="telemetry">Telemetry</SelectItem>
              <SelectItem value="security">Security</SelectItem>
              <SelectItem value="error">Error</SelectItem>
              <SelectItem value="collaboration">Collaboration</SelectItem>
            </SelectContent>
          </Select>
          <Select value={severityFilter} onValueChange={(v) => setSeverityFilter(v as Severity)}>
            <SelectTrigger className="w-[calc(var(--base-unit)*35)]" aria-label="Severity filter">
              <SelectValue placeholder="Severity" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All severities</SelectItem>
              <SelectItem value="info">Info</SelectItem>
              <SelectItem value="warning">Warning</SelectItem>
              <SelectItem value="error">Error</SelectItem>
              <SelectItem value="critical">Critical</SelectItem>
            </SelectContent>
          </Select>
        </div>
      }
      loadingContent={
        <div className="text-center py-8">
          <Loader2 className="h-8 w-8 text-muted-foreground mx-auto mb-2 animate-spin" aria-hidden />
          <p className="text-sm text-muted-foreground">Loading alerts...</p>
        </div>
      }
    >
      {criticalCount > 0 && (
        <div className="p-3 bg-gray-100 border border-gray-300 rounded-lg">
          <div className="flex items-center gap-2 text-gray-800">
            <AlertTriangle className="h-5 w-5" aria-hidden="true" />
            <span className="font-medium text-sm">
              {criticalCount} critical alert{criticalCount > 1 ? 's' : ''} require immediate attention
            </span>
          </div>
        </div>
      )}

      <div className="space-y-2" role="list" aria-label="Alerts list">
        {alerts.map((alert) => renderAlert(alert, getSeverityColor(alert.severity)))}
      </div>

      <Button
        variant="outline"
        size="sm"
        className="w-full"
        onClick={() => navigate(buildMetricsLink())}
      >
        View All Alerts
      </Button>
    </DashboardWidgetFrame>
  );
}

export const ActiveAlertsWidget = withSectionErrorBoundary(ActiveAlertsWidgetBase, 'Active Alerts Widget');
