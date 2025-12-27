import { createContext, useContext, useMemo, useState, useCallback, type ReactNode } from 'react';
import { logger } from '@/utils/logger';

export interface SuggestedAdapter {
  id: string;
  confidence: number;
  reason?: string;
  keywords?: string[];
  source?: 'mock-router' | 'manual' | 'router';
}

export interface AttachedAdapter extends SuggestedAdapter {
  attachedBy: 'auto' | 'manual';
  attachedAt: number;
}

interface ChatContextValue {
  suggestedAdapters: SuggestedAdapter[];
  attachedAdapters: AttachedAdapter[];
  setSuggestedAdapters: (adapters: SuggestedAdapter[]) => void;
  attachAdapter: (adapter: SuggestedAdapter, attachedBy?: 'auto' | 'manual') => void;
  removeAttachedAdapter: (adapterId: string) => void;
  lastAttachedAdapterId: string | null;
  autoAttachPaused: boolean;
  pauseAutoAttach: () => void;
  resumeAutoAttach: () => void;
  mutedAdapterIds: Set<string>;
  muteAdapter: (adapterId: string) => void;
  reset: () => void;
  setActiveSessionId: (sessionId: string | null) => void;
}

const ChatContext = createContext<ChatContextValue | null>(null);

export function ChatProvider({ children }: { children: ReactNode }) {
  const [suggestedAdapters, setSuggestedAdaptersState] = useState<SuggestedAdapter[]>([]);
  const [attachedAdapters, setAttachedAdapters] = useState<AttachedAdapter[]>([]);
  const [lastAttachedAdapterId, setLastAttachedAdapterId] = useState<string | null>(null);
  const [autoAttachPaused, setAutoAttachPaused] = useState(false);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [mutedBySession, setMutedBySession] = useState<Record<string, Set<string>>>({});

  const setSuggestedAdapters = useCallback((adapters: SuggestedAdapter[]) => {
    setSuggestedAdaptersState(adapters);
  }, []);

  const reset = useCallback(() => {
    setSuggestedAdaptersState([]);
    setAttachedAdapters([]);
    setLastAttachedAdapterId(null);
    setAutoAttachPaused(false);
  }, []);

  const attachAdapter = useCallback((adapter: SuggestedAdapter, attachedBy: 'auto' | 'manual' = 'manual') => {
    setAttachedAdapters((prev) => {
      if (prev.some((existing) => existing.id === adapter.id)) {
        return prev;
      }
      const next: AttachedAdapter = {
        ...adapter,
        attachedBy,
        attachedAt: Date.now(),
      };
      setLastAttachedAdapterId(adapter.id);
      logger.debug('Adapter attached', {
        component: 'ChatContext',
        adapterId: adapter.id,
        confidence: adapter.confidence,
        attachedBy,
      });
      return [...prev, next];
    });
  }, []);

  const removeAttachedAdapter = useCallback((adapterId: string) => {
    setAttachedAdapters((prev) => prev.filter((adapter) => adapter.id !== adapterId));
  }, []);

  const pauseAutoAttach = useCallback(() => {
    setAutoAttachPaused(true);
  }, []);

  const resumeAutoAttach = useCallback(() => {
    setAutoAttachPaused(false);
  }, []);

  const sessionKey = activeSessionId ?? 'global';
  const mutedAdapterIds = useMemo(() => mutedBySession[sessionKey] ?? new Set<string>(), [mutedBySession, sessionKey]);

  const muteAdapter = useCallback((adapterId: string) => {
    setMutedBySession((prev) => {
      const next = { ...prev };
      const existing = prev[sessionKey] ? new Set(prev[sessionKey]) : new Set<string>();
      existing.add(adapterId);
      next[sessionKey] = existing;
      return next;
    });
  }, [sessionKey]);

  const updateActiveSessionId = useCallback((sessionId: string | null) => {
    setActiveSessionId(sessionId);
  }, []);

  const value = useMemo<ChatContextValue>(() => ({
    suggestedAdapters,
    attachedAdapters,
    setSuggestedAdapters,
    attachAdapter,
    removeAttachedAdapter,
    lastAttachedAdapterId,
    autoAttachPaused,
    pauseAutoAttach,
    resumeAutoAttach,
    mutedAdapterIds,
    muteAdapter,
    reset,
    setActiveSessionId: updateActiveSessionId,
  }), [
    attachAdapter,
    attachedAdapters,
    autoAttachPaused,
    lastAttachedAdapterId,
    muteAdapter,
    mutedAdapterIds,
    pauseAutoAttach,
    removeAttachedAdapter,
    reset,
    resumeAutoAttach,
    setSuggestedAdapters,
    suggestedAdapters,
    updateActiveSessionId,
  ]);

  return (
    <ChatContext.Provider value={value}>
      {children}
    </ChatContext.Provider>
  );
}

export function useChatContext(): ChatContextValue {
  const context = useContext(ChatContext);
  if (!context) {
    throw new Error('useChatContext must be used within a ChatProvider');
  }
  return context;
}

export function useChatContextOptional(): ChatContextValue | null {
  return useContext(ChatContext);
}

export default ChatContext;
