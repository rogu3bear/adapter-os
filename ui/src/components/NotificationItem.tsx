//! Individual notification item component
//!
//! Displays notification with icon, timestamp, and read status.
//! Click handler for navigation. Expandable for more details.
//!
//! Citation: Event item pattern from ui/src/components/dashboard/ActivityFeedWidget.tsx L146-L190
//! - Icon based on notification type
//! - Timestamp with relative time (Citation: ui/src/hooks/useTimestamp.ts)
//! - Badge for severity/type

import React, { useState } from 'react';
import { Badge } from './ui/badge';
import { Button } from './ui/button';
import { Check, AlertTriangle, MessageSquare, Activity, Share, AtSign, Bell, ChevronDown, ChevronUp, ExternalLink } from 'lucide-react';
import { Notification } from '@/api/types';
import { useRelativeTime } from '@/hooks/useTimestamp';
import { useNavigate } from 'react-router-dom';
import { logger } from '@/utils/logger';
import { formatTimestamp } from '@/utils/format';

interface NotificationItemProps {
  notification: Notification;
  onMarkRead: (notificationId: string) => Promise<void>;
}

function getNotificationIcon(type: Notification['type']) {
  switch (type) {
    case 'alert':
      return AlertTriangle;
    case 'message':
      return MessageSquare;
    case 'activity':
      return Activity;
    case 'system':
      return Share;
    case 'mention':
      return AtSign;
    default:
      return Bell;
  }
}

function getNotificationColor(type: Notification['type']): string {
  switch (type) {
    case 'alert':
      return 'text-red-600';
    case 'message':
      return 'text-blue-600';
    case 'activity':
      return 'text-green-600';
    case 'system':
      return 'text-purple-600';
    case 'mention':
      return 'text-orange-600';
    default:
      return 'text-gray-600';
  }
}

function getNotificationBadgeVariant(type: Notification['type']): 'default' | 'secondary' | 'destructive' | 'outline' {
  switch (type) {
    case 'alert':
      return 'destructive';
    case 'message':
      return 'default';
    case 'activity':
      return 'secondary';
    case 'system':
      return 'outline';
    case 'mention':
      return 'outline';
    default:
      return 'outline';
  }
}

export function NotificationItem({ notification, onMarkRead }: NotificationItemProps) {
  const navigate = useNavigate();
  const relativeTime = useRelativeTime(notification.created_at);
  const Icon = getNotificationIcon(notification.type);
  const isRead = !!notification.read_at;
  const [isExpanded, setIsExpanded] = useState(false);

  const getNavigationPath = () => {
    switch (notification.type) {
      case 'message':
        if (notification.workspace_id) {
          return `/chat?workspace=${notification.workspace_id}`;
        }
        return '/chat';
      case 'activity':
        if (notification.workspace_id) {
          return `/dashboard?workspace=${notification.workspace_id}`;
        }
        return '/telemetry';
      case 'system':
        if (notification.target_type && notification.target_id) {
          switch (notification.target_type) {
            case 'adapter':
              return `/adapters/${notification.target_id}`;
            case 'model':
              return `/base-models`;
            case 'workspace':
              return `/dashboard?workspace=${notification.target_id}`;
            default:
              return '/dashboard';
          }
        }
        break;
      case 'mention':
        if (notification.workspace_id) {
          return `/chat?workspace=${notification.workspace_id}`;
        }
        return '/chat';
      case 'alert':
      default:
        return '/dashboard';
    }
    return null;
  };

  const handleToggleExpand = async (e: React.MouseEvent) => {
    e.stopPropagation();
    setIsExpanded(!isExpanded);

    // Mark as read when expanding if not already read
    if (!isRead && !isExpanded) {
      try {
        await onMarkRead(notification.id);
      } catch (err) {
        logger.error('Failed to mark notification as read on expand', {
          component: 'NotificationItem',
          operation: 'mark_read_on_expand',
          notificationId: notification.id,
        }, err instanceof Error ? err : new Error(String(err)));
      }
    }

    logger.info('Notification toggled', {
      component: 'NotificationItem',
      operation: isExpanded ? 'collapse' : 'expand',
      notificationId: notification.id,
    });
  };

  const handleNavigate = (e: React.MouseEvent) => {
    e.stopPropagation();
    const path = getNavigationPath();
    if (path) {
      navigate(path);
      logger.info('Notification navigated', {
        component: 'NotificationItem',
        operation: 'notification_navigate',
        notificationId: notification.id,
        type: notification.type,
        targetType: notification.target_type,
        targetId: notification.target_id,
      });
    }
  };

  const handleMarkReadClick = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!isRead) {
      await onMarkRead(notification.id);
    }
  };


  return (
    <div
      className={`rounded-lg border cursor-pointer transition-all backdrop-blur-sm ${
        isRead ? 'bg-muted/10 border-border/30' : 'bg-background/80 border-border/50'
      } ${isExpanded ? 'shadow-md' : 'hover:bg-muted/30'}`}
      onClick={handleToggleExpand}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          handleToggleExpand(e as unknown as React.MouseEvent);
        }
      }}
      aria-label={`Notification: ${notification.title}. ${isRead ? 'Read' : 'Unread'}. Click to ${isExpanded ? 'collapse' : 'expand'}.`}
      aria-expanded={isExpanded}
    >
      {/* Compact header */}
      <div className="flex items-start gap-3 p-3">
        <Icon
          className={`h-5 w-5 mt-0.5 flex-shrink-0 ${getNotificationColor(notification.type)}`}
          aria-hidden="true"
        />

        <div className="flex-1 min-w-0">
          <div className="flex items-start justify-between gap-2">
            <div className="flex-1 min-w-0">
              <h4 className={`text-sm font-medium ${isExpanded ? '' : 'truncate'} ${isRead ? 'text-muted-foreground' : 'text-foreground'}`}>
                {notification.title}
              </h4>
              {!isExpanded && (
                <p className="text-sm mt-1 text-muted-foreground truncate">
                  {notification.content}
                </p>
              )}
            </div>

            <div className="flex items-center gap-2 flex-shrink-0">
              <Badge variant={getNotificationBadgeVariant(notification.type)} className="text-xs">
                {notification.type.replace('_', ' ')}
              </Badge>

              {!isRead && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-6 w-6 p-0"
                  onClick={handleMarkReadClick}
                  aria-label="Mark as read"
                  title="Mark as read"
                >
                  <Check className="h-3 w-3" />
                </Button>
              )}

              <Button
                variant="ghost"
                size="sm"
                className="h-6 w-6 p-0"
                onClick={handleToggleExpand}
                aria-label={isExpanded ? 'Collapse' : 'Expand'}
                title={isExpanded ? 'Collapse' : 'Expand'}
              >
                {isExpanded ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />}
              </Button>
            </div>
          </div>

          {!isExpanded && (
            <div className="flex items-center justify-between mt-2">
              <span className="text-xs text-muted-foreground">
                {relativeTime}
              </span>
            </div>
          )}
        </div>
      </div>

      {/* Expanded details */}
      {isExpanded && (
        <div className="px-3 pb-3 pt-0 border-t border-border/30 mt-1">
          <div className="pl-8 space-y-3">
            {/* Full content */}
            <div className="pt-3">
              <p className="text-sm text-muted-foreground whitespace-pre-wrap">
                {notification.content}
              </p>
            </div>

            {/* Metadata grid */}
            <div className="grid grid-cols-2 gap-2 text-xs">
              <div>
                <span className="text-muted-foreground">Created:</span>{' '}
                <span className="text-foreground">{formatTimestamp(notification.created_at, 'long')}</span>
              </div>
              {notification.read_at && (
                <div>
                  <span className="text-muted-foreground">Read:</span>{' '}
                  <span className="text-foreground">{formatTimestamp(notification.read_at, 'long')}</span>
                </div>
              )}
              {notification.workspace_id && (
                <div>
                  <span className="text-muted-foreground">Workspace:</span>{' '}
                  <span className="text-foreground font-mono">{notification.workspace_id.slice(0, 8)}...</span>
                </div>
              )}
              {notification.target_type && (
                <div>
                  <span className="text-muted-foreground">Target:</span>{' '}
                  <span className="text-foreground capitalize">{notification.target_type}</span>
                </div>
              )}
              {notification.target_id && (
                <div>
                  <span className="text-muted-foreground">Target ID:</span>{' '}
                  <span className="text-foreground font-mono">{notification.target_id.slice(0, 8)}...</span>
                </div>
              )}
            </div>

            {/* Action button */}
            {getNavigationPath() && (
              <div className="pt-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={handleNavigate}
                  className="text-xs"
                >
                  <ExternalLink className="h-3 w-3 mr-1" />
                  View Details
                </Button>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
