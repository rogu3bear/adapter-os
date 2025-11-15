import { usePolling } from '@/hooks/usePolling';
import apiClient from '@/api/client';
import type { AdapterOSStatus } from '@/api/types';
import { logger, toError } from '@/utils/logger';

/**
 * Shared hook for polling service status.
 *
 * Deduplicates status polling across multiple components (ServiceStatusWidget, ActiveAlertsWidget).
 * Uses fast polling interval (2000ms) for real-time service health monitoring.
 *
 * @example
 * ```tsx
 * const { status, isLoading } = useServiceStatus();
 * const failedServices = status?.services?.filter(s => s.state === 'failed') || [];
 * ```
 */
export function useServiceStatus() {
  const { data: status, isLoading, error } = usePolling<AdapterOSStatus>(
    () => apiClient.getStatus(),
    'fast', // 2000ms polling interval
    {
      operationName: 'useServiceStatus.getStatus',
      showLoadingIndicator: false,
      onError: (err) => {
        logger.error('Failed to fetch service status', { hook: 'useServiceStatus' }, toError(err));
      }
    }
  );

  return {
    status,
    isLoading,
    error,
  };
}
