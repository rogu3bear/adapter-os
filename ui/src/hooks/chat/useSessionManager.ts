import { useReducer, useCallback, useRef, useEffect } from 'react';
import { ChatMessage } from '@/components/chat/ChatMessage';
import { useChatSessionsApi } from '@/hooks/chat/useChatSessionsApi';
import { toast } from 'sonner';
import { logger } from '@/utils/logger';

interface SessionState {
  currentId: string | null;
  messages: ChatMessage[];
  editing: { id: string; name: string } | null;
}

interface SessionConfig {
  stack_id?: string;
  routing_determinism_mode?: 'deterministic' | 'adaptive';
  adapter_strength_overrides?: Record<string, number>;
  [key: string]: unknown;
}

type SessionAction =
  | { type: 'SESSION_LOADED'; payload: { id: string; messages: ChatMessage[] } }
  | { type: 'SESSION_CREATED'; payload: { id: string; messages: ChatMessage[] } }
  | { type: 'SESSION_CLEARED' }
  | { type: 'SESSION_ID_SET'; payload: string | null }
  | { type: 'MESSAGES_SET'; payload: ChatMessage[] }
  | { type: 'MESSAGE_ADDED'; payload: ChatMessage }
  | { type: 'START_EDITING'; payload: { id: string; name: string } }
  | { type: 'FINISH_EDITING' };

function sessionReducer(state: SessionState, action: SessionAction): SessionState {
  switch (action.type) {
    case 'SESSION_LOADED':
    case 'SESSION_CREATED':
      return {
        currentId: action.payload.id,
        messages: action.payload.messages,
        editing: null,
      };
    case 'SESSION_CLEARED':
      return {
        currentId: null,
        messages: [],
        editing: null,
      };
    case 'SESSION_ID_SET':
      return {
        ...state,
        currentId: action.payload,
      };
    case 'MESSAGES_SET':
      return {
        ...state,
        messages: action.payload,
      };
    case 'MESSAGE_ADDED':
      return {
        ...state,
        messages: [...state.messages, action.payload],
      };
    case 'START_EDITING':
      return {
        ...state,
        editing: action.payload,
      };
    case 'FINISH_EDITING':
      return {
        ...state,
        editing: null,
      };
    default:
      return state;
  }
}

interface UseSessionManagerOptions {
  tenantId: string;
  sessionSourceType: 'general' | 'document';
  documentContext?: {
    documentId: string;
    documentName: string;
    collectionId?: string;
  };
}

export function useSessionManager(options: UseSessionManagerOptions) {
  const { tenantId, sessionSourceType, documentContext } = options;

  const [state, dispatch] = useReducer(sessionReducer, {
    currentId: null,
    messages: [],
    editing: null,
  });

  const {
    createSession: apiCreateSession,
    updateSession: apiUpdateSession,
    deleteSession: apiDeleteSession,
    getSession: apiGetSession,
  } = useChatSessionsApi(tenantId, {
    sourceType: sessionSourceType,
    documentId: documentContext?.documentId,
    documentName: documentContext?.documentName,
    collectionId: documentContext?.collectionId ?? null,
  });

  // AbortController for session creation
  const createAbortControllerRef = useRef<AbortController | null>(null);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      createAbortControllerRef.current?.abort();
    };
  }, []);

  const loadSession = useCallback(
    (sessionId: string) => {
      const session = apiGetSession(sessionId);
      if (session) {
        dispatch({
          type: 'SESSION_LOADED',
          payload: {
            id: sessionId,
            messages: session.messages,
          },
        });
        return session;
      }
      return null;
    },
    [apiGetSession]
  );

  const createSession = useCallback(
    async (
      name: string,
      stackId: string,
      stackName: string | undefined,
      collectionId: string | null | undefined,
      documentCtx: { documentId: string; documentName: string } | undefined,
      sourceType: 'general' | 'document',
      config: SessionConfig
    ) => {
      // Cancel any pending creation (single-flight deduplication)
      createAbortControllerRef.current?.abort();
      createAbortControllerRef.current = new AbortController();

      try {
        const newSession = await apiCreateSession(
          name,
          stackId,
          stackName,
          collectionId ?? undefined,
          documentCtx,
          sourceType,
          config
        );

        // Check if aborted
        if (createAbortControllerRef.current?.signal.aborted) {
          logger.info('Session creation aborted', {
            component: 'useSessionManager',
            sessionId: newSession.id,
          });
          return newSession;
        }

        dispatch({
          type: 'SESSION_CREATED',
          payload: {
            id: newSession.id,
            messages: newSession.messages,
          },
        });

        return newSession;
      } catch (err) {
        if (err instanceof Error && err.name === 'AbortError') {
          logger.info('Session creation aborted', {
            component: 'useSessionManager',
          });
          return null;
        }
        // Error already logged by useChatSessionsApi
        throw err;
      }
    },
    [apiCreateSession]
  );

  const updateSession = useCallback(
    (sessionId: string, updates: { name?: string }) => {
      apiUpdateSession(sessionId, updates);
    },
    [apiUpdateSession]
  );

  const deleteSession = useCallback(
    (sessionId: string) => {
      apiDeleteSession(sessionId);
      if (state.currentId === sessionId) {
        dispatch({ type: 'SESSION_CLEARED' });
      }
    },
    [apiDeleteSession, state.currentId]
  );

  const clearSession = useCallback(() => {
    dispatch({ type: 'SESSION_CLEARED' });
  }, []);

  const setMessages = useCallback((messagesOrUpdater: ChatMessage[] | ((prev: ChatMessage[]) => ChatMessage[])) => {
    if (typeof messagesOrUpdater === 'function') {
      // For functional updates, we need current state - use reducer with a function dispatch
      dispatch({ type: 'MESSAGES_SET', payload: messagesOrUpdater(state.messages) });
    } else {
      dispatch({ type: 'MESSAGES_SET', payload: messagesOrUpdater });
    }
  }, [state.messages]);

  const addMessage = useCallback((message: ChatMessage) => {
    dispatch({ type: 'MESSAGE_ADDED', payload: message });
  }, []);

  const startEditing = useCallback((id: string, name: string) => {
    dispatch({ type: 'START_EDITING', payload: { id, name } });
  }, []);

  const finishEditing = useCallback(() => {
    dispatch({ type: 'FINISH_EDITING' });
  }, []);

  const setCurrentSessionId = useCallback((id: string | null) => {
    dispatch({ type: 'SESSION_ID_SET', payload: id });
  }, []);

  return {
    // State
    currentSessionId: state.currentId,
    messages: state.messages,
    editing: state.editing,

    // Actions
    loadSession,
    createSession,
    updateSession,
    deleteSession,
    clearSession,
    setMessages,
    setCurrentSessionId,
    addMessage,
    startEditing,
    finishEditing,
  };
}
