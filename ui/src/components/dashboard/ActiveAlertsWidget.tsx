import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import { AlertTriangle, Bell, CheckCircle, Clock } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

interface Alert {
  id: string;
  severity: 'critical' | 'high' | 'medium' | 'low';
  title: string;
  timestamp: string;
  acknowledged: boolean;
}

export function ActiveAlertsWidget() {
  const navigate = useNavigate();

  // Mock alert data - in production, fetch from API
  const alerts: Alert[] = [
    {
      id: '1',
      severity: 'high',
      title: 'Memory usage at 87%',
      timestamp: '5m ago',
      acknowledged: false
    },
    {
      id: '2',
      severity: 'medium',
      title: 'P95 latency elevated',
      timestamp: '12m ago',
      acknowledged: false
    },
    {
      id: '3',
      severity: 'low',
      title: 'Adapter eviction occurred',
      timestamp: '1h ago',
      acknowledged: true
    }
  ];

  const activeAlerts = alerts.filter(a => !a.acknowledged);
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
        {alerts.length === 0 ? (
          <div className="text-center py-8">
            <CheckCircle className="h-12 w-12 text-green-600 mx-auto mb-2 opacity-20" aria-hidden="true" />
            <p className="text-sm text-muted-foreground">No active alerts</p>
          </div>
        ) : (
          <>
            {criticalCount > 0 && (
              <div className="p-3 bg-red-50 border border-red-200 rounded-lg">
                <div className="flex items-center gap-2 text-red-900">
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
                return (
                  <div
                    key={alert.id}
                    className={`p-3 rounded-lg border ${
                      alert.acknowledged 
                        ? 'opacity-60 bg-muted border-muted' 
                        : getSeverityColor(alert.severity)
                    }`}
                  >
                    <div className="flex items-start gap-2" role="listitem">
                      <Icon className="h-4 w-4 mt-0.5 flex-shrink-0" aria-hidden="true" />
                      <div className="flex-1 min-w-0">
                        <p className="text-sm font-medium">{alert.title}</p>
                        <div className="flex items-center gap-2 mt-1">
                          <Clock className="h-3 w-3 text-muted-foreground" aria-hidden="true" />
                          <span className="text-xs text-muted-foreground">{alert.timestamp}</span>
                          {alert.acknowledged && (
                            <Badge variant="outline" className="text-xs">
                              Acknowledged
                            </Badge>
                          )}
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
