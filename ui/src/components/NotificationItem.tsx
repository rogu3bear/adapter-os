//! Individual notification item component
//!
//! Displays notification with icon, timestamp, and read status.
//! Click handler for navigation.
//!
//! Citation: Event item pattern from ui/src/components/dashboard/ActivityFeedWidget.tsx L146-L190
//! - Icon based on notification type
//! - Timestamp with relative time (Citation: ui/src/hooks/useTimestamp.ts)
//! - Badge for severity/type

import React from 'react';
import { Badge } from './ui/badge';
import { Button } from './ui/button';
import { Check, AlertTriangle, MessageSquare, Activity, Share, AtSign, Bell } from 'lucide-react';
import { Notification } from '../api/types';
import { useRelativeTime } from '../hooks/useTimestamp';
import { useNavigate } from 'react-router-dom';
import { logger } from '../utils/logger';

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

  const handleClick = async () => {
    // Mark as read if not already read
    if (!isRead) {
      try {
        await onMarkRead(notification.id);
      } catch (err) {
        logger.error('Failed to mark notification as read on click', {
          component: 'NotificationItem',
          operation: 'mark_read_on_click',
          notificationId: notification.id,
        }, err instanceof Error ? err : new Error(String(err)));
      }
    }

    // Navigate based on notification type and target
    switch (notification.type) {
      case 'message':
        if (notification.workspace_id) {
          navigate(`/messages?workspace=${notification.workspace_id}`);
        }
        break;
      case 'activity':
        if (notification.workspace_id) {
          navigate(`/workspaces/${notification.workspace_id}`);
        } else {
          navigate('/activity');
        }
        break;
      case 'system':
        if (notification.target_type && notification.target_id) {
          switch (notification.target_type) {
            case 'adapter':
              navigate(`/adapters/${notification.target_id}`);
              break;
            case 'model':
              navigate(`/models/${notification.target_id}`);
              break;
            case 'workspace':
              navigate(`/workspaces/${notification.target_id}`);
              break;
            default:
              navigate('/dashboard');
          }
        }
        break;
      case 'mention':
        if (notification.workspace_id) {
          navigate(`/messages?workspace=${notification.workspace_id}`);
        }
        break;
      case 'alert':
      default:
        navigate('/dashboard');
    }

    logger.info('Notification clicked', {
      component: 'NotificationItem',
      operation: 'notification_click',
      notificationId: notification.id,
      type: notification.type,
      targetType: notification.target_type,
      targetId: notification.target_id,
    });
  };

  const handleMarkReadClick = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!isRead) {
      await onMarkRead(notification.id);
    }
  };

  return (
    <div
      className={`flex items-start gap-3 p-3 rounded-lg border cursor-pointer transition-colors hover:bg-muted/50 ${
        isRead ? 'bg-muted/20 opacity-75' : 'bg-background'
      }`}
      onClick={handleClick}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          handleClick();
        }
      }}
      aria-label={`Notification: ${notification.title}. ${isRead ? 'Read' : 'Unread'}. Click to view.`}
    >
      <Icon
        className={`h-5 w-5 mt-0.5 flex-shrink-0 ${getNotificationColor(notification.type)}`}
        aria-hidden="true"
      />

      <div className="flex-1 min-w-0">
        <div className="flex items-start justify-between gap-2">
          <div className="flex-1 min-w-0">
            <h4 className={`text-sm font-medium truncate ${isRead ? 'text-muted-foreground' : 'text-foreground'}`}>
              {notification.title}
            </h4>
            <p className={`text-sm mt-1 ${isRead ? 'text-muted-foreground' : 'text-muted-foreground'}`}>
              {notification.content}
            </p>
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
          </div>
        </div>

        <div className="flex items-center justify-between mt-2">
          <span className="text-xs text-muted-foreground">
            {relativeTime}
          </span>
          {notification.workspace_id && (
            <span className="text-xs text-muted-foreground">
              Workspace: {notification.workspace_id.slice(0, 8)}...
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
