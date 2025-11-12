//! Retry Notification Component
//!
//! Shows user-friendly notifications during automatic retry attempts.
//! Provides visibility into retry progress and allows cancellation.
//!
//! Citations:
//! - docs/Smashing Design Techniques.md L1-L50 - Trust-building UX patterns
//! - ui/src/utils/retry.ts L1-L50 - Retry logic integration

import React, { useState, useEffect } from 'react';
import { Alert, AlertDescription } from './alert';
import { Button } from './button';
import { Progress } from './progress';
import { AlertTriangle, X, RefreshCw } from 'lucide-react';
import { logger } from '../../utils/logger';

export interface RetryNotificationProps {
  operation: string; // Description of the operation being retried
  attempt: number;
  maxAttempts: number;
  delayMs: number; // Delay until next retry
  onCancel?: () => void; // Allow user to cancel retries
  className?: string;
}

export function RetryNotification({
  operation,
  attempt,
  maxAttempts,
  delayMs,
  onCancel,
  className = ''
}: RetryNotificationProps) {
  const [timeRemaining, setTimeRemaining] = useState(delayMs);
  const [isCancelled, setIsCancelled] = useState(false);

  useEffect(() => {
    setTimeRemaining(delayMs);
    if (delayMs <= 0) {
      return;
    }

    let remaining = delayMs;
    const interval = window.setInterval(() => {
      remaining = Math.max(remaining - 100, 0);
      setTimeRemaining(prev => Math.max(prev - 100, 0));
      if (remaining <= 0) {
        window.clearInterval(interval);
      }
    }, 100);

    return () => window.clearInterval(interval);
  }, [delayMs]);

  const progressPercent = ((maxAttempts - attempt + 1) / maxAttempts) * 100;
  const delaySeconds = Math.ceil(timeRemaining / 1000);

  const handleCancel = () => {
    setIsCancelled(true);
    onCancel?.();

    logger.info('User cancelled retry attempts', {
      component: 'RetryNotification',
      operation: 'handleCancel',
      operationDesc: operation,
      attempt,
      maxAttempts
    });
  };

  if (isCancelled) {
    return (
      <Alert className={`border-gray-300 bg-gray-50 ${className}`}>
        <AlertTriangle className="h-4 w-4 text-gray-500" />
        <AlertDescription className="text-gray-700">
          Retry cancelled for {operation}
        </AlertDescription>
      </Alert>
    );
  }

  if (timeRemaining <= 0) {
    return null;
  }

  return (
    <Alert className={`border-gray-300 bg-gray-50 ${className}`}>
      <RefreshCw className="h-4 w-4 text-gray-400 animate-spin" />
      <AlertDescription className="text-gray-700">
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <span className="font-medium">
              Retrying {operation}...
            </span>
            <span className="text-sm text-gray-600">
              {attempt}/{maxAttempts}
            </span>
          </div>

          <Progress value={progressPercent} className="h-2" />

          <div className="flex items-center justify-between text-sm">
            <span>
              Next attempt in {delaySeconds} second{delaySeconds !== 1 ? 's' : ''}
            </span>
            {onCancel && (
              <Button
                variant="outline"
                size="sm"
                onClick={handleCancel}
                className="h-6 px-2 text-xs"
              >
                <X className="h-3 w-3 mr-1" />
                Cancel
              </Button>
            )}
          </div>
        </div>
      </AlertDescription>
    </Alert>
  );
}

// Global retry notification manager
class RetryNotificationManager {
  private activeNotifications = new Map<string, {
    element: HTMLElement;
    timeoutId: number;
  }>();

  show(operation: string, attempt: number, maxAttempts: number, delayMs: number, onCancel?: () => void) {
    const key = `${operation}_${attempt}`;

    // Remove existing notification for this operation
    this.hide(operation);

    // Create container for the notification
    const container = document.createElement('div');
    container.id = `retry-notification-${key}`;
    container.className = 'fixed top-4 right-4 z-50 max-w-sm';
    document.body.appendChild(container);

    // Render the notification
    import('react-dom/client').then(({ createRoot }) => {
      const root = createRoot(container);
      root.render(
        <RetryNotification
          operation={operation}
          attempt={attempt}
          maxAttempts={maxAttempts}
          delayMs={delayMs}
          onCancel={() => {
            onCancel?.();
            this.hide(operation);
          }}
        />
      );

      // Auto-hide after delay
      const timeoutId = window.setTimeout(() => {
        this.hide(operation);
      }, delayMs + 1000);

      this.activeNotifications.set(operation, {
        element: container,
        timeoutId
      });
    });
  }

  hide(operation: string) {
    const notification = this.activeNotifications.get(operation);
    if (notification) {
      clearTimeout(notification.timeoutId);
      if (notification.element.parentNode) {
        notification.element.parentNode.removeChild(notification.element);
      }
      this.activeNotifications.delete(operation);
    }
  }

  hideAll() {
    for (const [operation] of this.activeNotifications) {
      this.hide(operation);
    }
  }
}

// Global instance
export const retryNotificationManager = new RetryNotificationManager();

// Cleanup on page unload
if (typeof window !== 'undefined') {
  window.addEventListener('beforeunload', () => {
    retryNotificationManager.hideAll();
  });
}
