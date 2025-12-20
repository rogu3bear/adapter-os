import { useState, useCallback } from 'react';

export type ChatModalType = 'history' | 'routerActivity' | 'archive' | 'share' | 'tags' | 'category';

interface ChatModalState {
  active: ChatModalType | null;
  data: {
    sessionId?: string;
  } | null;
}

export function useChatModals() {
  const [state, setState] = useState<ChatModalState>({
    active: null,
    data: null,
  });

  const openModal = useCallback((type: ChatModalType, data?: { sessionId?: string }) => {
    setState({
      active: type,
      data: data ?? null,
    });
  }, []);

  const closeModal = useCallback(() => {
    setState({
      active: null,
      data: null,
    });
  }, []);

  const isOpen = useCallback(
    (type: ChatModalType) => state.active === type,
    [state.active]
  );

  const getModalData = useCallback(() => state.data, [state.data]);

  // Specific modal state getters for backward compatibility
  const isHistoryOpen = state.active === 'history';
  const isRouterActivityOpen = state.active === 'routerActivity';
  const isArchivePanelOpen = state.active === 'archive';
  const shareDialogSessionId = state.active === 'share' ? state.data?.sessionId ?? null : null;
  const tagsDialogSessionId = state.active === 'tags' ? state.data?.sessionId ?? null : null;
  const categoryDialogSessionId = state.active === 'category' ? state.data?.sessionId ?? null : null;

  return {
    // Core API
    openModal,
    closeModal,
    isOpen,
    getModalData,

    // Specific modal states (for direct access in components)
    isHistoryOpen,
    isRouterActivityOpen,
    isArchivePanelOpen,
    shareDialogSessionId,
    tagsDialogSessionId,
    categoryDialogSessionId,

    // Setters for backward compatibility
    setIsHistoryOpen: (open: boolean) => {
      if (open) {
        openModal('history');
      } else if (state.active === 'history') {
        closeModal();
      }
    },
    setIsRouterActivityOpen: (open: boolean) => {
      if (open) {
        openModal('routerActivity');
      } else if (state.active === 'routerActivity') {
        closeModal();
      }
    },
    setIsArchivePanelOpen: (open: boolean) => {
      if (open) {
        openModal('archive');
      } else if (state.active === 'archive') {
        closeModal();
      }
    },
    setShareDialogSessionId: (sessionId: string | null) => {
      if (sessionId) {
        openModal('share', { sessionId });
      } else if (state.active === 'share') {
        closeModal();
      }
    },
    setTagsDialogSessionId: (sessionId: string | null) => {
      if (sessionId) {
        openModal('tags', { sessionId });
      } else if (state.active === 'tags') {
        closeModal();
      }
    },
    setCategoryDialogSessionId: (sessionId: string | null) => {
      if (sessionId) {
        openModal('category', { sessionId });
      } else if (state.active === 'category') {
        closeModal();
      }
    },
  };
}
