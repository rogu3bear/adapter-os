import { toast } from 'sonner';
import { captureException } from '@/stores/errorStore';
import { logger, toError } from '@/utils/logger';

const IGNORED_ERROR_NAMES = new Set(['AbortError', 'CanceledError']);

/**
 * Centralized React Query error handler.
 * - Logs via structured logger and error store
 * - Shows a user-facing toast (throttled by sonner)
 * - Ignores benign abort/cancel errors
 *
 * See query presets in docs/query-policy.md.
 */
export function createQueryErrorHandler() {
  return (error: unknown) => {
    const err = toError(error);

    if (IGNORED_ERROR_NAMES.has(err.name)) {
      return;
    }

    const message = err.message || 'Request failed';

    logger.error('React Query request failed', {
      component: 'react-query',
      operation: 'query/mutation',
      details: message,
    }, err);

    captureException(err, {
      component: 'react-query',
      operation: 'query error',
    });

    toast.error('Request failed', {
      description: message,
    });
  };
}

