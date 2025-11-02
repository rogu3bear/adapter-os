//! Notification bell component for header
//!
//! Shows unread notification count and provides quick access to notification center.
//! Integrated into RootLayout header.
//!
//! Citation: Button pattern from ui/src/layout/RootLayout.tsx L132-L143
//! - Badge overlay for unread count
//! - Dropdown/Dialog for notification list

import React, { useCallback, useMemo } from 'react';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Bell, BellRing } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';
import { useNotifications } from '../hooks/useNotifications';
import { logger } from '../utils/logger';

interface NotificationBellProps {
  onOpenChange?: (open: boolean) => void;
  showCountLabel?: boolean;
}

export function NotificationBell({ onOpenChange, showCountLabel = false }: NotificationBellProps) {
  const { summary, loading, error } = useNotifications({
    enabled: true,
    maxNotifications: 5, // Just for preview in dropdown
  });

  const setDialogOpen = useCallback((value: boolean) => {
    onOpenChange?.(value);
  }, [onOpenChange]);

  const unreadCount = summary?.unread_count ?? 0;
  const hasUnread = unreadCount > 0;

  const unreadLabel = useMemo(() => {
    if (hasUnread) {
      return `${unreadCount} unread`;
    }
    return 'All caught up';
  }, [hasUnread, unreadCount]);

  const handleClick = () => {
    setDialogOpen(true);
    logger.info('Notification bell clicked', {
      component: 'NotificationBell',
      operation: 'open_center',
      unreadCount,
    });
  };

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="relative"
            onClick={handleClick}
            aria-label={`Notifications ${hasUnread ? `(${unreadCount} unread)` : '(no unread)'}`}
            title={`Notifications ${hasUnread ? `(${unreadCount} unread)` : '(no unread)'}`}
          >
            {hasUnread ? (
              <BellRing className="h-5 w-5" />
            ) : (
              <Bell className="h-5 w-5" />
            )}
            {hasUnread && (
              <Badge
                variant="destructive"
                className="absolute -top-1 -right-1 h-5 w-5 rounded-full p-0 flex items-center justify-center text-xs"
              >
                {unreadCount > 99 ? '99+' : unreadCount}
              </Badge>
            )}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-80">
          <div className="px-2 py-1.5 border-b">
            <div className="flex items-center justify-between">
              <h3 className="font-semibold">Notifications</h3>
              {summary && (
                <Badge variant="outline" className="text-xs">
                  {unreadCount} unread
                </Badge>
              )}
            </div>
          </div>

          <div className="p-2">
            {loading && (
              <div className="space-y-2">
                <div className="h-4 bg-muted animate-pulse rounded" />
                <div className="h-4 bg-muted animate-pulse rounded w-5/6" />
                <div className="h-4 bg-muted animate-pulse rounded w-4/6" />
              </div>
            )}

            {error && (
              <div className="text-sm text-destructive py-2">
                Failed to load notifications
              </div>
            )}

            {!loading && !error && summary && (
              <div className="space-y-2">
                {summary.unread_count === 0 ? (
                  <div className="text-center py-4 text-sm text-muted-foreground">
                    No unread notifications
                  </div>
                ) : (
                  <div className="text-sm">
                    You have {summary.unread_count} unread notification{summary.unread_count !== 1 ? 's' : ''}
                  </div>
                )}

                <Button
                  variant="outline"
                  size="sm"
                  className="w-full"
                  onClick={() => setDialogOpen(true)}
                >
                  View All Notifications
                </Button>
              </div>
            )}
          </div>
        </DropdownMenuContent>
      </DropdownMenu>

      {showCountLabel && (
        <span className="text-xs text-muted-foreground hidden sm:inline">
          {unreadLabel}
        </span>
      )}
    </>
  );
}
