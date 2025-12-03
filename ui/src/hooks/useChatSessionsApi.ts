// Backend-aware chat session management hook with localStorage migration
// 【2025-11-25†prd-ux-01†chat_sessions_api_hook】
//
// This hook maintains the SAME interface as useChatSessions but uses the backend API.
// On first load, it automatically migrates localStorage sessions to the backend.

import { useState, useEffect, useCallback, useRef } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { getStorageKey, type ChatSession as LocalChatSession } from '@/types/chat';
import type { ChatMessage as LocalChatMessage } from '@/components/chat/ChatMessage';
import { logger } from '@/utils/logger';
import apiClient from '@/api/client';
import type { ChatSession, ChatMessage, CreateChatSessionRequest } from '@/api/chat-types';
import { toast } from 'sonner';

const MIGRATION_KEY_PREFIX = 'chat_sessions_migrated_';
const SESSION_QUERY_KEY = 'chat-sessions';

/**
 * Convert backend ChatSession to local ChatSession format
 */
function toLocalSession(backendSession: ChatSession, messages: ChatMessage[]): LocalChatSession {
  return {
    id: backendSession.id,
    name: backendSession.name,
    stackId: backendSession.stack_id || '',
    stackName: undefined, // Backend doesn't store stack name
    messages: messages.map(toLocalMessage),
    createdAt: new Date(backendSession.created_at),
    updatedAt: new Date(backendSession.last_activity_at),
    tenantId: backendSession.tenant_id,
  };
}

/**
 * Convert backend ChatMessage to local ChatMessage format
 */
function toLocalMessage(backendMessage: ChatMessage): LocalChatMessage {
  const metadata = backendMessage.metadata_json
    ? JSON.parse(backendMessage.metadata_json)
    : undefined;

  return {
    id: backendMessage.id,
    role: backendMessage.role as 'user' | 'assistant',
    content: backendMessage.content,
    timestamp: new Date(backendMessage.timestamp),
    routerDecision: metadata?.routerDecision,
    unavailablePinnedAdapters: metadata?.unavailablePinnedAdapters,
    pinnedRoutingFallback: metadata?.pinnedRoutingFallback,
  };
}

/**
 * Check if migration has been completed for this tenant
 */
function isMigrationComplete(tenantId: string): boolean {
  const migrationKey = `${MIGRATION_KEY_PREFIX}${tenantId}`;
  return localStorage.getItem(migrationKey) === 'true';
}

/**
 * Mark migration as complete for this tenant
 */
function setMigrationComplete(tenantId: string): void {
  const migrationKey = `${MIGRATION_KEY_PREFIX}${tenantId}`;
  localStorage.setItem(migrationKey, 'true');
}

/**
 * Migrate localStorage sessions to backend
 */
async function migrateLocalStorageSessions(tenantId: string): Promise<number> {
  try {
    const storageKey = getStorageKey(tenantId);
    const stored = localStorage.getItem(storageKey);
    if (!stored) {
      logger.info('No localStorage sessions to migrate', {
        component: 'useChatSessionsApi',
        operation: 'migrateLocalStorageSessions',
        tenantId,
      });
      return 0;
    }

    const parsed = JSON.parse(stored);
    const sessions: LocalChatSession[] = Array.isArray(parsed) ? parsed : [];

    if (sessions.length === 0) {
      logger.info('No sessions to migrate', {
        component: 'useChatSessionsApi',
        operation: 'migrateLocalStorageSessions',
        tenantId,
      });
      return 0;
    }

    logger.info('Starting migration of localStorage sessions', {
      component: 'useChatSessionsApi',
      operation: 'migrateLocalStorageSessions',
      tenantId,
      sessionCount: sessions.length,
    });

    let migratedCount = 0;
    for (const session of sessions) {
      try {
        // Deserialize dates if they're strings
        const createdAt = typeof session.createdAt === 'string'
          ? new Date(session.createdAt)
          : session.createdAt;
        const updatedAt = typeof session.updatedAt === 'string'
          ? new Date(session.updatedAt)
          : session.updatedAt;

        // Create session on backend
        const req: CreateChatSessionRequest = {
          name: session.name,
          stack_id: session.stackId || undefined,
          metadata: {
            migratedFrom: 'localStorage',
            originalCreatedAt: createdAt.toISOString(),
            stackName: session.stackName,
          },
        };

        const response = await apiClient.createChatSession(req);

        // Migrate messages
        const messages = Array.isArray(session.messages) ? session.messages : [];
        for (const msg of messages) {
          const msgTimestamp = typeof msg.timestamp === 'string'
            ? new Date(msg.timestamp)
            : msg.timestamp;

          await apiClient.addChatMessage(
            response.session_id,
            msg.role,
            msg.content,
            {
              originalTimestamp: msgTimestamp.toISOString(),
              routerDecision: msg.routerDecision,
              unavailablePinnedAdapters: msg.unavailablePinnedAdapters,
              pinnedRoutingFallback: msg.pinnedRoutingFallback,
            }
          );
        }

        migratedCount++;
        logger.info('Migrated session', {
          component: 'useChatSessionsApi',
          operation: 'migrateLocalStorageSessions',
          sessionId: session.id,
          newSessionId: response.session_id,
          messageCount: messages.length,
        });
      } catch (error) {
        logger.error('Failed to migrate session', {
          component: 'useChatSessionsApi',
          operation: 'migrateLocalStorageSessions',
          sessionId: session.id,
        }, error as Error);
      }
    }

    // Clear localStorage after successful migration
    localStorage.removeItem(storageKey);
    setMigrationComplete(tenantId);

    logger.info('Migration complete', {
      component: 'useChatSessionsApi',
      operation: 'migrateLocalStorageSessions',
      tenantId,
      migratedCount,
      totalCount: sessions.length,
    });

    return migratedCount;
  } catch (error) {
    logger.error('Migration failed', {
      component: 'useChatSessionsApi',
      operation: 'migrateLocalStorageSessions',
      tenantId,
    }, error as Error);
    throw error;
  }
}

export function useChatSessionsApi(tenantId: string) {
  const queryClient = useQueryClient();
  const [isLoadingInitial, setIsLoadingInitial] = useState(true);
  const [sessions, setSessions] = useState<LocalChatSession[]>([]);
  const migrationAttempted = useRef(false);

  // Fetch sessions from backend
  const { data: backendSessions = [], isLoading: isLoadingSessions } = useQuery({
    queryKey: [SESSION_QUERY_KEY, tenantId],
    queryFn: async () => {
      try {
        return await apiClient.listChatSessions({ limit: 100 });
      } catch (error) {
        logger.error('Failed to fetch sessions from backend', {
          component: 'useChatSessionsApi',
          operation: 'fetchSessions',
          tenantId,
        }, error as Error);
        throw error; // Let React Query handle error state
      }
    },
    staleTime: 30000, // 30 seconds
    refetchOnWindowFocus: true,
  });

  // One-time migration on mount
  useEffect(() => {
    if (migrationAttempted.current || isLoadingSessions) {
      return;
    }

    migrationAttempted.current = true;

    const performMigration = async () => {
      if (isMigrationComplete(tenantId)) {
        logger.info('Migration already complete', {
          component: 'useChatSessionsApi',
          operation: 'performMigration',
          tenantId,
        });
        setIsLoadingInitial(false);
        return;
      }

      try {
        const migratedCount = await migrateLocalStorageSessions(tenantId);
        if (migratedCount > 0) {
          toast.success(`Migrated ${migratedCount} chat session${migratedCount > 1 ? 's' : ''} to backend storage`);
          // Refetch sessions after migration
          queryClient.invalidateQueries({ queryKey: [SESSION_QUERY_KEY, tenantId] });
        }
      } catch (error) {
        logger.error('Migration error', {
          component: 'useChatSessionsApi',
          operation: 'performMigration',
          tenantId,
        }, error as Error);
        toast.error('Failed to migrate chat sessions. Your sessions are safe in local storage.');
      } finally {
        setIsLoadingInitial(false);
      }
    };

    performMigration();
  }, [tenantId, queryClient, isLoadingSessions]);

  // Convert backend sessions to local format and fetch messages
  useEffect(() => {
    const loadSessionsWithMessages = async () => {
      if (isLoadingSessions || backendSessions.length === 0) {
        setSessions([]);
        return;
      }

      try {
        const sessionsWithMessages = await Promise.all(
          backendSessions.map(async (backendSession) => {
            try {
              const messages = await apiClient.getChatMessages(backendSession.id);
              return toLocalSession(backendSession, messages);
            } catch (error) {
              logger.error('Failed to fetch messages for session', {
                component: 'useChatSessionsApi',
                operation: 'loadSessionsWithMessages',
                sessionId: backendSession.id,
              }, error as Error);
              return toLocalSession(backendSession, []);
            }
          })
        );
        setSessions(sessionsWithMessages);
      } catch (error) {
        logger.error('Failed to load sessions with messages', {
          component: 'useChatSessionsApi',
          operation: 'loadSessionsWithMessages',
          tenantId,
        }, error as Error);
      }
    };

    loadSessionsWithMessages();
  }, [backendSessions, isLoadingSessions, tenantId]);

  // Create session mutation
  const createSessionMutation = useMutation({
    mutationFn: async (params: { name: string; stackId: string; stackName?: string; collectionId?: string }) => {
      const req: CreateChatSessionRequest = {
        name: params.name,
        stack_id: params.stackId || undefined,
        collection_id: params.collectionId || undefined,
        metadata: params.stackName ? { stackName: params.stackName } : undefined,
      };
      return await apiClient.createChatSession(req);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: [SESSION_QUERY_KEY, tenantId] });
    },
  });

  // Delete session mutation
  const deleteSessionMutation = useMutation({
    mutationFn: async (sessionId: string) => {
      await apiClient.deleteChatSession(sessionId);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: [SESSION_QUERY_KEY, tenantId] });
    },
  });

  // Update session collection mutation
  const updateSessionCollectionMutation = useMutation({
    mutationFn: async (params: { sessionId: string; collectionId: string | null }) => {
      const url = `/v1/chat/sessions/${encodeURIComponent(params.sessionId)}/collection`;
      return await apiClient.request<void>(url, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ collection_id: params.collectionId }),
      });
    },
    onSuccess: (_, variables) => {
      // Invalidate session queries to refetch updated session
      queryClient.invalidateQueries({ queryKey: [SESSION_QUERY_KEY, tenantId] });
      logger.info('Session collection updated', {
        component: 'useChatSessionsApi',
        operation: 'updateSessionCollection',
        sessionId: variables.sessionId,
        collectionId: variables.collectionId,
      });
    },
    onError: (error, variables) => {
      logger.error('Failed to update session collection', {
        component: 'useChatSessionsApi',
        operation: 'updateSessionCollection',
        sessionId: variables.sessionId,
      }, error as Error);
      toast.error('Failed to update session collection');
    },
  });

  // Add message mutation
  const addMessageMutation = useMutation({
    mutationFn: async (params: { sessionId: string; message: LocalChatMessage }) => {
      const metadata = {
        routerDecision: params.message.routerDecision,
        unavailablePinnedAdapters: params.message.unavailablePinnedAdapters,
        pinnedRoutingFallback: params.message.pinnedRoutingFallback,
      };
      return await apiClient.addChatMessage(
        params.sessionId,
        params.message.role,
        params.message.content,
        metadata
      );
    },
    onSuccess: (_, variables) => {
      // Optimistically update local state
      setSessions((prev) =>
        prev.map((session) =>
          session.id === variables.sessionId
            ? {
                ...session,
                messages: [...session.messages, variables.message],
                updatedAt: new Date(),
              }
            : session
        )
      );
    },
  });

  const createSession = useCallback(
    (name: string, stackId: string, stackName?: string, collectionId?: string): LocalChatSession => {
      // Create optimistic local session
      const newSession: LocalChatSession = {
        id: `temp-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
        name,
        stackId,
        stackName,
        messages: [],
        createdAt: new Date(),
        updatedAt: new Date(),
        tenantId,
      };
      setSessions((prev) => [newSession, ...prev]);

      // Trigger backend creation
      createSessionMutation.mutate(
        { name, stackId, stackName, collectionId },
        {
          onSuccess: (response) => {
            // Replace temp session with backend session
            setSessions((prev) =>
              prev.map((session) =>
                session.id === newSession.id
                  ? { ...session, id: response.session_id }
                  : session
              )
            );
          },
          onError: (error) => {
            logger.error('Failed to create session on backend', {
              component: 'useChatSessionsApi',
              operation: 'createSession',
            }, error as Error);
            // Remove optimistic session
            setSessions((prev) => prev.filter((session) => session.id !== newSession.id));
            toast.error('Failed to create chat session');
          },
        }
      );

      return newSession;
    },
    [tenantId, createSessionMutation]
  );

  const updateSession = useCallback((sessionId: string, updates: Partial<LocalChatSession>) => {
    setSessions((prev) =>
      prev.map((session) =>
        session.id === sessionId
          ? { ...session, ...updates, updatedAt: new Date() }
          : session
      )
    );
    // Note: Backend doesn't have an update session endpoint yet
    // This only updates local state for now
  }, []);

  const addMessage = useCallback(
    (sessionId: string, message: LocalChatMessage) => {
      addMessageMutation.mutate(
        { sessionId, message },
        {
          onError: (error) => {
            logger.error('Failed to add message to backend', {
              component: 'useChatSessionsApi',
              operation: 'addMessage',
              sessionId,
            }, error as Error);
            toast.error('Failed to save message');
          },
        }
      );
    },
    [addMessageMutation]
  );

  const updateMessage = useCallback(
    (sessionId: string, messageId: string, updates: Partial<LocalChatMessage>) => {
      setSessions((prev) =>
        prev.map((session) =>
          session.id === sessionId
            ? {
                ...session,
                messages: session.messages.map((msg) =>
                  msg.id === messageId ? { ...msg, ...updates } : msg
                ),
                updatedAt: new Date(),
              }
            : session
        )
      );
      // Note: Backend doesn't have an update message endpoint yet
      // This only updates local state for now
    },
    []
  );

  const deleteSession = useCallback(
    (sessionId: string) => {
      // Optimistically remove from local state
      setSessions((prev) => prev.filter((session) => session.id !== sessionId));

      // Trigger backend deletion
      deleteSessionMutation.mutate(sessionId, {
        onError: (error) => {
          logger.error('Failed to delete session on backend', {
            component: 'useChatSessionsApi',
            operation: 'deleteSession',
            sessionId,
          }, error as Error);
          toast.error('Failed to delete session');
          // Re-fetch to restore session
          queryClient.invalidateQueries({ queryKey: [SESSION_QUERY_KEY, tenantId] });
        },
      });
    },
    [tenantId, deleteSessionMutation, queryClient]
  );

  const getSession = useCallback(
    (sessionId: string): LocalChatSession | undefined => {
      return sessions.find((s) => s.id === sessionId);
    },
    [sessions]
  );

  const updateSessionCollection = useCallback(
    (sessionId: string, collectionId: string | null) => {
      updateSessionCollectionMutation.mutate(
        { sessionId, collectionId },
        {
          onSuccess: () => {
            logger.info('Session collection updated successfully', {
              component: 'useChatSessionsApi',
              operation: 'updateSessionCollection',
              sessionId,
              collectionId,
            });
            toast.success('Session collection updated');
          },
        }
      );
    },
    [updateSessionCollectionMutation]
  );

  return {
    sessions,
    isLoading: isLoadingInitial || isLoadingSessions,
    createSession,
    updateSession,
    addMessage,
    updateMessage,
    deleteSession,
    getSession,
    updateSessionCollection,
  };
}
