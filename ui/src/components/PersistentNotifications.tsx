//! Persistent notification system for long-running operations
//!
//! Shows floating notifications that persist until completion or manual dismissal.
//! Used for training jobs, adapter loading, and other async operations.
//!
//! Features:
//! - Persistent notifications stay until completed or dismissed
//! - Stacking: new notifications appear below persistent ones
//! - Completion state with link to the completed resource
//! - Progress indicator for ongoing operations
//! - Shadow styling for visibility against white backgrounds

import React, { createContext, useContext, useState, useCallback, useEffect, useRef, KeyboardEvent } from 'react';
import { X, CheckCircle2, AlertCircle, Loader2, ExternalLink, ChevronDown, ChevronUp } from 'lucide-react';
import { Button } from './ui/button';
import { Progress } from './ui/progress';
import { cn } from './ui/utils';
import { useNavigate } from 'react-router-dom';
import { logger } from '@/utils/logger';

export type PersistentNotificationStatus = 'pending' | 'in_progress' | 'completed' | 'failed';

export interface PersistentNotification {
  id: string;
  title: string;
  description?: string;
  status: PersistentNotificationStatus;
  progress?: number; // 0-100
  resourceType?: 'adapter' | 'training_job' | 'document' | 'model' | 'workspace';
  resourceId?: string;
  resourceName?: string;
  linkPath?: string;
  metadata?: Record<string, unknown>;
  createdAt: Date;
  updatedAt: Date;
  persistent: boolean; // If true, won't auto-dismiss
  autoCloseDelay?: number; // ms to wait before auto-closing after completion
}

interface PersistentNotificationContextType {
  notifications: PersistentNotification[];
  addNotification: (notification: Omit<PersistentNotification, 'id' | 'createdAt' | 'updatedAt'>) => string;
  updateNotification: (id: string, updates: Partial<Omit<PersistentNotification, 'id' | 'createdAt'>>) => void;
  removeNotification: (id: string) => void;
  clearCompleted: () => void;
  config: {
    defaultAutoCloseDelay: number;
    maxVisibleNotifications: number;
  };
}

const PersistentNotificationContext = createContext<PersistentNotificationContextType | undefined>(undefined);

// No-op implementation for when hook is used outside provider
const noOpContext: PersistentNotificationContextType = {
  notifications: [],
  addNotification: () => '',
  updateNotification: () => {},
  removeNotification: () => {},
  clearCompleted: () => {},
  config: {
    defaultAutoCloseDelay: 5000,
    maxVisibleNotifications: 5,
  },
};

/**
 * Hook to access persistent notification system.
 * Returns no-op functions when used outside PersistentNotificationProvider,
 * allowing components to safely call notification functions without checking.
 */
export function usePersistentNotifications(): PersistentNotificationContextType {
  const context = useContext(PersistentNotificationContext);
  return context ?? noOpContext;
}

/**
 * Check if persistent notifications are available (inside provider)
 */
export function usePersistentNotificationsAvailable(): boolean {
  const context = useContext(PersistentNotificationContext);
  return context !== undefined;
}

interface PersistentNotificationProviderProps {
  children: React.ReactNode;
  /** Default auto-close delay for completed notifications (ms). Default: 5000 */
  defaultAutoCloseDelay?: number;
  /** Maximum number of visible notifications. Default: 5 */
  maxVisibleNotifications?: number;
}

export function PersistentNotificationProvider({
  children,
  defaultAutoCloseDelay = 5000,
  maxVisibleNotifications = 5,
}: PersistentNotificationProviderProps) {
  const [notifications, setNotifications] = useState<PersistentNotification[]>([]);
  const autoCloseTimers = useRef<Map<string, NodeJS.Timeout>>(new Map());

  // Clean up timers on unmount
  useEffect(() => {
    const timers = autoCloseTimers.current;
    return () => {
      timers.forEach(timer => clearTimeout(timer));
    };
  }, []);

  const addNotification = useCallback((notification: Omit<PersistentNotification, 'id' | 'createdAt' | 'updatedAt'>): string => {
    const id = `pn-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const now = new Date();

    const newNotification: PersistentNotification = {
      ...notification,
      id,
      createdAt: now,
      updatedAt: now,
    };

    setNotifications(prev => [...prev, newNotification]);

    logger.info('Persistent notification added', {
      component: 'PersistentNotifications',
      operation: 'add',
      notificationId: id,
      title: notification.title,
      status: notification.status,
    });

    return id;
  }, []);

  const updateNotification = useCallback((id: string, updates: Partial<Omit<PersistentNotification, 'id' | 'createdAt'>>) => {
    setNotifications(prev => prev.map(n => {
      if (n.id !== id) return n;

      const updated = {
        ...n,
        ...updates,
        updatedAt: new Date(),
      };

      // Handle auto-close for completed/failed non-persistent notifications
      if ((updates.status === 'completed' || updates.status === 'failed') && !updated.persistent) {
        const delay = updated.autoCloseDelay ?? defaultAutoCloseDelay;

        // Clear any existing timer
        const existingTimer = autoCloseTimers.current.get(id);
        if (existingTimer) clearTimeout(existingTimer);

        // Set new timer
        const timer = setTimeout(() => {
          setNotifications(p => p.filter(notif => notif.id !== id));
          autoCloseTimers.current.delete(id);
        }, delay);
        autoCloseTimers.current.set(id, timer);
      }

      return updated;
    }));

    logger.debug('Persistent notification updated', {
      component: 'PersistentNotifications',
      operation: 'update',
      notificationId: id,
      updates,
    });
  }, [defaultAutoCloseDelay]);

  const removeNotification = useCallback((id: string) => {
    // Clear any auto-close timer
    const timer = autoCloseTimers.current.get(id);
    if (timer) {
      clearTimeout(timer);
      autoCloseTimers.current.delete(id);
    }

    setNotifications(prev => prev.filter(n => n.id !== id));

    logger.info('Persistent notification removed', {
      component: 'PersistentNotifications',
      operation: 'remove',
      notificationId: id,
    });
  }, []);

  const clearCompleted = useCallback(() => {
    setNotifications(prev => prev.filter(n => n.status !== 'completed' && n.status !== 'failed'));
  }, []);

  const config = { defaultAutoCloseDelay, maxVisibleNotifications };

  return (
    <PersistentNotificationContext.Provider value={{
      notifications,
      addNotification,
      updateNotification,
      removeNotification,
      clearCompleted,
      config,
    }}>
      {children}
      <PersistentNotificationContainer />
      {import.meta.env.DEV && <DevNotificationTester />}
    </PersistentNotificationContext.Provider>
  );
}

function PersistentNotificationContainer() {
  const { notifications, config } = usePersistentNotifications();
  const [isCollapsed, setIsCollapsed] = useState(false);

  // Sort: persistent/in_progress first, then by creation date
  const sortedNotifications = [...notifications].sort((a, b) => {
    // Persistent and in_progress come first
    const aWeight = (a.persistent || a.status === 'in_progress') ? 0 : 1;
    const bWeight = (b.persistent || b.status === 'in_progress') ? 0 : 1;
    if (aWeight !== bWeight) return aWeight - bWeight;

    // Then by creation date (newest first for completed, oldest first for in_progress)
    if (a.status === 'in_progress' || a.status === 'pending') {
      return a.createdAt.getTime() - b.createdAt.getTime();
    }
    return b.createdAt.getTime() - a.createdAt.getTime();
  });

  if (notifications.length === 0) return null;

  const activeCount = notifications.filter(n => n.status === 'in_progress' || n.status === 'pending').length;

  return (
    <div
      role="region"
      aria-label="Notifications"
      aria-live="polite"
      className={cn(
        "fixed z-50 flex flex-col items-end gap-2",
        // Mobile: full width with padding, centered at bottom
        "bottom-2 left-2 right-2 max-w-none",
        // Desktop: fixed width, bottom-right corner
        "sm:bottom-4 sm:left-auto sm:right-4 sm:max-w-md"
      )}
    >
      {/* Collapse/Expand toggle when multiple notifications */}
      {notifications.length > 1 && (
        <Button
          variant="outline"
          size="sm"
          onClick={() => setIsCollapsed(!isCollapsed)}
          aria-expanded={!isCollapsed}
          aria-controls="notification-stack"
          className={cn(
            "shadow-lg backdrop-blur-xl bg-background/90 border-border/50",
            "hover:bg-background/95",
            "text-xs sm:text-sm" // Smaller text on mobile
          )}
        >
          {isCollapsed ? (
            <>
              <ChevronUp className="h-4 w-4 mr-1" aria-hidden="true" />
              <span className="sr-only">Expand</span>
              <span aria-hidden="true">Show {notifications.length} notifications</span>
              {activeCount > 0 && (
                <span className="ml-1 px-1.5 py-0.5 text-xs bg-primary/20 text-primary rounded-full">
                  {activeCount} active
                </span>
              )}
            </>
          ) : (
            <>
              <ChevronDown className="h-4 w-4 mr-1" aria-hidden="true" />
              <span className="sr-only">Collapse notifications</span>
              <span aria-hidden="true">Collapse</span>
            </>
          )}
        </Button>
      )}

      {/* Notification stack */}
      {!isCollapsed && (
        <div
          id="notification-stack"
          role="list"
          aria-label="Active notifications"
          className="flex flex-col gap-2 w-full"
        >
          {sortedNotifications.slice(0, config.maxVisibleNotifications).map((notification) => (
            <PersistentNotificationItem key={notification.id} notification={notification} />
          ))}
          {sortedNotifications.length > config.maxVisibleNotifications && (
            <div className="text-xs text-muted-foreground text-center py-1">
              +{sortedNotifications.length - config.maxVisibleNotifications} more notifications
            </div>
          )}
        </div>
      )}

      {/* Collapsed preview - show first active notification */}
      {isCollapsed && activeCount > 0 && (
        <div className="w-full" role="list" aria-label="Active notification preview">
          {sortedNotifications
            .filter(n => n.status === 'in_progress' || n.status === 'pending')
            .slice(0, 1)
            .map(notification => (
              <PersistentNotificationItem key={notification.id} notification={notification} compact />
            ))}
        </div>
      )}
    </div>
  );
}

interface PersistentNotificationItemProps {
  notification: PersistentNotification;
  compact?: boolean;
}

function PersistentNotificationItem({ notification, compact = false }: PersistentNotificationItemProps) {
  const { removeNotification } = usePersistentNotifications();
  const navigate = useNavigate();
  const itemRef = useRef<HTMLDivElement>(null);

  const statusConfig = {
    pending: {
      icon: Loader2,
      iconClass: 'text-muted-foreground animate-spin',
      bgClass: 'bg-muted/10 border-border/50',
      label: 'Pending',
    },
    in_progress: {
      icon: Loader2,
      iconClass: 'text-blue-500 animate-spin',
      bgClass: 'bg-blue-500/10 border-blue-500/30',
      label: 'In Progress',
    },
    completed: {
      icon: CheckCircle2,
      iconClass: 'text-green-500',
      bgClass: 'bg-green-500/10 border-green-500/30',
      label: 'Completed',
    },
    failed: {
      icon: AlertCircle,
      iconClass: 'text-red-500',
      bgClass: 'bg-red-500/10 border-red-500/30',
      label: 'Failed',
    },
  };

  const config = statusConfig[notification.status];
  const Icon = config.icon;

  const handleNavigate = () => {
    if (notification.linkPath) {
      navigate(notification.linkPath);
      // Auto-dismiss completed notifications when navigating
      if (notification.status === 'completed' || notification.status === 'failed') {
        removeNotification(notification.id);
      }
    }
  };

  // Keyboard handling: Escape to dismiss
  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      removeNotification(notification.id);
    }
  };

  const formatMetadata = (metadata: Record<string, unknown>) => {
    const items: { label: string; value: string }[] = [];

    if (metadata.duration_ms) {
      items.push({ label: 'Duration', value: `${(Number(metadata.duration_ms) / 1000).toFixed(1)}s` });
    }
    if (metadata.vram_mb || metadata.vram_delta_mb) {
      const vram = metadata.vram_mb || metadata.vram_delta_mb;
      items.push({ label: 'VRAM', value: `${Number(vram).toFixed(0)} MB` });
    }
    if (metadata.epoch && metadata.total_epochs) {
      items.push({ label: 'Epoch', value: `${metadata.epoch}/${metadata.total_epochs}` });
    }
    if (metadata.loss) {
      items.push({ label: 'Loss', value: Number(metadata.loss).toFixed(4) });
    }
    if (metadata.adapter_name) {
      items.push({ label: 'Adapter', value: String(metadata.adapter_name) });
    }

    return items;
  };

  // Build accessible label
  const accessibleLabel = `${config.label}: ${notification.title}${notification.description ? `. ${notification.description}` : ''}${notification.progress !== undefined ? `. ${notification.progress.toFixed(0)}% complete` : ''}`;

  return (
    <div
      ref={itemRef}
      role="listitem"
      tabIndex={0}
      onKeyDown={handleKeyDown}
      aria-label={accessibleLabel}
      className={cn(
        "w-full rounded-lg border shadow-xl backdrop-blur-xl transition-all duration-300",
        "animate-in slide-in-from-right-full",
        "focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-2",
        config.bgClass,
        // Mobile: smaller padding
        compact ? "p-2" : "p-3 sm:p-4"
      )}
      style={{
        boxShadow: '0 8px 32px rgba(0, 0, 0, 0.12), 0 4px 16px rgba(0, 0, 0, 0.08)',
      }}
    >
      <div className="flex items-start gap-2 sm:gap-3">
        {/* Status Icon */}
        <div className="flex-shrink-0 mt-0.5" aria-hidden="true">
          <Icon className={cn("h-4 w-4 sm:h-5 sm:w-5", config.iconClass)} />
        </div>

        {/* Content */}
        <div className="flex-1 min-w-0">
          <div className="flex items-start justify-between gap-2">
            <div className="flex-1 min-w-0">
              <h4 className={cn(
                "font-medium text-foreground",
                compact ? "text-xs sm:text-sm truncate" : "text-xs sm:text-sm"
              )}>
                {notification.title}
              </h4>

              {!compact && notification.description && (
                <p className="text-xs sm:text-sm text-muted-foreground mt-0.5 line-clamp-2">
                  {notification.description}
                </p>
              )}
            </div>

            {/* Dismiss button */}
            <Button
              variant="ghost"
              size="icon"
              className="h-6 w-6 flex-shrink-0 opacity-60 hover:opacity-100 focus:opacity-100"
              onClick={() => removeNotification(notification.id)}
              aria-label={`Dismiss ${notification.title} notification`}
            >
              <X className="h-4 w-4" aria-hidden="true" />
            </Button>
          </div>

          {/* Progress bar for in_progress */}
          {!compact && notification.status === 'in_progress' && notification.progress !== undefined && (
            <div className="mt-2" role="progressbar" aria-valuenow={notification.progress} aria-valuemin={0} aria-valuemax={100}>
              <Progress value={notification.progress} className="h-1 sm:h-1.5" />
              <div className="flex justify-between text-[10px] sm:text-xs text-muted-foreground mt-1">
                <span>{config.label}</span>
                <span aria-live="polite">{notification.progress.toFixed(0)}%</span>
              </div>
            </div>
          )}

          {/* Metadata display */}
          {!compact && notification.metadata && Object.keys(notification.metadata).length > 0 && (
            <div className="flex flex-wrap gap-x-2 sm:gap-x-3 gap-y-1 mt-2 text-[10px] sm:text-xs text-muted-foreground">
              {formatMetadata(notification.metadata).map(({ label, value }) => (
                <span key={label}>
                  <span className="opacity-70">{label}:</span>{' '}
                  <span className="font-medium text-foreground/80">{value}</span>
                </span>
              ))}
            </div>
          )}

          {/* Completion actions */}
          {!compact && (notification.status === 'completed' || notification.status === 'failed') && notification.linkPath && (
            <div className="mt-2 sm:mt-3 flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={handleNavigate}
                className="text-[10px] sm:text-xs h-6 sm:h-7 bg-background/50 hover:bg-background/80"
              >
                <ExternalLink className="h-3 w-3 mr-1" aria-hidden="true" />
                {notification.resourceName
                  ? `View ${notification.resourceName}`
                  : `View ${notification.resourceType || 'resource'}`
                }
              </Button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// Convenience hooks for common notification types
export function useTrainingNotification() {
  const { addNotification, updateNotification, removeNotification } = usePersistentNotifications();

  const startTraining = useCallback((jobId: string, adapterName: string) => {
    return addNotification({
      title: `Training: ${adapterName}`,
      description: 'Initializing training job...',
      status: 'in_progress',
      progress: 0,
      resourceType: 'training_job',
      resourceId: jobId,
      resourceName: adapterName,
      linkPath: `/training?job=${jobId}`,
      metadata: { adapter_name: adapterName },
      persistent: true,
    });
  }, [addNotification]);

  const updateProgress = useCallback((notificationId: string, progress: number, metadata?: Record<string, unknown>) => {
    updateNotification(notificationId, {
      progress,
      description: `Training in progress...`,
      metadata,
    });
  }, [updateNotification]);

  const completeTraining = useCallback((notificationId: string, adapterName: string, adapterId: string, metadata?: Record<string, unknown>) => {
    updateNotification(notificationId, {
      status: 'completed',
      title: `Training Complete: ${adapterName}`,
      description: 'Adapter is ready to use',
      progress: 100,
      resourceType: 'adapter',
      resourceId: adapterId,
      resourceName: adapterName,
      linkPath: `/adapters/${adapterId}`,
      metadata,
      persistent: false,
      autoCloseDelay: 10000,
    });
  }, [updateNotification]);

  const failTraining = useCallback((notificationId: string, adapterName: string, error?: string) => {
    updateNotification(notificationId, {
      status: 'failed',
      title: `Training Failed: ${adapterName}`,
      description: error || 'An error occurred during training',
      persistent: false,
      autoCloseDelay: 15000,
    });
  }, [updateNotification]);

  return { startTraining, updateProgress, completeTraining, failTraining, removeNotification };
}

export function useAdapterNotification() {
  const { addNotification, updateNotification, removeNotification } = usePersistentNotifications();

  const startLoading = useCallback((adapterId: string, adapterName: string) => {
    return addNotification({
      title: `Loading: ${adapterName}`,
      description: 'Loading adapter into memory...',
      status: 'in_progress',
      resourceType: 'adapter',
      resourceId: adapterId,
      resourceName: adapterName,
      linkPath: `/adapters/${adapterId}`,
      persistent: true,
    });
  }, [addNotification]);

  const completeLoading = useCallback((notificationId: string, adapterName: string, adapterId: string, metadata?: Record<string, unknown>) => {
    updateNotification(notificationId, {
      status: 'completed',
      title: `Loaded: ${adapterName}`,
      description: 'Adapter is now active',
      resourceType: 'adapter',
      resourceId: adapterId,
      resourceName: adapterName,
      linkPath: `/adapters/${adapterId}`,
      metadata,
      persistent: false,
      autoCloseDelay: 5000,
    });
  }, [updateNotification]);

  const failLoading = useCallback((notificationId: string, adapterName: string, error?: string) => {
    updateNotification(notificationId, {
      status: 'failed',
      title: `Failed to Load: ${adapterName}`,
      description: error || 'Could not load adapter',
      persistent: false,
      autoCloseDelay: 10000,
    });
  }, [updateNotification]);

  return { startLoading, completeLoading, failLoading, removeNotification };
}

// Dev-only test helper - exposes notification functions on window for testing
// Usage in browser console:
//   window.__testNotification.addInProgress()
//   window.__testNotification.complete(id)
//   window.__testNotification.fail(id)
//   window.__testNotification.addCompleted()
if (import.meta.env.DEV) {
  // We need to expose this through a React component that has access to context
  // This will be set by DevNotificationTester component below
}

/**
 * Dev-only component that exposes notification testing on window object.
 * Only rendered in development mode.
 */
export function DevNotificationTester() {
  const { addNotification, updateNotification, removeNotification } = usePersistentNotifications();

  useEffect(() => {
    if (!import.meta.env.DEV) return;

    /* eslint-disable no-console -- dev-only testing utilities */
    const testHelpers = {
      addInProgress: () => {
        const id = addNotification({
          title: 'Test Training: my-adapter-v1',
          description: 'Training in progress...',
          status: 'in_progress',
          progress: 35,
          resourceType: 'training_job',
          resourceId: 'test-job-123',
          resourceName: 'my-adapter-v1',
          linkPath: '/training?job=test-job-123',
          metadata: {
            adapter_name: 'my-adapter-v1',
            epoch: 2,
            total_epochs: 5,
            loss: 0.0234,
          },
          persistent: true,
        });
        console.log('Created in-progress notification:', id);
        return id;
      },
      addCompleted: () => {
        const id = addNotification({
          title: 'Training Complete: my-adapter-v1',
          description: 'Adapter is ready to use',
          status: 'completed',
          progress: 100,
          resourceType: 'adapter',
          resourceId: 'test-adapter-456',
          resourceName: 'my-adapter-v1',
          linkPath: '/adapters/test-adapter-456',
          metadata: {
            adapter_name: 'my-adapter-v1',
            duration_ms: 45000,
            loss: 0.0089,
          },
          persistent: false,
          autoCloseDelay: 15000,
        });
        console.log('Created completed notification:', id);
        return id;
      },
      addFailed: () => {
        const id = addNotification({
          title: 'Training Failed: broken-adapter',
          description: 'Out of memory error during epoch 3',
          status: 'failed',
          resourceType: 'training_job',
          resourceId: 'failed-job-789',
          resourceName: 'broken-adapter',
          linkPath: '/training?job=failed-job-789',
          persistent: false,
          autoCloseDelay: 20000,
        });
        console.log('Created failed notification:', id);
        return id;
      },
      complete: (id: string) => {
        updateNotification(id, {
          status: 'completed',
          title: 'Training Complete: my-adapter-v1',
          description: 'Adapter is ready to use',
          progress: 100,
          persistent: false,
          autoCloseDelay: 10000,
        });
        console.log('Marked notification as completed:', id);
      },
      fail: (id: string, error?: string) => {
        updateNotification(id, {
          status: 'failed',
          description: error || 'An error occurred',
          persistent: false,
          autoCloseDelay: 15000,
        });
        console.log('Marked notification as failed:', id);
      },
      updateProgress: (id: string, progress: number) => {
        updateNotification(id, { progress });
        console.log('Updated progress:', id, progress);
      },
      remove: (id: string) => {
        removeNotification(id);
        console.log('Removed notification:', id);
      },
      demo: () => {
        // Show all notification types at once
        testHelpers.addInProgress();
        testHelpers.addCompleted();
        testHelpers.addFailed();
        console.log('Demo: Added 3 notifications (in_progress, completed, failed)');
      },
    };

    (window as unknown as Record<string, unknown>).__testNotification = testHelpers;
    console.log('🔔 Dev notification tester available. Try: window.__testNotification.demo()');
    /* eslint-enable no-console */

    return () => {
      delete (window as unknown as Record<string, unknown>).__testNotification;
    };
  }, [addNotification, updateNotification, removeNotification]);

  return null; // This component renders nothing
}
