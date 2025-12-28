import { toast } from 'sonner';
import { captureException } from '@/stores/errorStore';
import { logger, toError } from '@/utils/logger';
import { enhanceError } from '@/utils/errorMessages';
import { TENANT_ACCESS_DENIED_EVENT } from '@/utils/tenant';

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

    const failureCode = (err as { failure_code?: string }).failure_code ?? (err as { code?: string }).code;

    if (failureCode === 'TENANT_ACCESS_DENIED' || (err as { status?: number }).status === 403) {
      window.dispatchEvent(new CustomEvent(TENANT_ACCESS_DENIED_EVENT, { detail: err }));
      toast.error('Workspace access denied', {
        description: 'Select a permitted workspace and try again. (TENANT_ACCESS_DENIED)',
      });
      return;
    }

    const enhanced = enhanceError(err, { operation: 'query/mutation' });
    const message = err.message || enhanced.userFriendly.message || 'Request failed';
    const toastTitle = failureCode
      ? `${enhanced.userFriendly.title} (${failureCode})`
      : enhanced.userFriendly.title || 'Request failed';

    logger.error('React Query request failed', {
      component: 'react-query',
      operation: 'query/mutation',
      details: message,
      failureCode,
    }, err);

    captureException(err, {
      component: 'react-query',
      operation: 'query error',
    });

    toast.error(toastTitle, {
      description: message,
    });
  };
}
