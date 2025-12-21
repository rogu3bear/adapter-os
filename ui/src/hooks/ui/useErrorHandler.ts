import { useCallback } from 'react';
import { toast } from 'sonner';
import { logUIError, type UIErrorSeverity } from '@/lib/logUIError';
import { logger } from '@/utils/logger';

interface ErrorHandlerOptions {
  component?: string;
  operation?: string;
  showToast?: boolean;
  severity?: UIErrorSeverity;
  onError?: (error: Error) => void;
}

interface ErrorContext {
  statusCode?: number;
  requestId?: string;
  endpoint?: string;
  [key: string]: unknown;
}

/**
 * Extract user-friendly error message from various error types
 */
function extractErrorMessage(error: unknown): string {
  if (typeof error === 'string') return error;

  if (error instanceof Error) {
    // Check for API error responses
    const apiError = error as Error & {
      response?: { data?: { message?: string; error?: string }; status?: number };
      message?: string;
    };

    if (apiError.response?.data?.message) {
      return apiError.response.data.message;
    }

    if (apiError.response?.data?.error) {
      return apiError.response.data.error;
    }

    return error.message;
  }

  return 'An unexpected error occurred';
}

/**
 * Extract error context for logging
 */
function extractErrorContext(error: unknown): ErrorContext {
  const context: ErrorContext = {};

  if (error && typeof error === 'object') {
    const apiError = error as {
      response?: { status?: number; headers?: Record<string, string> };
      config?: { url?: string };
    };

    if (apiError.response?.status) {
      context.statusCode = apiError.response.status;
    }

    if (apiError.response?.headers?.['x-request-id']) {
      context.requestId = apiError.response.headers['x-request-id'];
    }

    if (apiError.config?.url) {
      context.endpoint = apiError.config.url;
    }
  }

  return context;
}

/**
 * useErrorHandler - Hook for consistent error handling across the application
 *
 * Features:
 * - Automatic toast notifications
 * - Error logging with context
 * - User-friendly message extraction
 * - TypeScript-safe error handling
 *
 * Usage:
 * ```tsx
 * const { handleError } = useErrorHandler({
 *   component: 'TrainingPage',
 *   operation: 'loadJobs'
 * });
 *
 * try {
 *   await fetchData();
 * } catch (error) {
 *   handleError(error);
 * }
 * ```
 */
export function useErrorHandler(defaultOptions: ErrorHandlerOptions = {}) {
  const handleError = useCallback((
    error: unknown,
    options: ErrorHandlerOptions = {}
  ) => {
    const mergedOptions = { ...defaultOptions, ...options };
    const {
      component = 'Unknown',
      operation = 'operation',
      showToast = true,
      severity = 'error',
      onError,
    } = mergedOptions;

    // Convert to Error object
    const errorObj = error instanceof Error ? error : new Error(String(error));

    // Extract user-friendly message
    const message = extractErrorMessage(error);

    // Extract context for logging
    const context = extractErrorContext(error);

    // Log error
    logger.error(`Error in ${component} during ${operation}`, {
      component,
      operation,
      ...context,
    }, errorObj);

    // Log to UI error tracking
    logUIError(errorObj, {
      scope: 'hook',
      component,
      severity,
    });

    // Show toast notification
    if (showToast) {
      switch (severity) {
        case 'critical':
        case 'error':
          toast.error(message, {
            description: context.requestId ? `Request ID: ${context.requestId}` : undefined,
          });
          break;
        case 'warning':
          toast.warning(message);
          break;
        case 'info':
          toast.info(message);
          break;
      }
    }

    // Call custom error handler
    onError?.(errorObj);
  }, [defaultOptions]);

  return { handleError };
}

/**
 * useQueryErrorHandler - Specialized error handler for TanStack Query
 *
 * Usage with useQuery:
 * ```tsx
 * const { onError } = useQueryErrorHandler({ component: 'AdaptersPage' });
 *
 * const { data } = useQuery({
 *   queryKey: ['adapters'],
 *   queryFn: fetchAdapters,
 *   onError: onError('loadAdapters'),
 * });
 * ```
 */
export function useQueryErrorHandler(defaultOptions: ErrorHandlerOptions = {}) {
  const { handleError } = useErrorHandler(defaultOptions);

  const onError = useCallback((operation: string) => {
    return (error: unknown) => {
      handleError(error, { operation, ...defaultOptions });
    };
  }, [handleError, defaultOptions]);

  return { onError, handleError };
}
