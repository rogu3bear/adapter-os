// Chat session management hook
// 【2025-01-20†rectification†chat_session_hook】

import { useState, useEffect, useCallback } from 'react';
import { getStorageKey, serializeSession, deserializeSession, type ChatSession } from '@/types/chat';
import type { ChatMessage } from '@/components/chat/ChatMessage';
import { logger } from '@/utils/logger';

export function useChatSessions(tenantId: string) {
  const [sessions, setSessions] = useState<ChatSession[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  // Load sessions from localStorage on mount
  useEffect(() => {
    try {
      const storageKey = getStorageKey(tenantId);
      const stored = localStorage.getItem(storageKey);
      if (stored) {
        const parsed = JSON.parse(stored);
        const loadedSessions = Array.isArray(parsed)
          ? parsed.map(deserializeSession)
          : [];
        setSessions(loadedSessions);
      }
    } catch (error) {
      logger.error('Failed to load chat sessions', {
        component: 'useChatSessions',
        tenantId,
      }, error as Error);
    } finally {
      setIsLoading(false);
    }
  }, [tenantId]);

  // Save sessions to localStorage whenever they change
  useEffect(() => {
    if (!isLoading && sessions.length > 0) {
      try {
        const storageKey = getStorageKey(tenantId);
        const serialized = sessions.map(serializeSession);
        localStorage.setItem(storageKey, JSON.stringify(serialized));
      } catch (error) {
        // Handle storage quota exceeded or other storage errors gracefully
        const storageError = error as Error & { name?: string };
        if (storageError.name === 'QuotaExceededError') {
          // Try to free up space by removing oldest sessions
          const storageKey = getStorageKey(tenantId);
          const currentSessions = [...sessions];
          
          // Remove oldest 25% of sessions
          const sessionsToKeep = Math.floor(currentSessions.length * 0.75);
          const trimmedSessions = currentSessions
            .sort((a, b) => b.updatedAt.getTime() - a.updatedAt.getTime())
            .slice(0, sessionsToKeep);
          
          try {
            const serialized = trimmedSessions.map(serializeSession);
            localStorage.setItem(storageKey, JSON.stringify(serialized));
            logger.warn('Chat session storage quota exceeded. Removed oldest sessions.', {
              component: 'useChatSessions',
              tenantId,
              originalCount: sessions.length,
              keptCount: trimmedSessions.length,
            });
            // Update state to reflect trimmed sessions
            setSessions(trimmedSessions);
          } catch (retryError) {
            // If still failing, clear all sessions
            logger.error('Failed to save trimmed sessions. Clearing all sessions.', {
              component: 'useChatSessions',
              tenantId,
            }, retryError as Error);
            localStorage.removeItem(storageKey);
            setSessions([]);
          }
        } else {
          logger.error('Failed to save chat sessions', {
            component: 'useChatSessions',
            tenantId,
          }, storageError);
        }
      }
    }
  }, [sessions, tenantId, isLoading]);

  const createSession = useCallback((name: string, stackId: string, stackName?: string): ChatSession => {
    const newSession: ChatSession = {
      id: `session-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      name,
      stackId,
      stackName,
      messages: [],
      createdAt: new Date(),
      updatedAt: new Date(),
      tenantId,
    };
    setSessions(prev => [newSession, ...prev]);
    return newSession;
  }, [tenantId]);

  const updateSession = useCallback((sessionId: string, updates: Partial<ChatSession>) => {
    setSessions(prev =>
      prev.map(session =>
        session.id === sessionId
          ? { ...session, ...updates, updatedAt: new Date() }
          : session
      )
    );
  }, []);

  const addMessage = useCallback((sessionId: string, message: ChatMessage) => {
    setSessions(prev =>
      prev.map(session =>
        session.id === sessionId
          ? {
              ...session,
              messages: [...session.messages, message],
              updatedAt: new Date(),
            }
          : session
      )
    );
  }, []);

  const updateMessage = useCallback((sessionId: string, messageId: string, updates: Partial<ChatMessage>) => {
    setSessions(prev =>
      prev.map(session =>
        session.id === sessionId
          ? {
              ...session,
              messages: session.messages.map(msg =>
                msg.id === messageId ? { ...msg, ...updates } : msg
              ),
              updatedAt: new Date(),
            }
          : session
      )
    );
  }, []);

  const deleteSession = useCallback((sessionId: string) => {
    setSessions(prev => prev.filter(session => session.id !== sessionId));
  }, []);

  const getSession = useCallback((sessionId: string): ChatSession | undefined => {
    return sessions.find(s => s.id === sessionId);
  }, [sessions]);

  return {
    sessions,
    isLoading,
    createSession,
    updateSession,
    addMessage,
    updateMessage,
    deleteSession,
    getSession,
  };
}

