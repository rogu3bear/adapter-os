//! Structured logging utility for AdapterOS UI
//!
//! Provides structured logging with telemetry integration and user-facing error handling.
//! Replaces console.log/error/warn usage throughout the application.
//!
//! # Citations
//! - CONTRIBUTING.md L123: "Use `tracing` for logging (not `println!`)"
//! - CLAUDE.md L130: "Use `tracing` for logging (not `println!`)"
//! - Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"
//!
//! # Examples
//!
//! ```typescript
//! import { logger } from './utils/logger';
//!
//! // Info logging
//! logger.info('User logged in', { userId: 'user-123', tenantId: 'default' });
//!
//! // Error logging with context
//! logger.error('Failed to fetch data', { component: 'Dashboard', operation: 'fetchData' }, error);
//! ```

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

/**
 * Normalize unknown error-like values to proper Error instances.
 *
 * Ensures all logger callers can safely pass any thrown value without
 * duplicating conversion logic at call sites.
 */
export const toError = (error: unknown): Error => {
  if (error instanceof Error) {
    return error;
  }
  if (typeof error === 'string') {
    return new Error(error);
  }
  try {
    return new Error(JSON.stringify(error));
  } catch {
    return new Error(String(error));
  }
};

class Logger {
  private isDevelopment = import.meta.env.DEV;
  // Track recent error toasts to prevent spam (message -> last shown timestamp)
  private errorToastHistory = new Map<string, number>();
  private readonly ERROR_TOAST_THROTTLE_MS = 10000; // 10 seconds
  
  /**
   * Log a message with structured context and error information
   *
   * # Arguments
   *
   * * `level` - Log level (debug, info, warn, error)
   * * `message` - Human-readable log message
   * * `context` - Structured context data
   * * `error` - Error object if applicable
   *
   * # Policy Compliance
   *
   * - Policy Pack #9 (Telemetry): Logs events with canonical JSON structure
   * - CONTRIBUTING.md L123: Uses structured logging instead of console.log
   */
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

  /**
   * Send log entry to telemetry endpoint
   *
   * # Policy Compliance
   *
   * - Policy Pack #9 (Telemetry): Canonical JSON serialization
   * - Policy Pack #1 (Egress): Uses relative API paths only
   */
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

  /**
   * Log debug information
   *
   * # Arguments
   *
   * * `message` - Debug message
   * * `context` - Optional context data
   */
  debug(message: string, context?: LogContext) {
    this.log(LogLevel.DEBUG, message, context);
  }

  /**
   * Log informational message
   *
   * # Arguments
   *
   * * `message` - Info message
   * * `context` - Optional context data
   */
  info(message: string, context?: LogContext) {
    this.log(LogLevel.INFO, message, context);
  }

  /**
   * Log warning message
   *
   * # Arguments
   *
   * * `message` - Warning message
   * * `context` - Optional context data
   */
  warn(message: string, context?: LogContext) {
    this.log(LogLevel.WARN, message, context);
  }

  /**
   * Log error message with optional error object
   *
   * # Arguments
   *
   * * `message` - Error message
   * * `context` - Optional context data
   * * `error` - Optional Error object
   */
  error(message: string, context?: LogContext, error?: Error) {
    this.log(LogLevel.ERROR, message, context, error);
  }
}

export const logger = new Logger();
