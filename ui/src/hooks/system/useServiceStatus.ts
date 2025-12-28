import { usePolling } from '@/hooks/realtime/usePolling';
import { logger } from '@/utils/logger';
import type { AdapterOSStatus } from '@/api/types';

/**
 * Shared hook for polling service status.
 *
 * Deduplicates status polling across multiple components (ServiceStatusWidget, ActiveAlertsWidget).
 * Uses fast polling interval (2000ms) for real-time service health monitoring.
 *
 * IMPORTANT: This hook gracefully handles 404 errors when /v1/status endpoint is not available.
 * It will return null status without throwing or logging excessive errors.
 *
 * @example
 * ```tsx
 * const { status, isLoading } = useServiceStatus();
 * const failedServices = status?.services?.filter(s => s.state === 'failed') || [];
 * ```
 */
export function useServiceStatus() {
  const { data: status, isLoading, error, refetch, lastUpdated } = usePolling<AdapterOSStatus | null>(
    async () => {
      try {
        // Use direct fetch to avoid error logging in apiClient for expected 404s
        const response = await fetch('/api/v1/status');
        if (response.status === 404) {
          // Endpoint doesn't exist yet - this is expected
          return null;
        }
        if (!response.ok) {
          // Other errors - let polling handle retry
          logger.warn('Service status check failed', {
            component: 'useServiceStatus',
            operation: 'getStatus',
            status: response.status,
            statusText: response.statusText,
          });
          return null;
        }
        return await response.json();
      } catch (error) {
        logger.warn('Service status request error', {
          component: 'useServiceStatus',
          operation: 'getStatus',
        }, error as Error);
        // Network errors during startup - return null silently
        return null;
      }
    },
    'fast', // 2000ms polling interval
    {
      operationName: 'useServiceStatus.getStatus',
      showLoadingIndicator: false,
    }
  );

  return {
    status,
    isLoading,
    lastUpdated,
    error, // Return actual error so components can distinguish failure modes
    /** True when status is null due to fetch failure (not 404) */
    isFetchError: error !== null && status === null,
    refetch, // Expose refetch for manual refresh triggers
  };
}
