//! Notification center dialog
//!
//! Unified feed showing alerts, messages, mentions, activity.
//! Filter by type and mark as read functionality.
//!
//! Citation: ui/src/components/dashboard/ActivityFeedWidget.tsx L85-L196
//! - Unified feed showing alerts, messages, mentions, activity
//! - Filter by type (Select component per L97-L123)
//! - Mark as read/unread functionality

import React, { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { ScrollArea } from './ui/scroll-area';
import { useNotifications } from '@/hooks/useNotifications';
import { NotificationItem } from './NotificationItem';
import { logger } from '@/utils/logger';
import { CheckCheck, RefreshCw } from 'lucide-react';

type NotificationType = 'all' | 'system_alert' | 'user_message' | 'activity_event' | 'resource_share' | 'mention';

interface NotificationCenterProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function NotificationCenter({ open, onOpenChange }: NotificationCenterProps) {
  const [typeFilter, setTypeFilter] = useState<NotificationType>('all');
  const [tab, setTab] = useState<'all' | 'unread'>('unread');

  const { notifications, summary, loading, error, refresh, markRead, markAllRead } = useNotifications({
    enabled: open, // Only fetch when dialog is open
    maxNotifications: 100,
  });

  const handleMarkAllRead = async () => {
    try {
      await markAllRead();
      logger.info('All notifications marked as read', {
        component: 'NotificationCenter',
        operation: 'mark_all_read',
      });
    } catch (err) {
      logger.error('Failed to mark all notifications as read', {
        component: 'NotificationCenter',
        operation: 'mark_all_read',
      }, err instanceof Error ? err : new Error(String(err)));
    }
  };

  const handleRefresh = async () => {
    await refresh();
  };

  // Filter notifications based on tab and type
  const filteredNotifications = notifications.filter(notification => {
    const typeMatch = typeFilter === 'all' || notification.type === typeFilter;
    const readMatch = tab === 'all' || !notification.read_at;
    return typeMatch && readMatch;
  });

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[80vh] flex flex-col backdrop-blur-xl bg-background/95 border-border/50">
        <DialogHeader>
          <div className="flex items-center justify-between">
            <DialogTitle className="flex items-center gap-2">
              Notifications
              {summary && (
                <Badge variant="outline" className="text-xs bg-background/50">
                  {summary.unread_count} unread
                </Badge>
              )}
            </DialogTitle>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={handleRefresh}
                disabled={loading}
                aria-label="Refresh notifications"
                className="bg-background/50 hover:bg-background/80"
              >
                <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
              </Button>
              {summary && summary.unread_count > 0 && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={handleMarkAllRead}
                  aria-label="Mark all as read"
                  className="bg-background/50 hover:bg-background/80"
                >
                  <CheckCheck className="h-4 w-4 mr-1" />
                  Mark All Read
                </Button>
              )}
            </div>
          </div>
        </DialogHeader>

        <div className="flex items-center gap-4 py-4 border-b border-border/30">
          <Select value={typeFilter} onValueChange={(value) => setTypeFilter(value as NotificationType)}>
            <SelectTrigger className="w-[180px]">
              <SelectValue placeholder="Filter by type" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All types</SelectItem>
              <SelectItem value="system_alert">System alerts</SelectItem>
              <SelectItem value="user_message">Messages</SelectItem>
              <SelectItem value="activity_event">Activity</SelectItem>
              <SelectItem value="resource_share">Resource shares</SelectItem>
              <SelectItem value="mention">Mentions</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <Tabs value={tab} onValueChange={(value) => setTab(value as 'all' | 'unread')} className="flex-1 flex flex-col">
          <TabsList className="grid w-full grid-cols-2">
            <TabsTrigger value="unread">Unread ({summary?.unread_count ?? 0})</TabsTrigger>
            <TabsTrigger value="all">All ({notifications.length})</TabsTrigger>
          </TabsList>

          <TabsContent value="unread" className="flex-1 flex flex-col mt-4">
            <ScrollArea className="flex-1">
              {loading && (
                <div className="space-y-3 p-4">
                  {Array.from({ length: 3 }).map((_, i) => (
                    <div key={i} className="space-y-2">
                      <div className="h-4 bg-muted animate-pulse rounded" />
                      <div className="h-3 bg-muted animate-pulse rounded w-5/6" />
                      <div className="h-3 bg-muted animate-pulse rounded w-4/6" />
                    </div>
                  ))}
                </div>
              )}

              {error && (
                <div className="text-center py-8 text-sm text-destructive">
                  Failed to load notifications: {error}
                </div>
              )}

              {!loading && !error && filteredNotifications.length === 0 && (
                <div className="text-center py-8 text-sm text-muted-foreground">
                  {tab === 'unread' ? 'No unread notifications' : 'No notifications'}
                </div>
              )}

              {!loading && !error && filteredNotifications.length > 0 && (
                <div className="space-y-1 p-2">
                  {filteredNotifications.map((notification) => (
                    <NotificationItem
                      key={notification.id}
                      notification={notification}
                      onMarkRead={markRead}
                    />
                  ))}
                </div>
              )}
            </ScrollArea>
          </TabsContent>

          <TabsContent value="all" className="flex-1 flex flex-col mt-4">
            <ScrollArea className="flex-1">
              {loading && (
                <div className="space-y-3 p-4">
                  {Array.from({ length: 3 }).map((_, i) => (
                    <div key={i} className="space-y-2">
                      <div className="h-4 bg-muted animate-pulse rounded" />
                      <div className="h-3 bg-muted animate-pulse rounded w-5/6" />
                      <div className="h-3 bg-muted animate-pulse rounded w-4/6" />
                    </div>
                  ))}
                </div>
              )}

              {error && (
                <div className="text-center py-8 text-sm text-destructive">
                  Failed to load notifications: {error}
                </div>
              )}

              {!loading && !error && filteredNotifications.length === 0 && (
                <div className="text-center py-8 text-sm text-muted-foreground">
                  No notifications match the current filter
                </div>
              )}

              {!loading && !error && filteredNotifications.length > 0 && (
                <div className="space-y-1 p-2">
                  {filteredNotifications.map((notification) => (
                    <NotificationItem
                      key={notification.id}
                      notification={notification}
                      onMarkRead={markRead}
                    />
                  ))}
                </div>
              )}
            </ScrollArea>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
