import { useState, useEffect, useCallback } from 'react';
import apiClient from '@/api/client';
import { CoremlPackageStatus, Adapter } from '@/api/types';
import { isCoremlPackageUiEnabled } from '@/config/featureFlags';
import { logger, toError } from '@/utils/logger';
import { toast } from 'sonner';
import {
  extractCoremlErrorMessage,
  getExportBadgeVariant,
  getVerificationBadgeVariant,
  getExportStatusLabel,
  getVerificationStatusLabel,
} from '@/components/inference/helpers';

export interface UseCoreMLManagementOptions {
  /** Adapter ID to check CoreML status for */
  adapterId?: string;
  /** Model ID for CoreML export */
  modelId?: string;
  /** Selected adapter object (for fallback status) */
  selectedAdapter?: Adapter | null;
  /** Whether CoreML backend is available */
  coremlAvailable?: boolean;
}

export interface UseCoreMLManagementReturn {
  /** CoreML package status */
  status: CoremlPackageStatus | null;
  /** Loading state for status fetch */
  isLoading: boolean;
  /** Action currently in progress */
  actionInProgress: 'export' | 'verify' | null;
  /** Whether CoreML UI is enabled */
  isUiEnabled: boolean;
  /** Whether CoreML is supported for this adapter */
  isSupported: boolean;
  /** Whether CoreML actions are available */
  actionsAvailable: boolean;
  /** Export status display */
  exportStatus: { label: string; variant: 'default' | 'secondary' | 'destructive' | 'outline' };
  /** Verification status display */
  verificationStatus: { label: string; variant: 'default' | 'secondary' | 'destructive' | 'outline' };
  /** Whether there's a hash mismatch */
  hasMismatch: boolean;
  /** Expected package hash */
  expectedHash?: string;
  /** Actual package hash */
  actualHash?: string;
  /** Trigger CoreML export */
  triggerExport: () => Promise<void>;
  /** Trigger CoreML verification */
  triggerVerification: () => Promise<void>;
  /** Refresh CoreML status */
  refresh: () => Promise<void>;
}

/**
 * Hook for managing CoreML package export and verification.
 * Handles status polling, export/verify actions, and display formatting.
 */
export function useCoreMLManagement(
  options: UseCoreMLManagementOptions = {}
): UseCoreMLManagementReturn {
  const { adapterId, modelId, selectedAdapter, coremlAvailable = true } = options;
  const coremlUiEnabled = isCoremlPackageUiEnabled();

  const [status, setStatus] = useState<CoremlPackageStatus | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [actionInProgress, setActionInProgress] = useState<'export' | 'verify' | null>(null);

  // Refresh CoreML status from API
  const refresh = useCallback(async () => {
    if (!coremlUiEnabled) {
      setStatus({
        supported: false,
        export_available: false,
        verification_status: 'unsupported',
      });
      setIsLoading(false);
      return;
    }

    if (!adapterId || adapterId === 'none') {
      setStatus(null);
      return;
    }

    setIsLoading(true);
    try {
      const fetchedStatus = await apiClient.getCoremlPackageStatus(
        adapterId,
        modelId || undefined
      );
      setStatus(fetchedStatus);
    } catch (error) {
      logger.warn('Failed to load CoreML package status', {
        component: 'useCoreMLManagement',
        operation: 'refresh',
        adapterId,
        modelId,
        error: toError(error),
      });
      setStatus((prev) =>
        prev ?? { supported: false, export_available: false, verification_status: 'unknown' }
      );
    } finally {
      setIsLoading(false);
    }
  }, [coremlUiEnabled, adapterId, modelId]);

  // Trigger CoreML export
  const triggerExport = useCallback(async () => {
    if (!coremlUiEnabled) {
      toast.info('CoreML export is not yet supported in this UI.');
      return;
    }
    if (!adapterId || adapterId === 'none') {
      toast.info('Select an adapter to request a CoreML export.');
      return;
    }

    setActionInProgress('export');
    try {
      const resp = await apiClient.triggerCoremlExport(adapterId, modelId || undefined);

      if (resp?.status?.supported === false) {
        const message = resp?.message || 'CoreML export not supported by server';
        toast.error(message);
        setStatus(resp.status);
        return;
      }

      if (resp?.message) {
        toast.success(resp.message);
      } else {
        toast.success('CoreML export requested');
      }

      if (resp?.status) {
        setStatus(resp.status);
      } else {
        await refresh();
      }
    } catch (error) {
      const message = extractCoremlErrorMessage(error, 'Failed to request CoreML export');
      toast.error(message);
      logger.error(
        'CoreML export request failed',
        {
          component: 'useCoreMLManagement',
          operation: 'triggerExport',
          adapterId,
          modelId,
        },
        toError(error)
      );
    } finally {
      setActionInProgress(null);
    }
  }, [coremlUiEnabled, adapterId, modelId, refresh]);

  // Trigger CoreML verification
  const triggerVerification = useCallback(async () => {
    if (!coremlUiEnabled) {
      toast.info('CoreML verification is not yet supported in this UI.');
      return;
    }
    if (!adapterId || adapterId === 'none') {
      toast.info('Select an adapter to verify its CoreML package.');
      return;
    }

    setActionInProgress('verify');
    try {
      const resp = await apiClient.triggerCoremlVerification(adapterId);

      if (resp?.status?.supported === false) {
        const message = resp?.message || 'CoreML verification not supported by server';
        toast.error(message);
        setStatus(resp.status);
        return;
      }

      if (resp?.message) {
        toast.success(resp.message);
      } else {
        toast.success('CoreML verification requested');
      }

      if (resp?.status) {
        setStatus(resp.status);
      } else {
        await refresh();
      }
    } catch (error) {
      const message = extractCoremlErrorMessage(error, 'Failed to request CoreML verification');
      toast.error(message);
      logger.error(
        'CoreML verification request failed',
        {
          component: 'useCoreMLManagement',
          operation: 'triggerVerification',
          adapterId,
        },
        toError(error)
      );
    } finally {
      setActionInProgress(null);
    }
  }, [coremlUiEnabled, adapterId, refresh]);

  // Load status on mount and when adapter changes
  useEffect(() => {
    refresh();
  }, [refresh]);

  // Resolve status from API or fallback to adapter properties
  const resolvedStatus: CoremlPackageStatus | null =
    status ||
    (selectedAdapter
      ? {
          export_available: selectedAdapter.coreml_export_available,
          export_status: selectedAdapter.coreml_export_status,
          verified: selectedAdapter.coreml_export_verified,
          verification_status: selectedAdapter.coreml_verification_status,
          export_last_exported_at: selectedAdapter.coreml_export_last_exported_at,
          verified_at: selectedAdapter.coreml_export_last_verified_at,
          supported: selectedAdapter.coreml_export_available !== undefined ? true : undefined,
        }
      : null);

  const hasMismatch = coremlUiEnabled && resolvedStatus?.coreml_hash_mismatch === true;
  const isSupported = resolvedStatus?.supported !== false;
  const actionsAvailable =
    coremlUiEnabled &&
    coremlAvailable &&
    !!adapterId &&
    adapterId !== 'none' &&
    isSupported &&
    !actionInProgress &&
    !isLoading;

  const exportStatus = {
    label: getExportStatusLabel(
      coremlUiEnabled,
      resolvedStatus?.export_status,
      resolvedStatus?.export_available
    ),
    variant: getExportBadgeVariant(
      coremlUiEnabled,
      resolvedStatus?.export_status,
      resolvedStatus?.export_available
    ),
  };

  const verificationStatus = {
    label: getVerificationStatusLabel(
      coremlUiEnabled,
      hasMismatch,
      resolvedStatus?.verification_status,
      resolvedStatus?.verified
    ),
    variant: getVerificationBadgeVariant(
      coremlUiEnabled,
      hasMismatch,
      resolvedStatus?.verification_status,
      resolvedStatus?.verified
    ),
  };

  return {
    status: resolvedStatus,
    isLoading,
    actionInProgress,
    isUiEnabled: coremlUiEnabled,
    isSupported,
    actionsAvailable,
    exportStatus,
    verificationStatus,
    hasMismatch,
    expectedHash: resolvedStatus?.coreml_expected_package_hash,
    actualHash: resolvedStatus?.coreml_package_hash,
    triggerExport,
    triggerVerification,
    refresh,
  };
}
