//! Real-time workspace messaging hook
//!
//! Provides live message updates for workspace communication.
//! Workspace-scoped messaging with SSE support.
//!
//! Citation: ui/src/hooks/useActivityFeed.ts L199-L273 (SSE connection pattern)
//! - Real-time message updates per workspace
//! - Workspace-scoped only (no direct tenant-to-tenant messaging)

import { useState, useEffect, useRef, useCallback } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { logger, toError } from '@/utils/logger';
import apiClient from '@/api/client';
import { Message, CreateMessageRequest } from '@/api/types';

// Query keys for React Query cache management
const QUERY_KEYS = {
  messages: (workspaceId: string) => ['messages', workspaceId] as const,
  messageThread: (workspaceId: string, threadId: string) => ['messages', workspaceId, 'thread', threadId] as const,
};

export interface UseMessagesOptions {
  workspaceId: string;
  enabled?: boolean;
  maxMessages?: number;
  useSSE?: boolean;
}

export interface UseMessagesReturn {
  messages: Message[];
  isLoading: boolean;
  error: Error | null;
  sendMessage: (content: string, threadId?: string) => Promise<Message>;
  editMessage: (messageId: string, content: string) => Promise<Message>;
  getThread: (threadId: string) => Promise<Message[]>;
  refetch: () => Promise<void>;
}

/**
 * Hook for workspace messaging
 *
 * # Arguments
 *
 * * `options` - Configuration options for messaging
 *   * `workspaceId` - Required workspace identifier
 *   * `enabled` - Whether to enable the hook (default: true)
 *   * `maxMessages` - Maximum messages to fetch (default: 50)
 *   * `useSSE` - Whether to use SSE for real-time updates (default: true)
 *
 * # Returns
 *
 * * `messages` - Array of message objects
 * * `loading` - Loading state
 * * `error` - Error message if any
 * * `sendMessage` - Function to send a new message
 * * `editMessage` - Function to edit an existing message
 * * `getThread` - Function to get message thread
 * * `refetch` - Function to manually refresh messages
 *
 * # Policy Compliance
 *
 * - Policy Pack #9 (Telemetry): Uses canonical JSON structure
 * - Policy Pack #1 (Egress): Uses relative API paths only
 * - Workspace-scoped: Only allows messaging within shared workspaces
 */
export function useMessages(options: UseMessagesOptions): UseMessagesReturn {
  const { workspaceId, enabled = true, maxMessages = 50, useSSE = true } = options;

  const [messages, setMessages] = useState<Message[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const sseRef = useRef<(() => void) | null>(null);
  const fallbackIntervalRef = useRef<NodeJS.Timeout | null>(null);
  const baselineIntervalRef = useRef<NodeJS.Timeout | null>(null);
  const isMountedRef = useRef(true);
  
  // Store latest values in refs to avoid recreating callbacks
  const enabledRef = useRef(enabled);
  const maxMessagesRef = useRef(maxMessages);
  const workspaceIdRef = useRef(workspaceId);
  
  useEffect(() => {
    enabledRef.current = enabled;
    maxMessagesRef.current = maxMessages;
    workspaceIdRef.current = workspaceId;
  }, [enabled, maxMessages, workspaceId]);

  const fetchMessages = useCallback(async () => {
    if (!enabledRef.current || !workspaceIdRef.current || !isMountedRef.current) return;

    setLoading(true);
    setError(null);

    try {
      const messagesResponse = await apiClient.listWorkspaceMessages(workspaceIdRef.current, {
        limit: maxMessagesRef.current,
      });

      if (!isMountedRef.current) return;

      // Sort by created_at (newest first for display)
      const sortedMessages = messagesResponse.sort(
        (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
      );

      setMessages(sortedMessages);

      logger.info('Messages updated', {
        component: 'useMessages',
        operation: 'fetchMessages',
        messageCount: sortedMessages.length,
        workspaceId: workspaceIdRef.current,
      });
    } catch (err) {
      if (!isMountedRef.current) return;
      
      setError(err instanceof Error ? err : new Error('Failed to fetch messages'));

      logger.error('Failed to fetch messages', {
        component: 'useMessages',
        operation: 'fetchMessages',
        workspaceId: workspaceIdRef.current,
      }, err instanceof Error ? err : new Error(String(err)));
    } finally {
      if (isMountedRef.current) {
        setLoading(false);
      }
    }
  }, []); // Empty deps - use refs for values

  const sendMessage = useCallback(async (content: string, threadId?: string): Promise<Message> => {
    try {
      const messageData: CreateMessageRequest = { content };
      if (threadId) {
        messageData.thread_id = threadId;
      }

      const newMessage = await apiClient.createMessage(workspaceId, messageData);

      // Add to local state
      setMessages(prev => [newMessage, ...prev]);

      logger.info('Message sent', {
        component: 'useMessages',
        operation: 'sendMessage',
        messageId: newMessage.id,
        threadId,
        workspaceId,
      });

      return newMessage;
    } catch (err) {
      logger.error('Failed to send message', {
        component: 'useMessages',
        operation: 'sendMessage',
        workspaceId,
        threadId,
      }, toError(err));
      throw err;
    }
  }, [workspaceId]);

  const editMessage = useCallback(async (messageId: string, content: string): Promise<Message> => {
    try {
      const updatedMessage = await apiClient.editMessage(workspaceId, messageId, { content });

      // Update local state
      setMessages(prev =>
        prev.map(m => m.id === messageId ? updatedMessage : m)
      );

      logger.info('Message edited', {
        component: 'useMessages',
        operation: 'editMessage',
        messageId,
        workspaceId,
      });

      return updatedMessage;
    } catch (err) {
      logger.error('Failed to edit message', {
        component: 'useMessages',
        operation: 'editMessage',
        messageId,
        workspaceId,
      }, toError(err));
      throw err;
    }
  }, [workspaceId]);

  const getThread = useCallback(async (threadId: string): Promise<Message[]> => {
    try {
      const threadMessages = await apiClient.getMessageThread(workspaceId, threadId);

      logger.info('Thread retrieved', {
        component: 'useMessages',
        operation: 'getThread',
        threadId,
        messageCount: threadMessages.length,
        workspaceId,
      });

      return threadMessages;
    } catch (err) {
      logger.error('Failed to get thread', {
        component: 'useMessages',
        operation: 'getThread',
        threadId,
        workspaceId,
      }, toError(err));
      throw err;
    }
  }, [workspaceId]);

  useEffect(() => {
    if (!workspaceId || !enabled) {
      // Clean up everything if disabled or no workspace
      if (baselineIntervalRef.current) {
        clearInterval(baselineIntervalRef.current);
        baselineIntervalRef.current = null;
      }
      if (fallbackIntervalRef.current) {
        clearInterval(fallbackIntervalRef.current);
        fallbackIntervalRef.current = null;
      }
      if (sseRef.current) {
        sseRef.current();
        sseRef.current = null;
      }
      return;
    }

    isMountedRef.current = true;
    
    // Clean up any existing resources first
    if (baselineIntervalRef.current) {
      clearInterval(baselineIntervalRef.current);
      baselineIntervalRef.current = null;
    }
    if (fallbackIntervalRef.current) {
      clearInterval(fallbackIntervalRef.current);
      fallbackIntervalRef.current = null;
    }
    if (sseRef.current) {
      sseRef.current();
      sseRef.current = null;
    }

    fetchMessages();

    // Baseline polling every 30s
    baselineIntervalRef.current = setInterval(() => {
      if (isMountedRef.current && enabledRef.current) {
        fetchMessages();
      }
    }, 30000);

    // SSE live updates + reconnect with fallback polling
    let reconnectAttempts = 0;
    const maxReconnect = 5;
    const baseDelay = 500;

    function clearFallback() {
      if (fallbackIntervalRef.current) {
        clearInterval(fallbackIntervalRef.current);
        fallbackIntervalRef.current = null;
      }
    }

    function startFallbackPolling() {
      clearFallback();
      // quick polling while disconnected
      fallbackIntervalRef.current = setInterval(() => {
        if (isMountedRef.current && enabledRef.current) {
          fetchMessages();
        }
      }, 500);
    }

    function stopSSE() {
      if (sseRef.current) {
        try {
          sseRef.current();
        } catch (e) {
          // Ignore cleanup errors
        }
        sseRef.current = null;
      }
    }

    function connectSSE() {
      if (!useSSE || !workspaceId || !isMountedRef.current) return;

      try {
        const unsubscribe = apiClient.subscribeToMessages(workspaceId, (data) => {
          if (!isMountedRef.current) {
            // Component unmounted, cleanup
            if (unsubscribe) unsubscribe();
            return;
          }
          
          if (data) {
            // Update messages from SSE
            const sortedMessages = data.messages.sort(
              (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
            );
            setMessages(sortedMessages);
            setError(null);
            reconnectAttempts = 0;
            clearFallback();
          } else {
            // SSE failed, start fallback polling
            startFallbackPolling();
          }
        });

        sseRef.current = unsubscribe;
      } catch (err) {
        logger.error('Failed to initialize messages SSE', {
          component: 'useMessages',
          operation: 'sse_init',
          workspaceId: workspaceIdRef.current,
        }, err instanceof Error ? err : new Error(String(err)));
        startFallbackPolling();
      }
    }

    connectSSE();

    return () => {
      isMountedRef.current = false;
      
      if (baselineIntervalRef.current) {
        clearInterval(baselineIntervalRef.current);
        baselineIntervalRef.current = null;
      }
      clearFallback();
      stopSSE();
    };
  }, [enabled, workspaceId, useSSE, fetchMessages]);

  return {
    messages,
    isLoading: loading,
    error,
    sendMessage,
    editMessage,
    getThread,
    refetch: fetchMessages,
  };
}

// ============================================================================
// React Query-based hooks for mutation operations with toast notifications
// ============================================================================

/**
 * React Query hook for fetching messages (alternative to SSE-based useMessages)
 *
 * Use this when you prefer React Query's caching and refetching over SSE real-time updates.
 *
 * @param workspaceId - The workspace to fetch messages from
 * @param options - Optional configuration
 * @returns React Query result with messages data
 */
export function useMessagesQuery(
  workspaceId: string,
  options?: { limit?: number; offset?: number; enabled?: boolean }
) {
  const { limit = 50, offset, enabled = true } = options ?? {};

  return useQuery({
    queryKey: QUERY_KEYS.messages(workspaceId),
    queryFn: async () => {
      const messages = await apiClient.listWorkspaceMessages(workspaceId, { limit, offset });
      // Sort by created_at (newest first)
      return messages.sort(
        (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
      );
    },
    enabled: !!workspaceId && enabled,
    staleTime: 30000,
  });
}

/**
 * React Query hook for fetching a message thread
 *
 * @param workspaceId - The workspace containing the thread
 * @param threadId - The thread ID to fetch
 * @returns React Query result with thread messages
 */
export function useMessageThread(workspaceId: string, threadId: string) {
  return useQuery({
    queryKey: QUERY_KEYS.messageThread(workspaceId, threadId),
    queryFn: () => apiClient.getMessageThread(workspaceId, threadId),
    enabled: !!workspaceId && !!threadId,
  });
}

/**
 * Mutation hook for sending messages with toast notifications
 *
 * @param workspaceId - The workspace to send the message to
 * @returns Mutation object with mutate/mutateAsync functions
 *
 * @example
 * const { mutate: sendMessage, isPending } = useSendMessage('workspace-123');
 * sendMessage({ content: 'Hello world!' });
 */
export function useSendMessage(workspaceId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (data: CreateMessageRequest) => {
      return apiClient.createMessage(workspaceId, data);
    },
    onSuccess: (newMessage) => {
      // Invalidate messages query to refetch
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.messages(workspaceId) });

      // If message is in a thread, invalidate thread query too
      if (newMessage.thread_id) {
        queryClient.invalidateQueries({
          queryKey: QUERY_KEYS.messageThread(workspaceId, newMessage.thread_id),
        });
      }

      toast.success('Message sent successfully');
      logger.info('Message sent', {
        component: 'useSendMessage',
        operation: 'sendMessage',
        messageId: newMessage.id,
        workspaceId,
        threadId: newMessage.thread_id,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to send message: ${error.message}`);
      logger.error('Failed to send message', {
        component: 'useSendMessage',
        operation: 'sendMessage',
        workspaceId,
      }, error);
    },
  });
}

/**
 * Mutation hook for editing messages with toast notifications
 *
 * @param workspaceId - The workspace containing the message
 * @returns Mutation object with mutate/mutateAsync functions
 *
 * @example
 * const { mutate: editMessage } = useEditMessage('workspace-123');
 * editMessage({ messageId: 'msg-456', content: 'Updated content' });
 */
export function useEditMessage(workspaceId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({ messageId, content }: { messageId: string; content: string }) => {
      return apiClient.editMessage(workspaceId, messageId, { content });
    },
    onSuccess: (updatedMessage, { messageId }) => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.messages(workspaceId) });

      if (updatedMessage.thread_id) {
        queryClient.invalidateQueries({
          queryKey: QUERY_KEYS.messageThread(workspaceId, updatedMessage.thread_id),
        });
      }

      toast.success('Message updated successfully');
      logger.info('Message edited', {
        component: 'useEditMessage',
        operation: 'editMessage',
        messageId,
        workspaceId,
      });
    },
    onError: (error: Error, { messageId }) => {
      toast.error(`Failed to edit message: ${error.message}`);
      logger.error('Failed to edit message', {
        component: 'useEditMessage',
        operation: 'editMessage',
        messageId,
        workspaceId,
      }, error);
    },
  });
}

/**
 * Mutation hook for marking messages as read with toast notifications
 *
 * @param workspaceId - The workspace containing the message
 * @returns Mutation object with mutate/mutateAsync functions
 *
 * @example
 * const { mutate: markRead } = useMarkMessageRead('workspace-123');
 * markRead('message-456');
 */
export function useMarkMessageRead(workspaceId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (messageId: string) => {
      return apiClient.markMessageRead(workspaceId, messageId);
    },
    onSuccess: (updatedMessage, messageId) => {
      // Update cache optimistically
      queryClient.setQueryData<Message[]>(
        QUERY_KEYS.messages(workspaceId),
        (oldMessages) => {
          if (!oldMessages) return oldMessages;
          return oldMessages.map((msg) =>
            msg.id === messageId ? { ...msg, read: true, read_at: new Date().toISOString() } : msg
          );
        }
      );

      logger.info('Message marked as read', {
        component: 'useMarkMessageRead',
        operation: 'markRead',
        messageId,
        workspaceId,
      });
    },
    onError: (error: Error, messageId) => {
      toast.error(`Failed to mark message as read: ${error.message}`);
      logger.error('Failed to mark message as read', {
        component: 'useMarkMessageRead',
        operation: 'markRead',
        messageId,
        workspaceId,
      }, error);
    },
  });
}

/**
 * Mutation hook for deleting messages with toast notifications
 *
 * @param workspaceId - The workspace containing the message
 * @returns Mutation object with mutate/mutateAsync functions
 *
 * @example
 * const { mutate: deleteMessage } = useDeleteMessage('workspace-123');
 * deleteMessage('message-456');
 */
export function useDeleteMessage(workspaceId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (messageId: string) => {
      return apiClient.deleteMessage(workspaceId, messageId);
    },
    onSuccess: (_, messageId) => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.messages(workspaceId) });

      toast.success('Message deleted successfully');
      logger.info('Message deleted', {
        component: 'useDeleteMessage',
        operation: 'deleteMessage',
        messageId,
        workspaceId,
      });
    },
    onError: (error: Error, messageId) => {
      toast.error(`Failed to delete message: ${error.message}`);
      logger.error('Failed to delete message', {
        component: 'useDeleteMessage',
        operation: 'deleteMessage',
        messageId,
        workspaceId,
      }, error);
    },
  });
}

// Export namespace for cleaner imports
export const MessagesHooks = {
  useMessages,
  useMessagesQuery,
  useMessageThread,
  useSendMessage,
  useEditMessage,
  useMarkMessageRead,
  useDeleteMessage,
};
