/**
 * useVerificationReports - Manage verification reports for chat traces
 *
 * Provides caching and fetching for receipt verification results.
 *
 * @example
 * ```tsx
 * const {
 *   reports,
 *   fetchReport,
 *   dialogState,
 *   openDialog,
 *   closeDialog,
 * } = useVerificationReports();
 * ```
 */

import { useState, useCallback } from 'react';
import { apiClient } from '@/api/services';
import { logger, toError } from '@/utils/logger';
import { toast } from 'sonner';
import type { ReceiptVerificationResult } from '@/api/api-types';

// ============================================================================
// Types
// ============================================================================

/**
 * Dialog state for verification report display
 */
export interface VerificationDialogState {
  /** Trace ID being displayed */
  traceId: string | null;
  /** Error message if fetch failed */
  error: string | null;
  /** True if currently loading */
  loading: boolean;
}

/**
 * Hook return value
 */
export interface UseVerificationReportsReturn {
  /** Map of traceId to verification report */
  reports: Record<string, ReceiptVerificationResult>;
  /** Fetch a verification report (with optional silent mode) */
  fetchReport: (traceId: string, silent?: boolean) => Promise<ReceiptVerificationResult | null>;
  /** Current dialog state */
  dialogState: VerificationDialogState;
  /** Open the verification dialog for a trace */
  openDialog: (traceId: string) => void;
  /** Close the verification dialog */
  closeDialog: () => void;
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * Manage verification reports for chat traces
 *
 * Features:
 * - Report caching by trace ID
 * - Silent fetch mode for background updates
 * - Dialog state management
 */
export function useVerificationReports(): UseVerificationReportsReturn {
  const [reports, setReports] = useState<Record<string, ReceiptVerificationResult>>({});
  const [dialogState, setDialogState] = useState<VerificationDialogState>({
    traceId: null,
    error: null,
    loading: false,
  });

  const fetchReport = useCallback(
    async (traceId: string, silent = false): Promise<ReceiptVerificationResult | null> => {
      if (!traceId) return null;
      if (reports[traceId]) return reports[traceId];

      if (!silent) {
        setDialogState((prev) => ({ ...prev, loading: true, error: null }));
      }

      try {
        const report = await apiClient.verifyTraceReceipt(traceId);
        setReports((prev) => ({ ...prev, [traceId]: report }));
        return report;
      } catch (err) {
        const error = toError(err);
        if (!silent) {
          setDialogState((prev) => ({ ...prev, error: error.message }));
          toast.error(`Verification failed: ${error.message}`);
        }
        logger.error(
          'Verification fetch failed',
          {
            component: 'useVerificationReports',
            traceId,
          },
          error
        );
        return null;
      } finally {
        if (!silent) {
          setDialogState((prev) => ({ ...prev, loading: false }));
        }
      }
    },
    [reports]
  );

  const openDialog = useCallback((traceId: string) => {
    setDialogState({ traceId, error: null, loading: false });
  }, []);

  const closeDialog = useCallback(() => {
    setDialogState({ traceId: null, error: null, loading: false });
  }, []);

  return {
    reports,
    fetchReport,
    dialogState,
    openDialog,
    closeDialog,
  };
}
