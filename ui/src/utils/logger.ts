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
  errorType?: string;
  details?: string;
  // Enhanced specificity fields
  recoverySuggestion?: string;
  userJourney?: string; // e.g., 'login_flow', 'document_upload', 'inference_request'
  validationFailures?: string[]; // For validation errors
  timing?: {
    operationStart?: number;
    operationDuration?: number;
    networkLatency?: number;
  };
  resourceState?: Record<string, unknown>; // Current state of relevant resources
  environment?: {
    userAgent?: string;
    viewport?: string;
    online?: boolean;
    memoryUsage?: number;
  };
  featureFlags?: Record<string, boolean>; // Active feature flags
  rateLimit?: {
    remaining?: number;
    resetTime?: number;
    limit?: number;
  };
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

  // Track recent request IDs for error correlation
  private recentRequestIds: string[] = [];
  private readonly MAX_RECENT_REQUEST_IDS = 10;

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

  /** Track a request ID for error correlation */
  trackRequestId(requestId: string): void {
    this.recentRequestIds.unshift(requestId);
    if (this.recentRequestIds.length > this.MAX_RECENT_REQUEST_IDS) {
      this.recentRequestIds.pop();
    }
  }

  /** Get the most recent request ID for error correlation */
  private getRecentRequestId(): string | undefined {
    return this.recentRequestIds[0];
  }

  /** Collect current environment context for error reporting */
  private collectEnvironmentContext(): LogContext['environment'] {
    if (typeof window === 'undefined') return {};

    return {
      userAgent: navigator.userAgent,
      viewport: `${window.innerWidth}x${window.innerHeight}`,
      online: navigator.onLine,
      memoryUsage: (performance as { memory?: { usedJSHeapSize?: number } }).memory?.usedJSHeapSize,
    };
  }

  /** Create enhanced error context with automatic environment collection */
  createErrorContext(baseContext: LogContext, options?: {
    includeEnvironment?: boolean;
    includeTiming?: boolean;
    operationStart?: number;
  }): LogContext {
    const enhanced: LogContext = { ...baseContext };

    // Auto-include environment context if requested
    if (options?.includeEnvironment !== false) {
      enhanced.environment = this.collectEnvironmentContext();
    }

    // Add timing information if requested
    if (options?.includeTiming && options.operationStart) {
      enhanced.timing = {
        operationStart: options.operationStart,
        operationDuration: Date.now() - options.operationStart,
      };
    }

    return enhanced;
  }

  /** Enhanced error logging with recovery suggestions and categorization */
  errorWithRecovery(
    message: string,
    context: LogContext,
    error?: Error,
    recoverySuggestion?: string
  ): void {
    const enhancedContext = this.createErrorContext({
      ...context,
      recoverySuggestion,
    });
    this.log(LogLevel.ERROR, message, enhancedContext, error);
  }

  /** Validation error with detailed failure information */
  validationError(
    message: string,
    context: LogContext,
    validationFailures: string[],
    error?: Error
  ): void {
    const enhancedContext = this.createErrorContext({
      ...context,
      errorType: context.errorType || 'validation_failure',
      validationFailures,
      recoverySuggestion: 'Please check the highlighted fields and correct the validation errors.',
    });
    this.log(LogLevel.ERROR, message, enhancedContext, error);
  }

  /** Network error with specific categorization */
  networkError(
    message: string,
    context: LogContext,
    networkDetails: {
      status?: number;
      statusText?: string;
      url?: string;
      method?: string;
      timeout?: boolean;
      connectionError?: boolean;
    },
    error?: Error
  ): void {
    const recoverySuggestion = networkDetails.timeout
      ? 'The request timed out. Please check your connection and try again.'
      : networkDetails.connectionError
      ? 'Network connection failed. Please check your internet connection.'
      : networkDetails.status === 429
      ? 'Rate limit exceeded. Please wait before retrying.'
      : networkDetails.status && networkDetails.status >= 500
      ? 'Server error occurred. Please try again later or contact support.'
      : 'Network request failed. Please try again.';

    const enhancedContext = this.createErrorContext({
      ...context,
      errorType: 'network_failure',
      recoverySuggestion,
      networkDetails,
    });
    this.log(LogLevel.ERROR, message, enhancedContext, error);
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

    // Try to extract request ID from recent API calls for correlation
    if (!logEntry.context.requestId) {
      const recentRequestId = this.getRecentRequestId();
      if (recentRequestId) {
        logEntry.context.requestId = recentRequestId;
      }
    }

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

  warn(message: string, context?: LogContext, error?: Error) {
    this.log(LogLevel.WARN, message, context, error);
  }

  error(message: string, context?: LogContext, error?: Error) {
    this.log(LogLevel.ERROR, message, context, error);
  }
}

export const logger = new Logger();
