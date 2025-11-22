// 【ui/src/components/AlertsPage.tsx§131-134】 - Replace manual polling with standardized hook
import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from './ui/card';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Badge } from './ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Slider } from './ui/slider';

import { HelpTooltip } from './ui/help-tooltip';
import { errorRecoveryTemplates } from '@/components/ui/error-recovery';

import {
  Bell,
  AlertTriangle,
  TrendingUp,
  Activity,
  Plus,
  Trash2,
  Edit,
  Save,
  X,
  CheckCircle,
  Clock,
  Target,
  Zap
} from 'lucide-react';
import apiClient from '../api/client';
import { SystemMetrics } from '../api/types';
import { toast } from 'sonner';

import { logger, toError } from '../utils/logger';
import { usePolling } from '../hooks/usePolling';
import { useTenant } from '@/layout/LayoutProvider';
import type { Alert } from '@/api/types';

interface AlertsPageProps {
  selectedTenant?: string;
}

interface AlertRule {
  id: string;
  name: string;
  enabled: boolean;
  metric: string;
  condition: 'gt' | 'lt' | 'eq';
  threshold: number;
  duration_seconds: number;
  severity: 'critical' | 'high' | 'medium' | 'low' | 'info';
  notification_channels: string[];
  description: string;
}


export function AlertsPage({ selectedTenant: tenantProp }: AlertsPageProps) {
  const { selectedTenant } = useTenant();
  const effectiveTenant = tenantProp ?? selectedTenant;

  const [alertRules, setAlertRules] = useState<AlertRule[]>([]);
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [metrics, setMetrics] = useState<SystemMetrics | null>(null);
  const [editingRule, setEditingRule] = useState<AlertRule | null>(null);
  const [isCreatingRule, setIsCreatingRule] = useState(false);
  const [isLoading, setIsLoading] = useState(false);

  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);
  const CHANNEL_OPTIONS = ['dashboard', 'log', 'slack', 'pagerduty'] as const;

  // 【ui/src/hooks/usePolling.ts】 - Standardized polling hook for metrics
  const { 
    data: metricsData, 
    lastUpdated: metricsLastUpdated 
  } = usePolling(
    () => apiClient.getSystemMetrics(),
    'fast', // Real-time updates for alerts
    {
      showLoadingIndicator: false,
      onError: (err) => {
        logger.error('Failed to load system metrics for alerts', {
          component: 'AlertsPage',
          operation: 'loadMetrics',
          tenantId: effectiveTenant,
        }, err);
      }
    }
  );

  // Metrics loading now handled by usePolling hook

  const evaluateAlertRules = useCallback((currentMetrics: SystemMetrics) => {
    alertRules.forEach(rule => {
      if (!rule.enabled) return;

      const metricValue = currentMetrics[rule.metric as keyof SystemMetrics] as number | undefined;
      if (metricValue === undefined) return;

      let shouldAlert = false;
      switch (rule.condition) {
        case 'gt':
          shouldAlert = metricValue > rule.threshold;
          break;
        case 'lt':
          shouldAlert = metricValue < rule.threshold;
          break;
        case 'eq':
          shouldAlert = metricValue === rule.threshold;
          break;
      }

      if (shouldAlert) {
        // Note: Real-time alert generation disabled - alerts should come from backend
        // Future enhancement: Implement SSE stream endpoint for real-time alert updates
      }
    });
  }, [alertRules]);

  const loadAlerts = useCallback(async () => {
    try {
      const alerts = await apiClient.listAlerts({ limit: 50 });
      setAlerts(alerts);
      setErrorRecovery(null);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to load alerts';
      logger.error('Failed to load alerts', {
        component: 'AlertsPage',
        operation: 'loadAlerts',
        tenantId: effectiveTenant,
      }, toError(error));
      setAlerts([]);
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          error instanceof Error ? error : new Error(errorMessage),
          () => {
            setErrorRecovery(null);
            void loadAlerts();
          }
        )
      );
    }
  }, [effectiveTenant]);

  // Real-time alert streaming using EventSource
  //
  // Citations:
  // - SSE pattern: [source: ui/src/hooks/useActivityFeed.ts L350-L437]
  // - Backend endpoint: [source: crates/adapteros-server-api/src/handlers.rs L12929-12935]
  // - Event format: [source: crates/adapteros-server-api/src/types.rs L1732-1760]
  useEffect(() => {
    const base = (import.meta as any)?.env?.VITE_SSE_URL
      ? `http://${(import.meta as any).env.VITE_SSE_URL}`
      : ((import.meta as any)?.env?.VITE_API_URL || '/api');
    const url = `${base}/v1/monitoring/alerts/stream`;
    
    let eventSource: EventSource | null = null;
    let reconnectAttempts = 0;
    const maxReconnect = 5;
    const baseDelay = 1000;

    const connectSSE = () => {
      try {
        eventSource = new EventSource(url);
        
        eventSource.addEventListener('alert', (event) => {
          try {
            const alert = JSON.parse((event as MessageEvent).data);
            setAlerts((prev) => {
              // Update existing alert or add new one
              const existingIndex = prev.findIndex(a => a.id === alert.id);
              if (existingIndex >= 0) {
                const updated = [...prev];
                updated[existingIndex] = alert;
                return updated;
              } else {
                return [alert, ...prev].slice(0, 100); // Keep last 100 alerts
              }
            });
            reconnectAttempts = 0;
          } catch (err) {
            logger.error('Failed to parse alert SSE payload', {
              component: 'AlertsPage',
              operation: 'sse_alert_parse',
            }, toError(err));
          }
        });
        
        eventSource.addEventListener('open', () => {
          reconnectAttempts = 0;
          logger.info('Alert SSE stream connected', {
            component: 'AlertsPage',
            operation: 'sse_connect',
          });
        });
        
        eventSource.addEventListener('error', (evt: any) => {
          reconnectAttempts++;
          const unauthorized = evt?.status === 401 || evt?.code === 401;
          if (unauthorized) {
            logger.error('Alert SSE unauthorized', {
              component: 'AlertsPage',
              operation: 'sse_error',
            }, new Error('Unauthorized'));
            if (eventSource) {
              eventSource.close();
              eventSource = null;
            }
            return;
          }
          
          if (reconnectAttempts >= maxReconnect) {
            logger.error('Max SSE reconnect threshold reached (alerts)', {
              component: 'AlertsPage',
              operation: 'sse_reconnect',
              reconnectAttempts,
              maxReconnect,
            });
            if (eventSource) {
              eventSource.close();
              eventSource = null;
            }
            // Fallback to polling
            const fallbackInterval = setInterval(() => {
              void loadAlerts();
            }, 5000);
            return () => clearInterval(fallbackInterval);
          }
          
          const delay = Math.min(baseDelay * Math.pow(2, reconnectAttempts - 1), 30000);
          if (eventSource) {
            eventSource.close();
            eventSource = null;
          }
          
          setTimeout(() => {
            if (eventSource === null) {
              connectSSE();
            }
          }, delay);
        });
      } catch (err) {
        logger.error('Failed to initialize alert SSE', {
          component: 'AlertsPage',
          operation: 'sse_init',
        }, toError(err));
      }
    };
    
    // Initial load
    void loadAlerts();
    
    // Connect to SSE stream
    connectSSE();
    
    // Cleanup
    return () => {
      if (eventSource) {
        eventSource.close();
        eventSource = null;
      }
    };
  }, [effectiveTenant, loadAlerts]);

  // Update metrics and evaluate alert rules when polling data arrives
  useEffect(() => {
    if (!metricsData) return;
    
    setMetrics(metricsData);
    evaluateAlertRules(metricsData);
  }, [metricsData, evaluateAlertRules]);

  const [isLoadingRules, setIsLoadingRules] = useState(false);

  const loadAlertRules = useCallback(async () => {
    setIsLoadingRules(true);
    try {
      // Load monitoring rules from backend
      const rules = await apiClient.listMonitoringRules(effectiveTenant);

      // Transform MonitoringRule to AlertRule
      const transformedRules: AlertRule[] = rules.map(rule => ({
        id: rule.id,
        name: rule.name,
        enabled: rule.is_active,
        metric: rule.metric_name,
        condition: rule.threshold_operator === 'gt' ? 'gt' : rule.threshold_operator === 'lt' ? 'lt' : 'eq',
        threshold: rule.threshold_value,
        duration_seconds: rule.evaluation_window_seconds,
        severity: rule.severity as AlertRule['severity'],
        notification_channels: ['dashboard'], // Default for now - could be derived from rule.notification_channels
        description: `Monitor ${rule.metric_name} ${rule.threshold_operator} ${rule.threshold_value}`, // Generate description since MonitoringRule doesn't have one
      }));

      setAlertRules(transformedRules);
      setErrorRecovery(null);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to load alert rules';
      logger.error('Failed to load alert rules', {
        component: 'AlertsPage',
        operation: 'loadAlertRules',
        tenantId: effectiveTenant,
      }, toError(error));
      // Keep empty rules on API failure - no fallback to mock data
      setAlertRules([]);
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          error instanceof Error ? error : new Error(errorMessage),
          () => {
            setErrorRecovery(null);
            void loadAlertRules();
          }
        )
      );
    } finally {
      setIsLoadingRules(false);
    }
  }, [effectiveTenant]);

  const handleToggleRule = async (ruleId: string) => {
    try {
      const rule = alertRules.find(r => r.id === ruleId);
      if (!rule) return;

      await apiClient.updateMonitoringRule(ruleId, {
        enabled: !rule.enabled,
      });

      // Update local state after successful API call
      setAlertRules(prev =>
        prev.map(r =>
          r.id === ruleId ? { ...r, enabled: !r.enabled } : r
        )
      );
      toast.success('Alert rule updated');
    } catch (error) {
      logger.error('Failed to toggle alert rule', {
        component: 'AlertsPage',
        operation: 'handleToggleRule',
        ruleId,
      }, toError(error));
      toast.error('Failed to update alert rule');
    }
  };

  const handleDeleteRule = async (ruleId: string) => {
    try {
      await apiClient.deleteMonitoringRule(ruleId);
      setAlertRules(prev => prev.filter(rule => rule.id !== ruleId));
      toast.success('Alert rule deleted');
    } catch (error) {
      logger.error('Failed to delete alert rule', {
        component: 'AlertsPage',
        operation: 'handleDeleteRule',
        ruleId,
      }, toError(error));
      toast.error('Failed to delete alert rule');
    }
  };

  const handleSaveRule = async (rule: AlertRule) => {
    try {
      if (isCreatingRule) {
        // Create new rule via API
        const createRequest = {
          tenant_id: effectiveTenant || 'default',
          name: rule.name,
          rule_type: 'threshold' as const,
          metric_name: rule.metric,
          threshold_value: rule.threshold,
          threshold_operator: rule.condition,
          severity: rule.severity,
          evaluation_window_seconds: rule.duration_seconds,
          cooldown_seconds: 60,
          is_active: rule.enabled,
          notification_channels: rule.notification_channels.reduce((acc, channel) => {
            acc[channel] = {
              type: channel as 'email' | 'webhook' | 'slack' | 'pagerduty',
              enabled: true,
            };
            return acc;
          }, {} as Record<string, any>),
        };

        await apiClient.createMonitoringRule(createRequest);
        toast.success('Alert rule created');
        // Reload rules from backend
        await loadAlertRules();
      } else {
        // Map severity to API-compatible format (API doesn't support 'info')
        const apiSeverity = rule.severity === 'info' ? 'low' : rule.severity as 'low' | 'medium' | 'high' | 'critical';

        await apiClient.updateMonitoringRule(rule.id, {
          name: rule.name,
          description: rule.description,
          enabled: rule.enabled,
          severity: apiSeverity,
        });

        // Update local state after successful API call
        setAlertRules(prev =>
          prev.map(r => (r.id === rule.id ? rule : r))
        );
        toast.success('Alert rule updated');
      }
      setEditingRule(null);
      setIsCreatingRule(false);
    } catch (error) {
      logger.error('Failed to save alert rule', {
        component: 'AlertsPage',
        operation: 'handleSaveRule',
        isCreating: isCreatingRule,
      }, toError(error));
      toast.error('Failed to save alert rule');
    }
  };

  const handleAcknowledgeAlert = (alertId: string) => {
    setAlerts(prev =>
      prev.map(alert =>
        alert.id === alertId ? { ...alert, acknowledged: true } : alert
      )
    );
    toast.success('Alert acknowledged');
  };

  const handleResolveAlert = (alertId: string) => {
    setAlerts(prev =>
      prev.map(alert =>
        alert.id === alertId
          ? { ...alert, resolved_at: new Date().toISOString() }
          : alert
      )
    );
    toast.success('Alert resolved');
  };

  const getSeverityColor = (severity: Alert['severity']) => {
    switch (severity) {
      case 'critical':
        return 'text-red-600 bg-red-50 border-red-200';
      case 'error':
        return 'text-orange-600 bg-orange-50 border-orange-200';
      case 'warning':
        return 'text-amber-600 bg-amber-50 border-amber-200';
      case 'info':
        return 'text-gray-600 bg-gray-50 border-gray-200';
    }
  };

  const activeAlerts = alerts.filter(a => !a.resolved_at);
  const criticalAlerts = activeAlerts.filter(a => a.severity === 'critical').length;
  const unacknowledgedAlerts = activeAlerts.filter(a => !(a.acknowledged_by || a.acknowledged_at)).length;

  return (
    <div className="space-y-6">
      {/* Error Recovery */}
      {errorRecovery}

      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-3xl font-bold tracking-tight">Alerts & Monitoring</h2>
          <p className="text-muted-foreground">
            Configure alert rules and monitor system health
          </p>
        </div>
        <Button
          onClick={() => {
            setIsCreatingRule(true);
            setEditingRule({
              id: '',
              name: '',
              enabled: true,
              metric: 'memory_usage_pct',
              condition: 'gt',
              threshold: 0,
              duration_seconds: 60,
              severity: 'medium',
              notification_channels: ['dashboard'],
              description: ''
            });
          }}
        >
          <Plus className="w-4 h-4 mr-2" />
          Create Alert Rule
        </Button>
      </div>

      {/* Alert Summary */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-6">
        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Active Alerts</p>
                <p className="text-3xl font-bold">{activeAlerts.length}</p>
              </div>
              <Bell className="w-8 h-8 text-amber-600" />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Critical</p>
                <p className="text-3xl font-bold">{criticalAlerts}</p>
              </div>
              <AlertTriangle className="w-8 h-8 text-red-600" />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Unacknowledged</p>
                <p className="text-3xl font-bold">{unacknowledgedAlerts}</p>
              </div>
              <Clock className="w-8 h-8 text-orange-600" />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Alert Rules</p>
                <p className="text-3xl font-bold">
                  {alertRules.filter(r => r.enabled).length}/{alertRules.length}
                </p>
              </div>
              <Target className="w-8 h-8 text-primary" />
            </div>
          </CardContent>
        </Card>
      </div>

      <Tabs defaultValue="active" className="space-y-4">
        <TabsList>
          <TabsTrigger value="active">
            <Bell className="w-4 h-4 mr-2" />
            Active Alerts ({activeAlerts.length})
          </TabsTrigger>
          <TabsTrigger value="rules">
            <Target className="w-4 h-4 mr-2" />
            Alert Rules
          </TabsTrigger>
          <TabsTrigger value="metrics">
            <Activity className="w-4 h-4 mr-2" />
            Current Metrics
          </TabsTrigger>
        </TabsList>

        {/* Active Alerts */}
        <TabsContent value="active" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Active Alerts</CardTitle>
              <CardDescription>
                Alerts requiring attention
              </CardDescription>
            </CardHeader>
            <CardContent>
              {activeAlerts.length === 0 ? (
                <div className="text-center py-12 text-muted-foreground">
                  <CheckCircle className="w-12 h-12 mx-auto mb-3 opacity-20" />
                  <p>No active alerts</p>
                </div>
              ) : (
                <div className="space-y-3">
                  {activeAlerts.map(alert => (
                    <div
                      key={alert.id}
                      className={`
                        p-4 border-2 rounded-lg
                        ${getSeverityColor(alert.severity)}
                        ${(alert.acknowledged_by || alert.acknowledged_at) ? 'opacity-60' : ''}
                      `}
                    >
                      <div className="flex items-start justify-between">
                        <div className="flex-1">
                          <div className="flex items-center gap-2 mb-2">
                            <AlertTriangle className="w-5 h-5" />
                            <span className="font-semibold">{alert.title || alert.rule_name || 'Alert'}</span>
                            <Badge variant="outline">
                              {alert.severity.toUpperCase()}
                            </Badge>
                            {(alert.acknowledged_by || alert.acknowledged_at) && (
                              <Badge variant="outline" className="bg-blue-50">
                                Acknowledged
                              </Badge>
                            )}
                          </div>
                          <p className="text-sm mb-2">{alert.message}</p>
                          <div className="flex items-center gap-4 text-xs">
                            {alert.metric_value !== undefined && (
                              <span>
                                Current: {alert.metric_value}
                              </span>
                            )}
                            {alert.threshold_value !== undefined && (
                              <span>
                                Threshold: {alert.threshold_value}
                              </span>
                            )}
                            {alert.created_at && (
                              <span>
                                Created: {new Date(alert.created_at).toLocaleString()}
                              </span>
                            )}
                            {alert.triggered_at && !alert.created_at && (
                              <span>
                                Triggered: {new Date(alert.triggered_at).toLocaleString()}
                              </span>
                            )}
                          </div>
                        </div>
                        <div className="flex gap-2">
                          {!(alert.acknowledged_by || alert.acknowledged_at) && (
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
        </TabsContent>

        {/* Alert Rules */}
        <TabsContent value="rules" className="space-y-4">

          {isLoadingRules && (
            <Card>
              <CardContent className="pt-6">
                <div className="flex items-center justify-center p-8">
                  <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
                  <span className="ml-2 text-muted-foreground">Loading alert rules...</span>
                </div>
              </CardContent>
            </Card>
          )}

          {(editingRule || isCreatingRule) && (
            <Card>
              <CardHeader>
                <CardTitle>
                  {isCreatingRule ? 'Create Alert Rule' : 'Edit Alert Rule'}
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="space-y-2">
                  <Label htmlFor="rule-name">Rule Name</Label>
                  <Input
                    id="rule-name"
                    value={editingRule?.name || ''}
                    onChange={(e) =>
                      setEditingRule(prev => prev ? { ...prev, name: e.target.value } : null)
                    }
                  />
                </div>

                <div className="space-y-2">
                  <Label htmlFor="rule-description">Description</Label>
                  <Input
                    id="rule-description"
                    value={editingRule?.description || ''}
                    onChange={(e) =>
                      setEditingRule(prev => prev ? { ...prev, description: e.target.value } : null)
                    }
                  />
                </div>

                <div className="grid grid-cols-2 gap-4">
                  <div className="space-y-2">
                    <Label htmlFor="rule-metric">Metric</Label>
                    <select
                      id="rule-metric"
                      value={editingRule?.metric || ''}
                      onChange={(e) =>
                        setEditingRule(prev => prev ? { ...prev, metric: e.target.value } : null)
                      }
                      className="w-full rounded-md border border-input bg-background px-3 py-2"
                    >
                      <option value="memory_usage_pct">Memory Usage %</option>
                      <option value="latency_p95_ms">P95 Latency (ms)</option>
                      <option value="tokens_per_second">Tokens/Second</option>
                      <option value="adapter_count">Adapter Count</option>
                      <option value="active_sessions">Active Sessions</option>
                    </select>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="rule-condition">Condition</Label>
                    <select
                      id="rule-condition"
                      value={editingRule?.condition || ''}
                      onChange={(e) =>
                        setEditingRule(prev => prev ? { ...prev, condition: e.target.value as any } : null)
                      }
                      className="w-full rounded-md border border-input bg-background px-3 py-2"
                    >
                      <option value="gt">Greater Than</option>
                      <option value="lt">Less Than</option>
                      <option value="eq">Equal To</option>
                    </select>
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-4">
                  <div className="space-y-2">
                    <Label htmlFor="rule-threshold">Threshold</Label>
                    <Input
                      id="rule-threshold"
                      type="number"
                      value={editingRule?.threshold || 0}
                      onChange={(e) =>
                        setEditingRule(prev => prev ? { ...prev, threshold: parseFloat(e.target.value) } : null)
                      }
                    />
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="rule-duration">Duration (seconds)</Label>
                    <Input
                      id="rule-duration"
                      type="number"
                      value={editingRule?.duration_seconds || 0}
                      onChange={(e) =>
                        setEditingRule(prev => prev ? { ...prev, duration_seconds: parseInt(e.target.value) } : null)
                      }
                    />
                  </div>
                </div>


              <div className="space-y-2">
                <div className="flex items-center gap-2">
                  <Label>Notification Channels</Label>
                  <HelpTooltip helpId="alerts">
                    <span className="text-xs text-muted-foreground">What are channels?</span>
                  </HelpTooltip>
                </div>
                <div className="flex flex-wrap gap-2">
                  {CHANNEL_OPTIONS.map((ch) => {
                    const selected = editingRule?.notification_channels?.includes(ch) ?? false;
                    return (
                      <Button
                        key={ch}
                        type="button"
                        size="sm"
                        variant={selected ? 'default' : 'outline'}
                        onClick={() =>
                          setEditingRule(prev => prev ? {
                            ...prev,
                            notification_channels: selected
                              ? prev.notification_channels.filter(c => c !== ch)
                              : [...(prev.notification_channels || []), ch]
                          } : null)
                        }
                      >
                        {ch}
                      </Button>
                    );
                  })}
                </div>
              </div>


                <div className="space-y-2">
                  <Label htmlFor="rule-severity">Severity</Label>
                  <select
                    id="rule-severity"
                    value={editingRule?.severity || ''}
                    onChange={(e) =>
                      setEditingRule(prev => prev ? { ...prev, severity: e.target.value as any } : null)
                    }
                    className="w-full rounded-md border border-input bg-background px-3 py-2"
                  >
                    <option value="critical">Critical</option>
                    <option value="high">High</option>
                    <option value="medium">Medium</option>
                    <option value="low">Low</option>
                    <option value="info">Info</option>
                  </select>
                </div>

                <div className="flex gap-2">
                  <Button
                    onClick={() => editingRule && handleSaveRule(editingRule)}
                  >
                    <Save className="w-4 h-4 mr-2" />
                    Save Rule
                  </Button>
                  <Button
                    variant="outline"
                    onClick={() => {
                      setEditingRule(null);
                      setIsCreatingRule(false);
                    }}
                  >
                    <X className="w-4 h-4 mr-2" />
                    Cancel
                  </Button>
                </div>
              </CardContent>
            </Card>
          )}

          <Card>
            <CardHeader>
              <CardTitle>Configured Alert Rules</CardTitle>
              <CardDescription>
                Manage alert thresholds and conditions
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-3">
                {alertRules.map(rule => (
                  <div
                    key={rule.id}
                    className="p-4 border rounded-lg hover:bg-muted/50"
                  >
                    <div className="flex items-start justify-between">
                      <div className="flex-1">
                        <div className="flex items-center gap-2 mb-2">
                          <span className="font-medium">{rule.name}</span>
                          <Badge variant={rule.enabled ? 'default' : 'secondary'}>
                            {rule.enabled ? 'Enabled' : 'Disabled'}
                          </Badge>
                          <Badge variant="outline">
                            {rule.severity}
                          </Badge>

                              {rule.notification_channels && rule.notification_channels.length > 0 && (
                                <span className="flex gap-1 flex-wrap">
                                  {rule.notification_channels.map((ch) => (
                                    <Badge key={ch} variant="outline">{ch}</Badge>
                                  ))}
                                </span>
                              )}

                        </div>
                        <p className="text-sm text-muted-foreground mb-2">
                          {rule.description}
                        </p>
                        <div className="text-xs font-mono text-muted-foreground">
                          {rule.metric} {rule.condition === 'gt' ? '>' : rule.condition === 'lt' ? '<' : '='} {rule.threshold}
                          {rule.duration_seconds > 0 && ` for ${rule.duration_seconds}s`}
                        </div>
                      </div>
                      <div className="flex gap-2">
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => handleToggleRule(rule.id)}
                        >
                          {rule.enabled ? 'Disable' : 'Enable'}
                        </Button>
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => setEditingRule(rule)}
                        >
                          <Edit className="w-3 h-3" />
                        </Button>
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => handleDeleteRule(rule.id)}
                        >
                          <Trash2 className="w-3 h-3" />
                        </Button>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Current Metrics */}
        <TabsContent value="metrics" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Current System Metrics</CardTitle>
              <CardDescription>
                Live system performance indicators
              </CardDescription>
            </CardHeader>
            <CardContent>
              {metrics ? (
                <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
                  <div className="p-4 bg-muted rounded-lg">
                    <div className="text-sm text-muted-foreground mb-1">Memory Usage</div>
                    <div className="text-2xl font-bold">{metrics.memory_usage_pct}%</div>
                  </div>
                  <div className="p-4 bg-muted rounded-lg">
                    <div className="text-sm text-muted-foreground mb-1">P95 Latency</div>
                    <div className="text-2xl font-bold">{metrics.latency_p95_ms}ms</div>
                  </div>
                  <div className="p-4 bg-muted rounded-lg">
                    <div className="text-sm text-muted-foreground mb-1">Tokens/Second</div>
                    <div className="text-2xl font-bold">{metrics.tokens_per_second}</div>
                  </div>
                  <div className="p-4 bg-muted rounded-lg">
                    <div className="text-sm text-muted-foreground mb-1">Adapter Count</div>
                    <div className="text-2xl font-bold">{metrics.adapter_count}</div>
                  </div>
                  <div className="p-4 bg-muted rounded-lg">
                    <div className="text-sm text-muted-foreground mb-1">Active Sessions</div>
                    <div className="text-2xl font-bold">{metrics.active_sessions}</div>
                  </div>
                </div>
              ) : (
                <div className="text-center py-12 text-muted-foreground">
                  <Activity className="w-12 h-12 mx-auto mb-3 opacity-20 animate-pulse" />
                  <p>Loading metrics...</p>
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
