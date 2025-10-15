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

class Logger {
  private isDevelopment = import.meta.env.DEV;
  
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

    // User-facing errors: Show toast notification
    if (level === LogLevel.ERROR && error) {
      toast.error(message);
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
      await fetch('/api/v1/telemetry/logs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(logEntry),
      });
    } catch (err) {
      // Fallback to console in case of telemetry failure
      console.error('Failed to send log to telemetry:', err);
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
