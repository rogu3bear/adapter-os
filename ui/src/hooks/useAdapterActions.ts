import { useCallback, useEffect, useMemo, useState } from 'react';
import { toast } from 'sonner';
import { logger } from '@/utils/logger';
import { useAdapterOperations, type UseAdapterOperationsOptions } from './useAdapterOperations';

export type AdapterActionType = 'load' | 'unload' | 'delete';

export interface AdapterActionTarget {
  id: string;
  name?: string | null;
  version?: string | null;
  state?: string | null;
}

export interface InlineStatus {
  type: 'conflict' | 'error';
  message: string;
}

interface AdapterAction {
  action: AdapterActionType;
  target: AdapterActionTarget;
}

export interface UseAdapterActionsOptions {
  onRefetch?: () => void | Promise<void>;
  onDeleteSuccess?: () => void;
  onShowPreflight?: UseAdapterOperationsOptions['onShowPreflight'];
}

interface ConfirmationCopy {
  title: string;
  description: string;
  confirmText: string;
  variant: 'default' | 'outline' | 'destructive';
}

export function useAdapterActions(options: UseAdapterActionsOptions = {}) {
  const { onRefetch, onDeleteSuccess, onShowPreflight } = options;

  const [pendingAction, setPendingAction] = useState<AdapterAction | null>(null);
  const [isConfirmOpen, setIsConfirmOpen] = useState(false);
  const [isRunning, setIsRunning] = useState(false);
  const [inlineStatuses, setInlineStatuses] = useState<Record<string, InlineStatus>>({});
  const [highlightedId, setHighlightedId] = useState<string | null>(null);

  const {
    loadAdapter,
    unloadAdapter,
    deleteAdapter,
  } = useAdapterOperations({
    onDataRefresh: onRefetch,
    onShowPreflight,
  });

  const confirmationCopy: ConfirmationCopy | null = useMemo(() => {
    if (!pendingAction) return null;
    const { action, target } = pendingAction;
    const name = target.name || target.id;

    if (action === 'load') {
      return {
        title: 'Load adapter',
        description: `Load ${name} into memory for inference.`,
        confirmText: 'Load adapter',
        variant: 'default',
      };
    }
    if (action === 'unload') {
      return {
        title: 'Unload adapter',
        description: `Remove ${name} from memory. It can be reloaded later.`,
        confirmText: 'Unload adapter',
        variant: 'outline',
      };
    }
    return {
      title: 'Delete adapter',
      description: `Permanently delete ${name}. This cannot be undone.`,
      confirmText: 'Delete adapter',
      variant: 'destructive',
    };
  }, [pendingAction]);

  const clearInlineStatus = useCallback((id: string) => {
    setInlineStatuses(prev => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
  }, []);

  const openAction = useCallback((action: AdapterActionType, target: AdapterActionTarget) => {
    setPendingAction({ action, target });
    setIsConfirmOpen(true);
  }, []);

  const performAction = useCallback(async () => {
    if (!pendingAction) return;
    const { action, target } = pendingAction;

    setIsRunning(true);
    try {
      if (action === 'load') {
        if (!loadAdapter) throw new Error('Load adapter is unavailable');
        await loadAdapter(target.id);
        toast.success(`Loaded ${target.name || target.id}`);
      } else if (action === 'unload') {
        if (!unloadAdapter) throw new Error('Unload adapter is unavailable');
        await unloadAdapter(target.id);
        toast.success(`Unloaded ${target.name || target.id}`);
      } else {
        if (!deleteAdapter) throw new Error('Delete adapter is unavailable');
        await deleteAdapter(target.id);
        toast.success(`Deleted ${target.name || target.id}`);
        onDeleteSuccess?.();
      }

      clearInlineStatus(target.id);
      setHighlightedId(target.id);
      setPendingAction(null);
      setIsConfirmOpen(false);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error';
      setInlineStatuses(prev => ({
        ...prev,
        [target.id]: {
          type: 'error',
          message,
        },
      }));
      logger.error('Adapter action failed', { action, adapterId: target.id }, err as Error);
      toast.error(`Failed to ${action} adapter`, { description: message });
    } finally {
      setIsRunning(false);
    }
  }, [clearInlineStatus, deleteAdapter, loadAdapter, onDeleteSuccess, pendingAction, unloadAdapter]);

  useEffect(() => {
    if (!highlightedId) return;
    const timer = setTimeout(() => setHighlightedId(null), 4000);
    return () => clearTimeout(timer);
  }, [highlightedId]);

  return {
    openAction,
    pendingAction,
    isConfirmOpen,
    setIsConfirmOpen,
    performAction,
    isRunning,
    inlineStatuses,
    highlightedId,
    confirmationCopy,
  };
}

