import { useState, useEffect, useCallback } from 'react';
import apiClient from '@/api/client';
import { Adapter } from '@/api/types';
import { logger, toError } from '@/utils/logger';
import { toast } from 'sonner';
import { READY_ADAPTER_STATES } from '@/components/inference/constants';

export interface UseAdapterSelectionOptions {
  /** Initial adapter ID from URL or props */
  initialAdapterId?: string;
  /** Callback when adapter loading fails */
  onLoadError?: (error: Error) => void;
}

export interface UseAdapterSelectionReturn {
  /** List of available adapters */
  adapters: Adapter[];
  /** Currently selected adapter ID */
  selectedAdapterId: string;
  /** Set selected adapter ID */
  setSelectedAdapterId: (id: string) => void;
  /** Currently selected adapter object */
  selectedAdapter: Adapter | undefined;
  /** Current adapter strength (null if no adapter selected) */
  adapterStrength: number | null;
  /** Set adapter strength locally */
  setAdapterStrength: (value: number | null) => void;
  /** Whether strength update is in progress */
  isStrengthUpdating: boolean;
  /** Commit strength change to server */
  commitStrength: (value: number) => Promise<void>;
  /** Whether adapters are loading */
  isLoading: boolean;
  /** Error message if adapter loading failed */
  error: string | null;
  /** Reload adapters */
  reload: () => Promise<void>;
}

/**
 * Hook for managing adapter selection and strength adjustment.
 * Handles adapter loading, selection, and real-time strength updates.
 */
export function useAdapterSelection(
  options: UseAdapterSelectionOptions = {}
): UseAdapterSelectionReturn {
  const { initialAdapterId, onLoadError } = options;

  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [selectedAdapterId, setSelectedAdapterId] = useState<string>('none');
  const [adapterStrength, setAdapterStrength] = useState<number | null>(null);
  const [isStrengthUpdating, setIsStrengthUpdating] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load adapters from API
  const loadAdapters = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      const adapterList = await apiClient.listAdapters();
      setAdapters(adapterList);

      // Check for initial adapter parameter
      if (initialAdapterId) {
        const targetAdapter = adapterList.find(
          (a: Adapter) => a.id === initialAdapterId || a.adapter_id === initialAdapterId
        );
        if (targetAdapter) {
          setSelectedAdapterId(targetAdapter.id);
          return;
        } else {
          logger.warn('Requested adapter not found', {
            component: 'useAdapterSelection',
            operation: 'loadAdapters',
            requestedAdapter: initialAdapterId,
          });
        }
      }

      // Fallback: Select first active adapter if available
      const activeAdapter = adapterList.find(
        (a: Adapter) =>
          a.current_state && READY_ADAPTER_STATES.includes(a.current_state as typeof READY_ADAPTER_STATES[number])
      );
      if (activeAdapter?.id) {
        setSelectedAdapterId(activeAdapter.id);
      }
    } catch (err) {
      const loadError = err instanceof Error ? err : new Error('Failed to load adapters');
      logger.error(
        'Failed to load adapters',
        {
          component: 'useAdapterSelection',
          operation: 'loadAdapters',
        },
        loadError
      );
      setError(loadError.message);
      onLoadError?.(loadError);
    } finally {
      setIsLoading(false);
    }
  }, [initialAdapterId, onLoadError]);

  // Commit strength change to server
  const commitStrength = useCallback(
    async (value: number) => {
      const targetId = selectedAdapterId && selectedAdapterId !== 'none' ? selectedAdapterId : null;
      if (!targetId) return;

      setAdapterStrength(value);
      setIsStrengthUpdating(true);

      try {
        await apiClient.updateAdapterStrength(targetId, value);
        setAdapters((prev) =>
          prev.map((adapter) =>
            adapter.id === targetId ? { ...adapter, lora_strength: value } : adapter
          )
        );
        toast.success(`Strength set to ${value.toFixed(2)}`);
      } catch (err) {
        toast.error(err instanceof Error ? err.message : 'Failed to update strength');
        logger.error(
          'Failed to update adapter strength',
          {
            component: 'useAdapterSelection',
            operation: 'commitStrength',
            adapterId: targetId,
            value,
          },
          toError(err)
        );
      } finally {
        setIsStrengthUpdating(false);
      }
    },
    [selectedAdapterId]
  );

  // Load adapters on mount
  useEffect(() => {
    loadAdapters();
  }, [loadAdapters]);

  // Update strength when selected adapter changes
  useEffect(() => {
    const target = adapters.find((a) => a.id === selectedAdapterId);
    if (target) {
      setAdapterStrength(target.lora_strength ?? 1);
    } else {
      setAdapterStrength(null);
    }
  }, [adapters, selectedAdapterId]);

  const selectedAdapter = adapters.find((a) => a.id === selectedAdapterId);

  return {
    adapters,
    selectedAdapterId,
    setSelectedAdapterId,
    selectedAdapter,
    adapterStrength,
    setAdapterStrength,
    isStrengthUpdating,
    commitStrength,
    isLoading,
    error,
    reload: loadAdapters,
  };
}
