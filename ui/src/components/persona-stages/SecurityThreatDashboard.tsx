import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '../ui/card';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import {
  AlertTriangle,
  Shield,
  Activity,
  Eye,
  RefreshCw,
  CheckCircle,
  XCircle,
  TrendingUp,
  Clock,
  BarChart3,
  AlertCircle,
  Ban,
} from 'lucide-react';
import apiClient from '../../api/client';
import { Alert, AlertFilters, AnomalyDetectionStatus, AccessPattern } from '../../api/types';
import { toast } from 'sonner';
import { logger } from '../../utils/logger';

interface PolicyViolation {
  id?: string;
  rule: string;
  message: string;
  reason?: string;
  severity?: string;
  resolved?: boolean;
  timestamp?: string;
}

interface SecurityThreatDashboardProps {
  tenantId?: string;
}

export default function SecurityThreatDashboard({ tenantId }: SecurityThreatDashboardProps) {
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [anomalyStatus, setAnomalyStatus] = useState<AnomalyDetectionStatus | null>(null);
  const [accessPatterns, setAccessPatterns] = useState<AccessPattern[]>([]);
  const [policyViolations, setPolicyViolations] = useState<PolicyViolation[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const loadThreatData = useCallback(async () => {
    setIsLoading(true);
    try {
      // Load active alerts
      const alertFilters: AlertFilters = {
        tenant_id: tenantId,
        limit: 20,
      };
      const alertsList = await apiClient.listAlerts(alertFilters);
      setAlerts(alertsList.filter((a) => !a.resolved));

      // Load anomaly detection status
      try {
        const anomaly = await apiClient.getAnomalyDetectionStatus();
        setAnomalyStatus(anomaly);
      } catch {
        // Anomaly detection may not be available
        setAnomalyStatus({
          enabled: false,
          last_scan: new Date().toISOString(),
          anomalies_detected: 0,
          model_version: 'N/A',
        });
      }

      // Load access patterns
      try {
        const patterns = await apiClient.getAccessPatterns(tenantId);
        setAccessPatterns(patterns);
      } catch {
        // Generate mock access patterns if endpoint not available
        setAccessPatterns(generateMockAccessPatterns());
      }

      // Load compliance audit for violations
      try {
        const compliance = await apiClient.getComplianceAudit();
        setPolicyViolations(compliance.violations || []);
      } catch {
        setPolicyViolations([]);
      }

      logger.info('Threat data loaded', {
        component: 'SecurityThreatDashboard',
        operation: 'loadThreatData',
        alertCount: alertsList.length,
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to load threat data';
      logger.error('Failed to load threat data', {
        component: 'SecurityThreatDashboard',
        operation: 'loadThreatData',
        error: errorMessage,
      });
      toast.error(errorMessage);
    } finally {
      setIsLoading(false);
    }
  }, [tenantId]);

  useEffect(() => {
    loadThreatData();
    // Auto-refresh every 30 seconds
    const interval = setInterval(loadThreatData, 30000);
    return () => clearInterval(interval);
  }, [loadThreatData]);

  const generateMockAccessPatterns = (): AccessPattern[] => {
    return Array.from({ length: 24 }, (_, i) => ({
      hour: i,
      count: Math.floor(Math.random() * 100) + 10,
      anomaly_score: Math.random() * 0.3,
    }));
  };

  const handleAcknowledgeAlert = async (alertId: string) => {
    try {
      await apiClient.acknowledgeAlert(alertId, {
        alert_id: alertId,
        acknowledged_by: 'current-user',
      });
      toast.success('Alert acknowledged');
      loadThreatData();
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to acknowledge alert';
      toast.error(errorMessage);
    }
  };

  const handleResolveAlert = async (alertId: string) => {
    try {
      await apiClient.resolveAlert(alertId, {
        alert_id: alertId,
        resolved_by: 'current-user',
      });
      toast.success('Alert resolved');
      loadThreatData();
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to resolve alert';
      toast.error(errorMessage);
    }
  };

  const getSeverityColor = (severity: string) => {
    switch (severity) {
      case 'critical':
        return 'bg-red-600 text-white';
      case 'high':
        return 'bg-orange-600 text-white';
      case 'medium':
        return 'bg-amber-600 text-white';
      case 'low':
        return 'bg-blue-600 text-white';
      default:
        return 'bg-gray-600 text-white';
    }
  };

  const getSeverityIcon = (severity: string) => {
    switch (severity) {
      case 'critical':
        return <XCircle className="w-4 h-4" />;
      case 'high':
        return <AlertTriangle className="w-4 h-4" />;
      case 'medium':
        return <AlertCircle className="w-4 h-4" />;
      default:
        return <Activity className="w-4 h-4" />;
    }
  };

  const criticalAlerts = alerts.filter((a) => a.severity === 'critical').length;
  const highAlerts = alerts.filter((a) => a.severity === 'high').length;
  const activeViolations = policyViolations.filter((v) => !v.resolved).length;

  // Find max count for access pattern visualization
  const maxAccessCount = Math.max(...accessPatterns.map((p) => p.count), 1);

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold tracking-tight">Security Threat Dashboard</h2>
          <p className="text-muted-foreground">
            Monitor security threats and anomalies
          </p>
        </div>
        <Button onClick={loadThreatData} disabled={isLoading}>
          <RefreshCw className={`w-4 h-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
          Refresh
        </Button>
      </div>

      {/* Overview Stats */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Active Alerts</p>
                <p className="text-3xl font-bold">{alerts.length}</p>
              </div>
              <AlertTriangle className={`w-8 h-8 ${alerts.length > 0 ? 'text-amber-600' : 'text-green-600'}`} />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Critical Alerts</p>
                <p className="text-3xl font-bold text-red-600">{criticalAlerts}</p>
              </div>
              <XCircle className={`w-8 h-8 ${criticalAlerts > 0 ? 'text-red-600' : 'text-muted-foreground'}`} />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Policy Violations</p>
                <p className="text-3xl font-bold">{activeViolations}</p>
              </div>
              <Ban className={`w-8 h-8 ${activeViolations > 0 ? 'text-orange-600' : 'text-green-600'}`} />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Anomalies Detected</p>
                <p className="text-3xl font-bold">{anomalyStatus?.anomalies_detected || 0}</p>
              </div>
              <Eye className={`w-8 h-8 ${(anomalyStatus?.anomalies_detected || 0) > 0 ? 'text-amber-600' : 'text-green-600'}`} />
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Anomaly Detection Status */}
      <Card>
        <CardHeader>
          <CardTitle className="text-lg flex items-center gap-2">
            <Eye className="w-5 h-5" />
            Anomaly Detection Status
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
            <div className="p-3 bg-muted rounded-lg">
              <p className="text-sm text-muted-foreground">Status</p>
              <div className="flex items-center gap-2 mt-1">
                {anomalyStatus?.enabled ? (
                  <>
                    <CheckCircle className="w-4 h-4 text-green-600" />
                    <span className="font-medium text-green-600">Active</span>
                  </>
                ) : (
                  <>
                    <XCircle className="w-4 h-4 text-red-600" />
                    <span className="font-medium text-red-600">Inactive</span>
                  </>
                )}
              </div>
            </div>

            <div className="p-3 bg-muted rounded-lg">
              <p className="text-sm text-muted-foreground">Last Scan</p>
              <p className="font-medium mt-1">
                {anomalyStatus?.last_scan
                  ? new Date(anomalyStatus.last_scan).toLocaleString()
                  : 'N/A'}
              </p>
            </div>

            <div className="p-3 bg-muted rounded-lg">
              <p className="text-sm text-muted-foreground">Anomalies Found</p>
              <p className="font-medium mt-1">{anomalyStatus?.anomalies_detected || 0}</p>
            </div>

            <div className="p-3 bg-muted rounded-lg">
              <p className="text-sm text-muted-foreground">Model Version</p>
              <p className="font-medium mt-1">{anomalyStatus?.model_version || 'N/A'}</p>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Access Pattern Visualization */}
      <Card>
        <CardHeader>
          <CardTitle className="text-lg flex items-center gap-2">
            <BarChart3 className="w-5 h-5" />
            Access Pattern Visualization
          </CardTitle>
          <CardDescription>
            Hourly access patterns with anomaly scoring
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex items-end gap-1 h-40">
            {accessPatterns.map((pattern) => (
              <div
                key={pattern.hour}
                className="flex-1 flex flex-col items-center"
              >
                <div
                  className={`w-full rounded-t transition-all ${
                    pattern.anomaly_score > 0.2
                      ? 'bg-red-500'
                      : pattern.anomaly_score > 0.1
                      ? 'bg-amber-500'
                      : 'bg-blue-500'
                  }`}
                  style={{
                    height: `${(pattern.count / maxAccessCount) * 100}%`,
                    minHeight: '4px',
                  }}
                  title={`Hour ${pattern.hour}: ${pattern.count} requests, anomaly: ${(pattern.anomaly_score * 100).toFixed(1)}%`}
                />
              </div>
            ))}
          </div>
          <div className="flex justify-between mt-2 text-xs text-muted-foreground">
            <span>00:00</span>
            <span>06:00</span>
            <span>12:00</span>
            <span>18:00</span>
            <span>24:00</span>
          </div>
          <div className="flex gap-4 mt-4 text-xs">
            <div className="flex items-center gap-1">
              <div className="w-3 h-3 bg-blue-500 rounded" />
              <span>Normal</span>
            </div>
            <div className="flex items-center gap-1">
              <div className="w-3 h-3 bg-amber-500 rounded" />
              <span>Elevated</span>
            </div>
            <div className="flex items-center gap-1">
              <div className="w-3 h-3 bg-red-500 rounded" />
              <span>Anomaly</span>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Active Alerts List */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center justify-between">
            <span className="flex items-center gap-2">
              <AlertTriangle className="w-5 h-5" />
              Active Alerts
            </span>
            {alerts.length > 0 && (
              <div className="flex gap-2">
                <Badge variant="outline" className="bg-red-50 text-red-700">
                  {criticalAlerts} Critical
                </Badge>
                <Badge variant="outline" className="bg-orange-50 text-orange-700">
                  {highAlerts} High
                </Badge>
              </div>
            )}
          </CardTitle>
          <CardDescription>
            Security alerts requiring attention
          </CardDescription>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <div className="flex items-center justify-center py-12">
              <RefreshCw className="w-8 h-8 animate-spin text-muted-foreground" />
            </div>
          ) : alerts.length === 0 ? (
            <div className="text-center py-12 text-muted-foreground">
              <CheckCircle className="w-12 h-12 mx-auto mb-3 text-green-600 opacity-50" />
              <p>No active alerts</p>
              <p className="text-sm">All systems operating normally</p>
            </div>
          ) : (
            <div className="space-y-3">
              {alerts.map((alert) => (
                <div
                  key={alert.id}
                  className={`p-4 border rounded-lg ${
                    alert.severity === 'critical'
                      ? 'border-red-300 bg-red-50/50'
                      : alert.severity === 'high'
                      ? 'border-orange-300 bg-orange-50/50'
                      : 'bg-background'
                  }`}
                >
                  <div className="flex items-start justify-between">
                    <div className="flex items-start gap-3">
                      <div className={getSeverityColor(alert.severity)}>
                        {getSeverityIcon(alert.severity)}
                      </div>
                      <div>
                        <div className="flex items-center gap-2 mb-1">
                          <Badge className={getSeverityColor(alert.severity)}>
                            {alert.severity.toUpperCase()}
                          </Badge>
                          <span className="font-medium">{alert.title || alert.message}</span>
                        </div>
                        <p className="text-sm text-muted-foreground">
                          {alert.message}
                        </p>
                        <div className="flex items-center gap-4 mt-2 text-xs text-muted-foreground">
                          <span className="flex items-center gap-1">
                            <Clock className="w-3 h-3" />
                            {new Date(alert.timestamp).toLocaleString()}
                          </span>
                          {alert.source && (
                            <span>Source: {alert.source}</span>
                          )}
                        </div>
                      </div>
                    </div>
                    <div className="flex gap-2">
                      {!alert.acknowledged && (
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => handleAcknowledgeAlert(alert.id)}
                        >
                          Acknowledge
                        </Button>
                      )}
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => handleResolveAlert(alert.id)}
                      >
                        Resolve
                      </Button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Policy Violations */}
      {policyViolations.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Ban className="w-5 h-5" />
              Recent Policy Violations
            </CardTitle>
            <CardDescription>
              Policy compliance issues detected
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-3">
              {policyViolations.slice(0, 5).map((violation, index) => (
                <div
                  key={violation.id || `violation-${index}`}
                  className={`p-4 border rounded-lg ${
                    violation.resolved ? 'opacity-60 bg-muted/50' : 'bg-background'
                  }`}
                >
                  <div className="flex items-start justify-between">
                    <div>
                      <div className="flex items-center gap-2 mb-1">
                        <Badge
                          className={
                            violation.severity === 'critical'
                              ? 'bg-red-600 text-white'
                              : violation.severity === 'high'
                              ? 'bg-orange-600 text-white'
                              : 'bg-amber-600 text-white'
                          }
                        >
                          {violation.severity?.toUpperCase() || 'MEDIUM'}
                        </Badge>
                        <span className="font-medium">{violation.rule}</span>
                        {violation.resolved && (
                          <Badge variant="outline" className="bg-green-50 text-green-700">
                            Resolved
                          </Badge>
                        )}
                      </div>
                      <p className="text-sm text-muted-foreground">
                        {violation.message || violation.reason}
                      </p>
                      {violation.timestamp && (
                        <div className="text-xs text-muted-foreground mt-2">
                          {new Date(violation.timestamp).toLocaleString()}
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
