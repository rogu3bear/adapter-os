import { createContext, useContext, useState, useCallback, ReactNode } from 'react';

interface ModalContextValue {
  openModal: (modalId: string) => void;
  closeModal: () => void;
  isOpen: (modalId: string) => boolean;
  activeModal: string | null;
}

/**
 * @deprecated Use useDialogManager from '@/hooks/useDialogManager' instead.
 * This context will be removed in v2.0.
 *
 * Migration example:
 * ```typescript
 * // Before:
 * const { openModal, closeModal, isOpen } = useModalManager();
 *
 * // After:
 * import { createDialogManager } from '@/hooks/useDialogManager';
 * const useMyDialogs = createDialogManager(...);
 * const dialogs = useMyDialogs();
 * dialogs.openDialog('myDialog', data);
 * ```
 */
const ModalContext = createContext<ModalContextValue | undefined>(undefined);

interface ModalProviderProps {
  children: ReactNode;
}

export function ModalProvider({ children }: ModalProviderProps) {
  const [activeModal, setActiveModal] = useState<string | null>(null);

  const openModal = useCallback((modalId: string) => {
    setActiveModal(modalId);
  }, []);

  const closeModal = useCallback(() => {
    setActiveModal(null);
  }, []);

  const isOpen = useCallback((modalId: string) => {
    return activeModal === modalId;
  }, [activeModal]);

  const value: ModalContextValue = {
    openModal,
    closeModal,
    isOpen,
    activeModal
  };

  return (
    <ModalContext.Provider value={value}>
      {children}
    </ModalContext.Provider>
  );
}

/**
 * @deprecated Use useDialogManager from '@/hooks/useDialogManager' instead.
 * This hook will be removed in v2.0.
 *
 * @see {@link import('@/hooks/useDialogManager').createDialogManager}
 */
export function useModalManager(): ModalContextValue {
  const context = useContext(ModalContext);
  if (!context) {
    throw new Error('useModalManager must be used within ModalProvider');
  }

  // Log deprecation warning in development
  if (process.env.NODE_ENV === 'development') {
    console.warn(
      '[DEPRECATED] useModalManager from ModalContext is deprecated. ' +
      'Use createDialogManager from @/hooks/useDialogManager instead. ' +
      'See documentation for migration examples.'
    );
  }

  return context;
}
