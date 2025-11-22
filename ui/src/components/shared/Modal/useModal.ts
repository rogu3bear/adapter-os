"use client";

import * as React from "react";
import type { UseModalReturn } from "./types";

/**
 * Hook for managing modal state
 *
 * @example
 * ```tsx
 * const modal = useModal<{ id: string }>();
 *
 * // Open with data
 * modal.open({ id: "123" });
 *
 * // In component
 * <Modal open={modal.isOpen} onOpenChange={modal.onOpenChange}>
 *   <p>Editing item: {modal.data?.id}</p>
 * </Modal>
 * ```
 */
export function useModal<T = unknown>(initialOpen = false): UseModalReturn<T> {
  const [isOpen, setIsOpen] = React.useState(initialOpen);
  const [data, setData] = React.useState<T | undefined>(undefined);

  const open = React.useCallback((modalData?: T) => {
    setData(modalData);
    setIsOpen(true);
  }, []);

  const close = React.useCallback(() => {
    setIsOpen(false);
    // Clear data after animation completes
    setTimeout(() => {
      setData(undefined);
    }, 200);
  }, []);

  const toggle = React.useCallback(() => {
    setIsOpen((prev) => !prev);
  }, []);

  const onOpenChange = React.useCallback(
    (open: boolean) => {
      if (open) {
        setIsOpen(true);
      } else {
        close();
      }
    },
    [close]
  );

  return {
    isOpen,
    data,
    open,
    close,
    toggle,
    onOpenChange,
  };
}

/**
 * Hook for managing multiple modals by key
 *
 * @example
 * ```tsx
 * const modals = useModalManager<"edit" | "delete" | "create">();
 *
 * modals.open("edit", { id: "123" });
 * modals.isOpen("edit"); // true
 * modals.close("edit");
 * ```
 */
export function useModalManager<K extends string>() {
  const [openModals, setOpenModals] = React.useState<Map<K, unknown>>(
    new Map()
  );

  const open = React.useCallback(<T>(key: K, data?: T) => {
    setOpenModals((prev) => {
      const next = new Map(prev);
      next.set(key, data);
      return next;
    });
  }, []);

  const close = React.useCallback((key: K) => {
    setOpenModals((prev) => {
      const next = new Map(prev);
      next.delete(key);
      return next;
    });
  }, []);

  const closeAll = React.useCallback(() => {
    setOpenModals(new Map());
  }, []);

  const isOpen = React.useCallback(
    (key: K) => {
      return openModals.has(key);
    },
    [openModals]
  );

  const getData = React.useCallback(
    <T>(key: K): T | undefined => {
      return openModals.get(key) as T | undefined;
    },
    [openModals]
  );

  const onOpenChange = React.useCallback(
    (key: K) => (open: boolean) => {
      if (!open) {
        close(key);
      }
    },
    [close]
  );

  return {
    open,
    close,
    closeAll,
    isOpen,
    getData,
    onOpenChange,
  };
}

/**
 * Hook for confirmation modal with async action support
 *
 * @example
 * ```tsx
 * const confirm = useConfirmation({
 *   onConfirm: async () => {
 *     await deleteItem(id);
 *   },
 * });
 *
 * <button onClick={confirm.trigger}>Delete</button>
 * <ConfirmationModal {...confirm.modalProps} />
 * ```
 */
export function useConfirmation(options: {
  onConfirm: () => void | Promise<void>;
  onCancel?: () => void;
}) {
  const modal = useModal();
  const [isLoading, setIsLoading] = React.useState(false);

  const trigger = React.useCallback(() => {
    modal.open();
  }, [modal]);

  const handleConfirm = React.useCallback(async () => {
    setIsLoading(true);
    try {
      await options.onConfirm();
      modal.close();
    } finally {
      setIsLoading(false);
    }
  }, [options, modal]);

  const handleCancel = React.useCallback(() => {
    options.onCancel?.();
    modal.close();
  }, [options, modal]);

  return {
    trigger,
    isLoading,
    modalProps: {
      open: modal.isOpen,
      onOpenChange: modal.onOpenChange,
      onConfirm: handleConfirm,
      onCancel: handleCancel,
      isLoading,
    },
  };
}
