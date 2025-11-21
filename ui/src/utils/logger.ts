// Structured logging utility with telemetry integration.
// Complies with Policy Pack #9 (Telemetry) - canonical JSON events.

import { toast } from 'sonner';

export enum LogLevel {
  DEBUG = 'debug',
  INFO = 'info',
  WARN = 'warn',
  ERROR = 'error',
}

export interface LogContext {
  component?: string;
  operation?: string;
  userId?: string;
  tenantId?: string;
  requestId?: string;
  [key: string]: unknown;
}

export interface LogEntry {
  timestamp: string;
  level: LogLevel;
  message: string;
  context: LogContext;
  error?: {
    name: string;
    message: string;
    stack?: string;
  };
}


/** Normalize unknown error-like values to proper Error instances. */
export const toError = (error: unknown): Error => {
  if (error instanceof Error) {
    return error;
  }
  if (typeof error === 'string') {
    return new Error(error);
  }
  if (error === undefined) {
    return new Error('undefined');
  }
  try {
    return new Error(JSON.stringify(error));
  } catch {
    return new Error(String(error));
  }
};

class Logger {
  // Track recent error toasts to prevent spam (message -> last shown timestamp)
  private errorToastHistory = new Map<string, number>();
  private readonly ERROR_TOAST_THROTTLE_MS = 10000; // 10 seconds

  // Allow tests to override development mode detection
  private _isDevelopmentOverride: boolean | null = null;

  private get isDevelopment(): boolean {
    if (this._isDevelopmentOverride !== null) {
      return this._isDevelopmentOverride;
    }
    return import.meta.env.DEV;
  }

  /** For testing only: override development mode detection */
  setDevelopmentMode(isDev: boolean | null): void {
    this._isDevelopmentOverride = isDev;
  }

  /** Log a message with structured context. */
  log(level: LogLevel, message: string, context?: LogContext, error?: Error) {
    const logEntry: LogEntry = {
      timestamp: new Date().toISOString(),
      level,
      message,
      context: context || {},
      error: error ? {
        name: error.name,
        message: error.message,
        stack: error.stack,
      } : undefined,
    };

    // Development: Console logging with structured format
    if (this.isDevelopment) {
      const consoleMethod = level === LogLevel.ERROR ? 'error' :
                           level === LogLevel.WARN ? 'warn' : 'log';
      console[consoleMethod](`[${level.toUpperCase()}] ${message}`, logEntry);
    }

    // Production: Send to telemetry endpoint
    if (!this.isDevelopment) {
      this.sendToTelemetry(logEntry);
    }


    // User-facing errors: Show toast notification (with throttling to prevent spam)
    if (level === LogLevel.ERROR && error) {
      const isBackgroundOperation = context?.operation === 'fetchNotifications' ||
                                   context?.operation === 'sse_init' ||
                                   context?.operation === 'storage_listener';

      // For background operations, suppress toasts (errors still logged and shown in UI)
      // For user-initiated actions, show toast with throttling
      if (!isBackgroundOperation) {
        const now = Date.now();
        const lastShown = this.errorToastHistory.get(message);

        // Show toast if we haven't shown this error recently (throttle duplicate errors)
        if (!lastShown || (now - lastShown) > this.ERROR_TOAST_THROTTLE_MS) {
          toast.error(message);
          this.errorToastHistory.set(message, now);

          // Clean up old entries periodically to prevent memory leaks
          if (this.errorToastHistory.size > 50) {
            const cutoff = now - this.ERROR_TOAST_THROTTLE_MS * 2;
            for (const [key, timestamp] of this.errorToastHistory.entries()) {
              if (timestamp < cutoff) {
                this.errorToastHistory.delete(key);
              }
            }
          }
        }
      }
      // Background operations: errors are logged but no toast shown
      // Users can see errors in NotificationCenter UI or check console logs
    }
  }

  private async sendToTelemetry(logEntry: LogEntry) {
    try {

      // Transform to UnifiedTelemetryEvent format expected by backend
      const telemetryEvent = {
        id: logEntry.timestamp.replace(/[:.]/g, '-'), // Create deterministic ID
        timestamp: logEntry.timestamp,
        event_type: logEntry.level === LogLevel.ERROR ? 'client_error' : 'client_log',
        level: this.mapLogLevelToUnified(logEntry.level),
        message: logEntry.message,
        component: logEntry.context.component,
        tenant_id: logEntry.context.tenantId,
        user_id: logEntry.context.userId,
        metadata: logEntry.context,
      };

      await fetch('/api/v1/telemetry/logs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify([telemetryEvent]), // Backend expects array
        credentials: 'include', // Include cookies for authentication
      });
    } catch (err) {
      // Fallback to console in case of telemetry failure
      window.console.error('Failed to send log to telemetry:', err);
    }
  }

  private mapLogLevelToUnified(level: LogLevel): 'Debug' | 'Info' | 'Warn' | 'Error' | 'Critical' {
    switch (level) {
      case LogLevel.DEBUG: return 'Debug';
      case LogLevel.INFO: return 'Info';
      case LogLevel.WARN: return 'Warn';
      case LogLevel.ERROR: return 'Error';
      default: return 'Info';
    }
  }

  debug(message: string, context?: LogContext) {
    this.log(LogLevel.DEBUG, message, context);
  }

  info(message: string, context?: LogContext) {
    this.log(LogLevel.INFO, message, context);
  }

  warn(message: string, context?: LogContext) {
    this.log(LogLevel.WARN, message, context);
  }

  error(message: string, context?: LogContext, error?: Error) {
    this.log(LogLevel.ERROR, message, context, error);
  }
}

export const logger = new Logger();
