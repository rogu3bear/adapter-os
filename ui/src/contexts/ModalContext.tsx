import React, { createContext, useContext, useState, useCallback, ReactNode } from 'react';

interface ModalContextValue {
  openModal: (modalId: string) => void;
  closeModal: () => void;
  isOpen: (modalId: string) => boolean;
  activeModal: string | null;
}

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

export function useModalManager(): ModalContextValue {
  const context = useContext(ModalContext);
  if (!context) {
    throw new Error('useModalManager must be used within ModalProvider');
  }
  return context;
}
