import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from './ui/card';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Badge } from './ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Slider } from './ui/slider';
import { HelpTooltip } from './ui/help-tooltip';
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

import { useTenant } from '@/layout/LayoutProvider';

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

interface Alert {
  id: string;
  rule_id: string;
  rule_name: string;
  severity: 'critical' | 'high' | 'medium' | 'low' | 'info';
  message: string;
  current_value: number;
  threshold: number;
  triggered_at: string;
  resolved_at?: string;
  acknowledged: boolean;
}

const DEFAULT_ALERT_RULES: AlertRule[] = [
  {
    id: 'rule-1',
    name: 'High Memory Usage',
    enabled: true,
    metric: 'memory_usage_pct',
    condition: 'gt',
    threshold: 85,
    duration_seconds: 300,
    severity: 'high',
    notification_channels: ['dashboard', 'log'],
    description: 'Alert when memory usage exceeds 85% for 5 minutes'
  },
  {
    id: 'rule-2',
    name: 'High Latency',
    enabled: true,
    metric: 'latency_p95_ms',
    condition: 'gt',
    threshold: 24,
    duration_seconds: 60,
    severity: 'medium',
    notification_channels: ['dashboard'],
    description: 'Alert when P95 latency exceeds 24ms for 1 minute'
  },
  {
    id: 'rule-3',
    name: 'Low Tokens/Second',
    enabled: true,
    metric: 'tokens_per_second',
    condition: 'lt',
    threshold: 10,
    duration_seconds: 120,
    severity: 'medium',
    notification_channels: ['dashboard'],
    description: 'Alert when token throughput drops below 10/s'
  },
  {
    id: 'rule-4',
    name: 'Adapter Capacity',
    enabled: true,
    metric: 'adapter_count',
    condition: 'gt',
    threshold: 256,
    duration_seconds: 0,
    severity: 'high',
    notification_channels: ['dashboard', 'log'],
    description: 'Alert when adapter count exceeds capacity'
  }
];

export function AlertsPage({ selectedTenant: tenantProp }: AlertsPageProps) {
  const { selectedTenant } = useTenant();
  const effectiveTenant = tenantProp ?? selectedTenant;
  const [alertRules, setAlertRules] = useState<AlertRule[]>(DEFAULT_ALERT_RULES);
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [metrics, setMetrics] = useState<SystemMetrics | null>(null);
  const [editingRule, setEditingRule] = useState<AlertRule | null>(null);
  const [isCreatingRule, setIsCreatingRule] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const CHANNEL_OPTIONS = ['dashboard', 'log', 'slack', 'pagerduty'] as const;

  useEffect(() => {
    loadAlerts();
    loadMetrics();

    // Poll metrics every 2 seconds for instant updates
    const interval = setInterval(loadMetrics, 2000);
    return () => clearInterval(interval);
  }, [selectedTenant]);

  useEffect(() => {
    // Evaluate alert rules when metrics update
    if (metrics) {
      evaluateAlertRules(metrics);
    }
  }, [metrics, alertRules]);

  const loadAlerts = () => {
    // Mock alerts (in production, load from backend)
    const mockAlerts: Alert[] = [
      {
        id: 'alert-1',
        rule_id: 'rule-1',
        rule_name: 'High Memory Usage',
        severity: 'high',
        message: 'Memory usage at 87% for 5 minutes',
        current_value: 87,
        threshold: 85,
        triggered_at: new Date(Date.now() - 300000).toISOString(),
        acknowledged: false
      }
    ];
    setAlerts(mockAlerts);
  };

  const loadMetrics = async () => {
    try {
      const metricsData = await apiClient.getSystemMetrics();
      setMetrics(metricsData);
    } catch (error) {
      logger.error('Failed to load system metrics for alerts', {
        component: 'AlertsPage',
        operation: 'loadMetrics',
        tenantId: effectiveTenant,
      }, toError(error));
    }
  };

  const evaluateAlertRules = (currentMetrics: SystemMetrics) => {
    alertRules.forEach(rule => {
      if (!rule.enabled) return;

      const metricValue = (currentMetrics as any)[rule.metric];
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
        // Check if alert already exists
        const existingAlert = alerts.find(
          a => a.rule_id === rule.id && !a.resolved_at
        );
        if (!existingAlert) {
          const newAlert: Alert = {
            id: `alert-${Date.now()}`,
            rule_id: rule.id,
            rule_name: rule.name,
            severity: rule.severity,
            message: `${rule.name}: ${metricValue} ${rule.condition === 'gt' ? '>' : rule.condition === 'lt' ? '<' : '='} ${rule.threshold}`,
            current_value: metricValue,
            threshold: rule.threshold,
            triggered_at: new Date().toISOString(),
            acknowledged: false
          };
          setAlerts(prev => [newAlert, ...prev]);
        }
      }
    });
  };

  const handleToggleRule = (ruleId: string) => {
    setAlertRules(prev =>
      prev.map(rule =>
        rule.id === ruleId ? { ...rule, enabled: !rule.enabled } : rule
      )
    );
    toast.success('Alert rule updated');
  };

  const handleDeleteRule = (ruleId: string) => {
    setAlertRules(prev => prev.filter(rule => rule.id !== ruleId));
    toast.success('Alert rule deleted');
  };

  const handleSaveRule = (rule: AlertRule) => {
    if (isCreatingRule) {
      setAlertRules(prev => [...prev, { ...rule, id: `rule-${Date.now()}` }]);
      toast.success('Alert rule created');
    } else {
      setAlertRules(prev =>
        prev.map(r => (r.id === rule.id ? rule : r))
      );
      toast.success('Alert rule updated');
    }
    setEditingRule(null);
    setIsCreatingRule(false);
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
      case 'high':
        return 'text-orange-600 bg-orange-50 border-orange-200';
      case 'medium':
        return 'text-amber-600 bg-amber-50 border-amber-200';
      case 'low':
        return 'text-blue-600 bg-blue-50 border-blue-200';
      case 'info':
        return 'text-gray-600 bg-gray-50 border-gray-200';
    }
  };

  const activeAlerts = alerts.filter(a => !a.resolved_at);
  const criticalAlerts = activeAlerts.filter(a => a.severity === 'critical').length;
  const unacknowledgedAlerts = activeAlerts.filter(a => !a.acknowledged).length;

  return (
    <div className="space-y-6">
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
                        ${alert.acknowledged ? 'opacity-60' : ''}
                      `}
                    >
                      <div className="flex items-start justify-between">
                        <div className="flex-1">
                          <div className="flex items-center gap-2 mb-2">
                            <AlertTriangle className="w-5 h-5" />
                            <span className="font-semibold">{alert.rule_name}</span>
                            <Badge variant="outline">
                              {alert.severity.toUpperCase()}
                            </Badge>
                            {alert.acknowledged && (
                              <Badge variant="outline" className="bg-blue-50">
                                Acknowledged
                              </Badge>
                            )}
                          </div>
                          <p className="text-sm mb-2">{alert.message}</p>
                          <div className="flex items-center gap-4 text-xs">
                            <span>
                              Current: {alert.current_value}
                            </span>
                            <span>
                              Threshold: {alert.threshold}
                            </span>
                            <span>
                              Triggered: {new Date(alert.triggered_at).toLocaleString()}
                            </span>
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
