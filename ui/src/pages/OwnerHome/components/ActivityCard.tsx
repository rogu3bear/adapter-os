import React from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { formatDistanceToNow } from 'date-fns';
import {
  Activity,
  Info,
  AlertTriangle,
  XCircle,
  CheckCircle,
  ExternalLink,
  Clock,
} from 'lucide-react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { ScrollArea } from '@/components/ui/scroll-area';
import { apiClient } from '@/api/client';
import type { RecentActivityEvent } from '@/api/auth-types';

interface ActivityCardProps {
  refreshKey: number;
}

// Map activity event type to display type with semantic meaning
type ActivityType = 'info' | 'warning' | 'error' | 'success';

interface ActivityItem {
  id: string;
  type: ActivityType;
  message: string;
  timestamp: string;
  actor?: string;
  icon: React.ElementType;
  badgeVariant: 'default' | 'secondary' | 'destructive' | 'outline';
}

const getActivityType = (event: RecentActivityEvent): ActivityType => {
  const eventType = event.event_type || event.type || '';

  // Error patterns
  if (eventType.includes('error') || eventType.includes('failed') || eventType.includes('deleted')) {
    return 'error';
  }

  // Warning patterns
  if (eventType.includes('warning') || eventType.includes('quarantine') || eventType.includes('removed')) {
    return 'warning';
  }

  // Success patterns
  if (
    eventType.includes('created') ||
    eventType.includes('completed') ||
    eventType.includes('success') ||
    eventType.includes('shared') ||
    eventType.includes('joined')
  ) {
    return 'success';
  }

  // Default to info
  return 'info';
};

const getActivityIcon = (type: ActivityType): React.ElementType => {
  switch (type) {
    case 'error':
      return XCircle;
    case 'warning':
      return AlertTriangle;
    case 'success':
      return CheckCircle;
    case 'info':
    default:
      return Info;
  }
};

const getBadgeVariant = (type: ActivityType): 'default' | 'secondary' | 'destructive' | 'outline' => {
  switch (type) {
    case 'error':
      return 'destructive';
    case 'warning':
      return 'outline';
    case 'success':
      return 'default';
    case 'info':
    default:
      return 'secondary';
  }
};

const formatActivityMessage = (event: RecentActivityEvent): string => {
  // Use explicit message if available
  if (event.message) {
    return event.message;
  }

  // Fallback: construct message from action and target
  const actor = event.actor || event.user_name || 'System';
  const action = event.action || event.event_type || event.type || 'performed action';
  const target = event.target || event.resource_name || '';

  if (target) {
    return `${actor} ${action} ${target}`;
  }

  return `${actor} ${action}`;
};

const ActivityCard: React.FC<ActivityCardProps> = ({ refreshKey }) => {
  const navigate = useNavigate();

  const { data: events, isLoading, isError } = useQuery<RecentActivityEvent[]>({
    queryKey: ['recent-activity', refreshKey],
    queryFn: async () => {
      try {
        // Fetch recent activity events (last 10)
        const response = await apiClient.getRecentActivityEvents({ limit: 10 });
        return response || [];
      } catch (_error) {
        // Return empty array if API is unavailable (graceful degradation)
        return [];
      }
    },
    refetchInterval: 30000, // Refresh every 30 seconds
  });

  const activityItems: ActivityItem[] = React.useMemo(() => {
    if (!events || events.length === 0) return [];

    return events.slice(0, 10).map((event) => {
      const type = getActivityType(event);
      return {
        id: event.id,
        type,
        message: formatActivityMessage(event),
        timestamp: event.timestamp || event.created_at || new Date().toISOString(),
        actor: event.actor || event.user_name,
        icon: getActivityIcon(type),
        badgeVariant: getBadgeVariant(type),
      };
    });
  }, [events]);

  const handleViewAll = () => {
    navigate('/telemetry');
  };

  if (isLoading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Recent Activity</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            {[...Array(5)].map((_, i) => (
              <div key={i} className="flex items-start space-x-3">
                <Skeleton className="h-8 w-8 rounded-full flex-shrink-0" />
                <div className="flex-1 space-y-2">
                  <Skeleton className="h-4 w-full" />
                  <Skeleton className="h-3 w-24" />
                </div>
              </div>
            ))}
            <Skeleton className="h-10 w-full" />
          </div>
        </CardContent>
      </Card>
    );
  }

  if (isError) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Recent Activity</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex flex-col items-center justify-center py-8 text-center">
            <Activity className="h-12 w-12 text-slate-400 mb-3" />
            <p className="text-sm text-slate-600 mb-4">
              Failed to load activity feed
            </p>
            <Button
              variant="outline"
              size="sm"
              onClick={() => window.location.reload()}
            >
              Retry
            </Button>
          </div>
        </CardContent>
      </Card>
    );
  }

  // Empty state
  if (!activityItems || activityItems.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center justify-between">
            <span>Recent Activity</span>
            <Activity className="h-5 w-5 text-slate-500" />
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex flex-col items-center justify-center py-8 text-center">
            <Activity className="h-12 w-12 text-slate-300 mb-3" />
            <p className="text-sm text-slate-600 mb-1">No recent activity</p>
            <p className="text-xs text-slate-500">
              Activity events will appear here as they occur
            </p>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between">
          <span>Recent Activity</span>
          <Activity className="h-5 w-5 text-slate-500" />
        </CardTitle>
      </CardHeader>
      <CardContent>
        <ScrollArea className="h-[320px] pr-4">
          <div className="space-y-4">
            {activityItems.map((item) => {
              const Icon = item.icon;
              return (
                <div
                  key={item.id}
                  className="flex items-start space-x-3 pb-3 border-b border-slate-100 last:border-0"
                >
                  <div
                    className={`
                      flex-shrink-0 p-2 rounded-full
                      ${item.type === 'error' ? 'bg-red-100' : ''}
                      ${item.type === 'warning' ? 'bg-amber-100' : ''}
                      ${item.type === 'success' ? 'bg-green-100' : ''}
                      ${item.type === 'info' ? 'bg-blue-100' : ''}
                    `}
                  >
                    <Icon
                      className={`
                        h-4 w-4
                        ${item.type === 'error' ? 'text-red-600' : ''}
                        ${item.type === 'warning' ? 'text-amber-600' : ''}
                        ${item.type === 'success' ? 'text-green-600' : ''}
                        ${item.type === 'info' ? 'text-blue-600' : ''}
                      `}
                    />
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-start justify-between mb-1">
                      <p className="text-sm text-slate-900 leading-snug">
                        {item.message}
                      </p>
                      <Badge
                        variant={item.badgeVariant}
                        className="ml-2 flex-shrink-0 text-xs"
                      >
                        {item.type}
                      </Badge>
                    </div>
                    <div className="flex items-center space-x-1 text-xs text-slate-500">
                      <Clock className="h-3 w-3" />
                      <span>
                        {formatDistanceToNow(new Date(item.timestamp), {
                          addSuffix: true,
                        })}
                      </span>
                      {item.actor && (
                        <>
                          <span className="mx-1">•</span>
                          <span className="truncate">{item.actor}</span>
                        </>
                      )}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        </ScrollArea>

        <Button
          variant="outline"
          className="w-full mt-4"
          onClick={handleViewAll}
        >
          View All Activity
          <ExternalLink className="ml-2 h-4 w-4" />
        </Button>
      </CardContent>
    </Card>
  );
};

export default ActivityCard;
