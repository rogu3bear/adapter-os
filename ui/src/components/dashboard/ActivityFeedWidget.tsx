//! Activity Feed Widget
//!
//! Displays recent telemetry events with filters and tenant scoping.
//!
//! Citations
//! - Hook: ui/src/hooks/useActivityFeed.ts
//! - API: getTelemetryEvents ui/src/api/client.ts
//! - Time: useRelativeTime utilities ui/src/hooks/useTimestamp.ts

import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import {
  Activity,
  AlertTriangle,
  Shield,
  Hammer,
  Box,
  Radio,
  ShieldAlert,
  Users,
} from 'lucide-react';
import { useActivityFeed } from '@/hooks/useActivityFeed';
import { useTenant } from '@/layout/LayoutProvider';
import { useRelativeTime } from '@/hooks/useTimestamp';
import { useNavigate } from 'react-router-dom';

type EventType = 'all' | 'recovery' | 'policy' | 'build' | 'adapter' | 'telemetry' | 'security' | 'error' | 'collaboration';
type Severity = 'all' | 'info' | 'warning' | 'error' | 'critical';

function typeIcon(type: Exclude<EventType, 'all'>) {
  switch (type) {
    case 'recovery':
      return Activity;
    case 'policy':
      return Shield;
    case 'build':
      return Hammer;
    case 'adapter':
      return Box;
    case 'telemetry':
      return Radio;
    case 'security':
      return ShieldAlert;
    case 'error':
      return AlertTriangle;
    case 'collaboration':
      return Users; // Using Users icon for collaboration
    default:
      return Activity;
  }
}

function severityBadge(severity: Exclude<Severity, 'all'>) {
  switch (severity) {
    case 'critical':
      return 'bg-red-100 text-red-800 border-red-200';
    case 'error':
      return 'bg-orange-100 text-orange-800 border-orange-200';
    case 'warning':
      return 'bg-amber-100 text-amber-800 border-amber-200';
    case 'info':
    default:
      return 'bg-blue-100 text-blue-800 border-blue-200';
  }
}

export function ActivityFeedWidget() {
  const { selectedTenant } = useTenant();
  const navigate = useNavigate();
  const [typeFilter, setTypeFilter] = React.useState<EventType>('all');
  const [severityFilter, setSeverityFilter] = React.useState<Severity>('all');

  const { events, loading, error, refresh } = useActivityFeed({
    tenantId: selectedTenant,
    enabled: true,
    maxEvents: 50,
  });

  const filtered = events.filter((e) => {
    const typeOk = typeFilter === 'all' || e.type === typeFilter;
    const sevOk = severityFilter === 'all' || e.severity === severityFilter;
    return typeOk && sevOk;
  });

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-3">
          <CardTitle className="flex items-center gap-2">
            <Activity className="h-5 w-5" />
            Activity Feed
            <Badge variant="outline" className="ml-2">
              {filtered.length}
            </Badge>
          </CardTitle>
          <div className="flex items-center gap-2">
            <Select value={typeFilter} onValueChange={(v) => setTypeFilter(v as EventType)}>
              <SelectTrigger className="w-[140px]" aria-label="Type filter">
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
              <SelectTrigger className="w-[140px]" aria-label="Severity filter">
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
            <Button variant="outline" size="sm" onClick={() => void refresh()} aria-label="Refresh events">
              Refresh
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent>
        {loading && (
          <div className="space-y-2">
            <div className="h-4 bg-muted animate-pulse rounded" />
            <div className="h-4 bg-muted animate-pulse rounded w-5/6" />
            <div className="h-4 bg-muted animate-pulse rounded w-4/6" />
          </div>
        )}
        {!loading && error && (
          <div className="text-sm text-destructive">Failed to load activity: {error}</div>
        )}
        {!loading && !error && filtered.length === 0 && (
          <div className="text-center py-6 text-sm text-muted-foreground">No recent activity</div>
        )}
        {!loading && !error && filtered.length > 0 && (
          <div className="space-y-2">
            {filtered.slice(0, 15).map((event) => {
              const Icon = typeIcon(event.type);
              const rel = useRelativeTime(event.timestamp);
              return (
                <div
                  key={event.id}
                  className="flex items-start gap-3 p-2 rounded border bg-muted/50 cursor-pointer hover:bg-muted"
                  onClick={() => {
                    switch (event.type) {
                      case 'policy':
                        navigate('/policies');
                        break;
                      case 'build':
                        navigate('/plans');
                        break;
                      case 'adapter':
                        navigate('/adapters');
                        break;
                      case 'security':
                      case 'error':
                        navigate('/monitoring');
                        break;
                      case 'collaboration':
                        // Navigate to messages or workspace based on event metadata
                        if (event.workspaceId) {
                          navigate(`/workspaces/${event.workspaceId}`);
                        } else {
                          navigate('/messages');
                        }
                        break;
                      case 'telemetry':
                      case 'recovery':
                      default:
                        navigate('/telemetry');
                    }
                  }}
                  role="button"
                >
                  <Icon className="h-4 w-4 mt-0.5 text-muted-foreground flex-shrink-0" />
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium truncate">{event.message}</span>
                      <Badge variant="outline" className={`text-[10px] ${severityBadge(event.severity)}`}>
                        {event.severity}
                      </Badge>
                    </div>
                    <div className="text-xs text-muted-foreground mt-0.5">
                      <span>{rel}</span>
                      {event.component ? <span className="ml-2">• {event.component}</span> : null}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

export default ActivityFeedWidget;
