/**
 * AlertHero - Conditional warning banner
 *
 * Displays only when there are system issues requiring attention:
 * - Memory pressure HIGH or CRITICAL
 * - No model loaded
 * - Unhealthy services
 *
 * Dismissible with 1-hour localStorage cooldown.
 */

import React, { useState, useEffect, useMemo } from 'react';
import { AlertTriangle, XCircle, X, Database, MemoryStick, Activity, ExternalLink } from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import { useNavigate } from 'react-router-dom';
import type { SystemOverview } from '@/api/owner-types';
import type { BaseModelStatus } from '@/api/api-types';

interface SystemStateData {
  memory?: {
    pressure_level?: 'low' | 'medium' | 'high' | 'critical';
    used_mb?: number;
    total_mb?: number;
  };
}

interface AlertHeroProps {
  systemOverview?: SystemOverview;
  baseModelStatus?: BaseModelStatus;
  systemState?: SystemStateData;
  className?: string;
}

interface Alert {
  id: string;
  type: 'error' | 'warning';
  icon: React.ElementType;
  title: string;
  description: string;
  action?: {
    label: string;
    path: string;
  };
}

const DISMISS_STORAGE_KEY = 'aos-alert-hero-dismissed';
const DISMISS_DURATION_MS = 60 * 60 * 1000; // 1 hour

export function AlertHero({
  systemOverview,
  baseModelStatus,
  systemState,
  className,
}: AlertHeroProps) {
  const navigate = useNavigate();
  const [dismissed, setDismissed] = useState(false);

  // Check localStorage for dismissal on mount
  useEffect(() => {
    const dismissedAt = localStorage.getItem(DISMISS_STORAGE_KEY);
    if (dismissedAt) {
      const elapsed = Date.now() - parseInt(dismissedAt, 10);
      if (elapsed < DISMISS_DURATION_MS) {
        setDismissed(true);
      } else {
        localStorage.removeItem(DISMISS_STORAGE_KEY);
      }
    }
  }, []);

  // Generate alerts based on system state
  const alerts = useMemo<Alert[]>(() => {
    const result: Alert[] = [];

    // Memory pressure check
    const memoryPressure = systemState?.memory?.pressure_level;
    if (memoryPressure === 'high' || memoryPressure === 'critical') {
      const usedMb = systemState?.memory?.used_mb || 0;
      const totalMb = systemState?.memory?.total_mb || 1;
      const usagePercent = Math.round((usedMb / totalMb) * 100);

      result.push({
        id: 'memory-pressure',
        type: memoryPressure === 'critical' ? 'error' : 'warning',
        icon: MemoryStick,
        title: `Memory pressure is ${memoryPressure.toUpperCase()}`,
        description: `${usagePercent}% memory used (${Math.round(usedMb / 1024)} GB of ${Math.round(totalMb / 1024)} GB)`,
        action: {
          label: 'View Memory',
          path: '/system/memory',
        },
      });
    }

    // No model loaded check
    if (!baseModelStatus?.model_id && !baseModelStatus?.model_name) {
      result.push({
        id: 'no-model',
        type: 'warning',
        icon: Database,
        title: 'No base model loaded',
        description: 'Import and load a base model to enable inference',
        action: {
          label: 'Import Model',
          path: '/base-models',
        },
      });
    }

    // Unhealthy services check
    const unhealthyServices = systemOverview?.services?.filter(
      s => s.status !== 'healthy'
    ) || [];
    if (unhealthyServices.length > 0) {
      result.push({
        id: 'unhealthy-services',
        type: unhealthyServices.some(s => s.status === 'unhealthy') ? 'error' : 'warning',
        icon: Activity,
        title: `${unhealthyServices.length} service${unhealthyServices.length > 1 ? 's' : ''} unhealthy`,
        description: unhealthyServices.map(s => s.name).join(', '),
        action: {
          label: 'View Services',
          path: '/system',
        },
      });
    }

    return result;
  }, [systemOverview, baseModelStatus, systemState]);

  // Don't render if dismissed or no alerts
  if (dismissed || alerts.length === 0) {
    return null;
  }

  const handleDismiss = () => {
    localStorage.setItem(DISMISS_STORAGE_KEY, Date.now().toString());
    setDismissed(true);
  };

  const handleAction = (path: string) => {
    navigate(path);
  };

  // Show the most severe alert first
  const primaryAlert = alerts.find(a => a.type === 'error') || alerts[0];
  const hasMultiple = alerts.length > 1;
  const isError = primaryAlert.type === 'error';

  return (
    <Card
      className={cn(
        'border-2 shadow-sm',
        isError
          ? 'border-red-300 bg-gradient-to-br from-red-50 to-orange-50'
          : 'border-amber-300 bg-gradient-to-br from-amber-50 to-yellow-50',
        className
      )}
    >
      <CardContent className="p-4">
        <div className="flex items-start justify-between gap-4">
          <div className="flex items-start gap-3 flex-1">
            <div
              className={cn(
                'p-2 rounded-lg flex-shrink-0',
                isError ? 'bg-red-100' : 'bg-amber-100'
              )}
            >
              {isError ? (
                <XCircle className="h-5 w-5 text-red-600" />
              ) : (
                <AlertTriangle className="h-5 w-5 text-amber-600" />
              )}
            </div>

            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2 flex-wrap">
                <h3
                  className={cn(
                    'text-sm font-semibold',
                    isError ? 'text-red-900' : 'text-amber-900'
                  )}
                >
                  {primaryAlert.title}
                </h3>
                {hasMultiple && (
                  <Badge variant="outline" className="text-xs">
                    +{alerts.length - 1} more
                  </Badge>
                )}
              </div>
              <p
                className={cn(
                  'text-sm mt-0.5',
                  isError ? 'text-red-700' : 'text-amber-700'
                )}
              >
                {primaryAlert.description}
              </p>

              {/* Show all alert icons if multiple */}
              {hasMultiple && (
                <div className="flex items-center gap-2 mt-2">
                  {alerts.map((alert) => {
                    const Icon = alert.icon;
                    return (
                      <div
                        key={alert.id}
                        className={cn(
                          'p-1.5 rounded',
                          alert.type === 'error' ? 'bg-red-100' : 'bg-amber-100'
                        )}
                        title={alert.title}
                      >
                        <Icon
                          className={cn(
                            'h-4 w-4',
                            alert.type === 'error' ? 'text-red-600' : 'text-amber-600'
                          )}
                        />
                      </div>
                    );
                  })}
                </div>
              )}

              {primaryAlert.action && (
                <Button
                  size="sm"
                  variant={isError ? 'destructive' : 'default'}
                  className="mt-3"
                  onClick={() => handleAction(primaryAlert.action!.path)}
                >
                  {primaryAlert.action.label}
                  <ExternalLink className="h-3 w-3 ml-1.5" />
                </Button>
              )}
            </div>
          </div>

          <Button
            variant="ghost"
            size="sm"
            onClick={handleDismiss}
            className="flex-shrink-0 h-8 w-8 p-0 hover:bg-slate-200/50"
            aria-label="Dismiss alert"
          >
            <X className="h-4 w-4 text-slate-500" />
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
