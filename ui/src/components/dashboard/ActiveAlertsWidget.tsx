import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import { AlertTriangle, Bell, CheckCircle, Clock, Loader2 } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useTenant } from '@/layout/LayoutProvider';
import { usePolling } from '@/hooks/usePolling';
import { useServiceStatus } from '@/hooks/useServiceStatus';
import { useRelativeTime } from '@/hooks/useTimestamp';
import apiClient from '@/api/client';
import type { Alert as ApiAlert } from '@/api/types';
import { logger, toError } from '@/utils/logger';

interface Alert {
  id: string;
  severity: 'critical' | 'high' | 'medium' | 'low';
  title: string;
  created_at: string; // Store ISO timestamp for dynamic relative time calculation
  acknowledged: boolean;
}

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
    title: apiAlert.title,
    created_at: apiAlert.created_at, // Store ISO timestamp for dynamic calculation
    acknowledged: apiAlert.status === 'acknowledged',
  };
}

export function ActiveAlertsWidget() {
  const navigate = useNavigate();
  const { selectedTenant } = useTenant();

  // Fetch service status for service failure alerts (shared subscription)
  const { status } = useServiceStatus();

  // Fetch alerts from API with polling
  const {
    data: apiAlerts,
    isLoading,
    error,
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

  // All alerts from API are active (unacknowledged) since we filter by status: 'active'
  const activeAlerts = alerts;
  const criticalCount = activeAlerts.filter(a => a.severity === 'critical').length;

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

  return (
    <Card aria-labelledby="active-alerts-title">
      <CardHeader>
        <CardTitle id="active-alerts-title" className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Bell className="h-5 w-5" aria-hidden="true" />
            <span>Active Alerts</span>
          </div>
          <Badge variant={activeAlerts.length > 0 ? 'destructive' : 'default'}>
            {activeAlerts.length}
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3" aria-live="polite">
        {isLoading && !apiAlerts ? (
          <div className="text-center py-8">
            <Loader2 className="h-8 w-8 text-muted-foreground mx-auto mb-2 animate-spin" aria-hidden="true" />
            <p className="text-sm text-muted-foreground">Loading alerts...</p>
          </div>
        ) : error ? (
          <div className="text-center py-8">
            <AlertTriangle className="h-8 w-8 text-gray-500 mx-auto mb-2 opacity-50" aria-hidden="true" />
            <p className="text-sm text-muted-foreground">Failed to load alerts</p>
            {!selectedTenant && (
              <p className="text-xs text-muted-foreground mt-1">Please select a tenant</p>
            )}
          </div>
        ) : alerts.length === 0 ? (
          <div className="text-center py-8">
            <CheckCircle className="h-12 w-12 text-gray-400 mx-auto mb-2 opacity-20" aria-hidden="true" />
            <p className="text-sm text-muted-foreground">No active alerts</p>
          </div>
        ) : (
          <>
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
              {alerts.map((alert) => {
                const Icon = getSeverityIcon(alert.severity);
                // Calculate relative time at render for freshness (updates on each render)
                const relativeTime = useRelativeTime(alert.created_at);
                return (
                  <div
                    key={alert.id}
                    className={`p-3 rounded-lg border ${getSeverityColor(alert.severity)}`}
                  >
                    <div className="flex items-start gap-2" role="listitem">
                      <Icon className="h-4 w-4 mt-0.5 flex-shrink-0" aria-hidden="true" />
                      <div className="flex-1 min-w-0">
                        <p className="text-sm font-medium">{alert.title}</p>
                        <div className="flex items-center gap-2 mt-1">
                          <Clock className="h-3 w-3 text-muted-foreground" aria-hidden="true" />
                          <span className="text-xs text-muted-foreground">{relativeTime}</span>
                        </div>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>

            <Button
              variant="outline"
              size="sm"
              className="w-full"
              onClick={() => navigate('/monitoring')}
            >
              View All Alerts
            </Button>
          </>
        )}
      </CardContent>
    </Card>
  );
}
