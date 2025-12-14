/**
 * Confirmation Dialog Hook
 *
 * Provides promise-based confirmation dialog state management.
 * Supports customizable titles, messages, and action labels.
 *
 * Usage:
 * ```tsx
 * const { confirm, ConfirmationDialog } = useConfirmation();
 *
 * const handleDelete = async () => {
 *   const confirmed = await confirm({
 *     title: 'Delete Adapter',
 *     message: 'Are you sure you want to delete this adapter?',
 *     confirmLabel: 'Delete',
 *     cancelLabel: 'Cancel',
 *     variant: 'destructive',
 *   });
 *   if (confirmed) {
 *     await deleteAdapter(adapterId);
 *   }
 * };
 * ```
 *
 * Citations:
 * - docs/UI_INTEGRATION.md - Dialog patterns
 */

import { useState, useCallback, useRef } from 'react';

export type ConfirmationVariant = 'default' | 'destructive' | 'warning' | 'info';

export interface ConfirmationOptions {
  /** Dialog title */
  title: string;
  /** Dialog message/description */
  message: string;
  /** Label for confirm button (default: "Confirm") */
  confirmLabel?: string;
  /** Label for cancel button (default: "Cancel") */
  cancelLabel?: string;
  /** Visual variant affecting button styling */
  variant?: ConfirmationVariant;
  /** Additional details to show (e.g., list of affected items) */
  details?: string[];
  /** Whether the action is irreversible (shows stronger warning) */
  irreversible?: boolean;
}

export interface ConfirmationState extends ConfirmationOptions {
  isOpen: boolean;
}

export interface UseConfirmationReturn {
  /** Current confirmation dialog state */
  state: ConfirmationState;
  /** Open confirmation dialog and return promise that resolves to user's choice */
  confirm: (options: ConfirmationOptions) => Promise<boolean>;
  /** Programmatically confirm (resolve promise with true) */
  handleConfirm: () => void;
  /** Programmatically cancel (resolve promise with false) */
  handleCancel: () => void;
  /** Check if dialog is currently open */
  isOpen: boolean;
  /** Close dialog without resolving (same as cancel) */
  close: () => void;
}

const defaultState: ConfirmationState = {
  isOpen: false,
  title: '',
  message: '',
  confirmLabel: 'Confirm',
  cancelLabel: 'Cancel',
  variant: 'default',
  details: undefined,
  irreversible: false,
};

/**
 * Hook for managing confirmation dialog state with promise-based API.
 *
 * @returns Confirmation state and control functions
 */
export function useConfirmation(): UseConfirmationReturn {
  const [state, setState] = useState<ConfirmationState>(defaultState);

  // Store resolve function for the current confirmation promise
  const resolveRef = useRef<((value: boolean) => void) | null>(null);

  const confirm = useCallback((options: ConfirmationOptions): Promise<boolean> => {
    return new Promise<boolean>((resolve) => {
      resolveRef.current = resolve;
      setState({
        isOpen: true,
        title: options.title,
        message: options.message,
        confirmLabel: options.confirmLabel ?? 'Confirm',
        cancelLabel: options.cancelLabel ?? 'Cancel',
        variant: options.variant ?? 'default',
        details: options.details,
        irreversible: options.irreversible ?? false,
      });
    });
  }, []);

  const handleConfirm = useCallback(() => {
    if (resolveRef.current) {
      resolveRef.current(true);
      resolveRef.current = null;
    }
    setState(defaultState);
  }, []);

  const handleCancel = useCallback(() => {
    if (resolveRef.current) {
      resolveRef.current(false);
      resolveRef.current = null;
    }
    setState(defaultState);
  }, []);

  const close = useCallback(() => {
    handleCancel();
  }, [handleCancel]);

  return {
    state,
    confirm,
    handleConfirm,
    handleCancel,
    isOpen: state.isOpen,
    close,
  };
}

/**
 * Hook for pre-configured destructive action confirmation.
 * Convenience wrapper around useConfirmation.
 */
export function useDestructiveConfirmation(): UseConfirmationReturn & {
  confirmDelete: (itemName: string, itemType?: string) => Promise<boolean>;
} {
  const confirmation = useConfirmation();

  const confirmDelete = useCallback(
    (itemName: string, itemType: string = 'item'): Promise<boolean> => {
      return confirmation.confirm({
        title: `Delete ${itemType}`,
        message: `Are you sure you want to delete "${itemName}"? This action cannot be undone.`,
        confirmLabel: 'Delete',
        cancelLabel: 'Cancel',
        variant: 'destructive',
        irreversible: true,
      });
    },
    [confirmation]
  );

  return {
    ...confirmation,
    confirmDelete,
  };
}

export default useConfirmation;
