import { useState } from 'react';
import { toast } from 'sonner';
import { logger } from '../utils/logger';
import apiClient from '../api/client';
import { ErrorRecoveryTemplates } from '../components/ui/error-recovery';
import { useCancellableOperation } from './useCancellableOperation';
import type { AdapterCategory, CategoryPolicy } from '../api/types';

export interface UseAdapterOperationsOptions {
  onAdapterUpdate?: (adapterId: string, updates: Partial<any>) => void;
  onAdapterEvict?: (adapterId: string) => void;
  onAdapterPin?: (adapterId: string, pinned: boolean) => void;
  onPolicyUpdate?: (category: AdapterCategory, policy: CategoryPolicy) => void;
  onDataRefresh?: () => void | Promise<void>;
}

export interface UseAdapterOperationsReturn {
  isOperationLoading: boolean;
  operationError: React.ReactElement | null;
  // Individual loading states for optimistic updates
  isEvicting: boolean;
  isPinning: boolean;
  isPromoting: boolean;
  isDeleting: boolean;
  isUpdatingPolicy: boolean;
  // Cancellation support
  cancelOperation: () => void;
  evictAdapter: (adapterId: string) => Promise<void>;
  pinAdapter: (adapterId: string, pinned: boolean) => Promise<void>;
  promoteAdapter: (adapterId: string) => Promise<void>;
  deleteAdapter: (adapterId: string) => Promise<void>;
  updateCategoryPolicy: (category: AdapterCategory, policy: CategoryPolicy) => Promise<void>;
}

export function useAdapterOperations(options: UseAdapterOperationsOptions = {}): UseAdapterOperationsReturn {
  const [isOperationLoading, setIsOperationLoading] = useState(false);
  const [operationError, setOperationError] = useState<React.ReactElement | null>(null);

  // Individual loading states for optimistic updates
  const [isEvicting, setIsEvicting] = useState(false);
  const [isPinning, setIsPinning] = useState(false);
  const [isPromoting, setIsPromoting] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [isUpdatingPolicy, setIsUpdatingPolicy] = useState(false);

  // Cancellation support for long-running operations
  const { start: startCancellableOperation, cancel: cancelCancellableOperation } = useCancellableOperation();

  const {
    onAdapterUpdate,
    onAdapterEvict,
    onAdapterPin,
    onPolicyUpdate,
    onDataRefresh,
  } = options;

  const evictAdapter = async (adapterId: string) => {
    setIsEvicting(true);
    setIsOperationLoading(true);
    setOperationError(null);
    try {
      const result = await apiClient.evictAdapter(adapterId);
      toast.success('Adapter evicted successfully');
      onAdapterEvict?.(adapterId);
      // Trigger data refresh
      await onDataRefresh?.();
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Unknown error';
      setOperationError(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMessage),
          () => evictAdapter(adapterId)
        )
      );
      logger.error('Failed to evict adapter', {
        operation: 'evictAdapter',
        adapterId,
        error: errorMessage
      });
    } finally {
      setIsEvicting(false);
      setIsOperationLoading(false);
    }
  };

  const pinAdapter = async (adapterId: string, pinned: boolean) => {
    setIsPinning(true);
    setIsOperationLoading(true);
    setOperationError(null);
    try {
      await apiClient.pinAdapter(adapterId, pinned);
      toast.success(pinned ? 'Adapter pinned successfully' : 'Adapter unpinned successfully');
      onAdapterPin?.(adapterId, pinned);
      // Trigger data refresh
      await onDataRefresh?.();
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Unknown error';
      setOperationError(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMessage),
          () => pinAdapter(adapterId, pinned)
        )
      );
      logger.error('Failed to pin/unpin adapter', {
        operation: 'pinAdapter',
        adapterId,
        pinned,
        error: errorMessage
      });
    } finally {
      setIsPinning(false);
      setIsOperationLoading(false);
    }
  };

  const promoteAdapter = async (adapterId: string) => {
    setIsPromoting(true);
    setIsOperationLoading(true);
    setOperationError(null);

    try {
      await startCancellableOperation(async (signal) => {
        await apiClient.promoteAdapterState(adapterId, {}, false, signal);
        toast.success('Adapter state promoted successfully');
        onAdapterUpdate?.(adapterId, { current_state: 'hot' }); // Simplified update
        // Trigger data refresh
        await onDataRefresh?.();
      }, `promote-adapter-${adapterId}`);
    } catch (err) {
      if (err) { // Only set error if not cancelled
        const errorMessage = err instanceof Error ? err.message : 'Unknown error';
        setOperationError(
          ErrorRecoveryTemplates.genericError(
            err instanceof Error ? err : new Error(errorMessage),
            () => promoteAdapter(adapterId)
          )
        );
        logger.error('Failed to promote adapter', {
          operation: 'promoteAdapter',
          adapterId,
          error: errorMessage
        });
      }
    } finally {
      setIsPromoting(false);
      setIsOperationLoading(false);
    }
  };

  const deleteAdapter = async (adapterId: string) => {
    setIsDeleting(true);
    setIsOperationLoading(true);
    setOperationError(null);
    try {
      await apiClient.deleteAdapter(adapterId);
      toast.success('Adapter deleted successfully');
      // Trigger data refresh
      await onDataRefresh?.();
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Unknown error';
      setOperationError(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMessage),
          () => deleteAdapter(adapterId)
        )
      );
      logger.error('Failed to delete adapter', {
        operation: 'deleteAdapter',
        adapterId,
        error: errorMessage
      });
    } finally {
      setIsDeleting(false);
      setIsOperationLoading(false);
    }
  };

  const updateCategoryPolicy = async (category: AdapterCategory, policy: CategoryPolicy) => {
    setIsUpdatingPolicy(true);
    setIsOperationLoading(true);
    setOperationError(null);
    try {
      const updatedPolicy = await apiClient.updateCategoryPolicy(category, policy);
      toast.success(`Policy updated successfully for ${category}`);
      onPolicyUpdate?.(category, updatedPolicy);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Unknown error';
      setOperationError(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMessage),
          () => updateCategoryPolicy(category, policy)
        )
      );
      logger.error('Failed to update category policy', {
        operation: 'updateCategoryPolicy',
        category,
        error: errorMessage
      });
    } finally {
      setIsUpdatingPolicy(false);
      setIsOperationLoading(false);
    }
  };

  return {
    isOperationLoading,
    operationError,
    // Individual loading states for optimistic updates
    isEvicting,
    isPinning,
    isPromoting,
    isDeleting,
    isUpdatingPolicy,
    // Cancellation support
    cancelOperation: cancelCancellableOperation,
    evictAdapter,
    pinAdapter,
    promoteAdapter,
    deleteAdapter,
    updateCategoryPolicy,
  };
}
