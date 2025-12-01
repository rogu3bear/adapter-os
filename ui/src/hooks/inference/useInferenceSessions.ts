import { useState, useCallback, useEffect } from 'react';
import { InferenceSession, InferenceConfig, InferResponse } from '@/api/api-types';
import { logger } from '@/utils/logger';

/**
 * Options for configuring the inference sessions hook
 */
export interface UseInferenceSessionsOptions {
  /** Maximum number of sessions to keep in history (default: 10) */
  maxSessions?: number;
  /** localStorage key for persisting sessions (default: 'inference-sessions') */
  storageKey?: string;
}

/**
 * Return value from useInferenceSessions hook
 */
export interface UseInferenceSessionsReturn {
  /** List of recent inference sessions */
  recentSessions: InferenceSession[];
  /** Add a new session to the history */
  addSession: (session: InferenceSession) => void;
  /** Remove a session by ID */
  removeSession: (id: string) => void;
  /** Clear all sessions */
  clearSessions: () => void;
  /** Load a session by ID */
  loadSession: (id: string) => InferenceSession | undefined;
  /** Save current inference request/response as a session */
  saveCurrentSession: (config: InferenceConfig, response: InferResponse) => InferenceSession;
}

/**
 * Hook for managing inference session history with localStorage persistence.
 *
 * Features:
 * - Automatic loading from localStorage on mount
 * - Automatic persistence to localStorage on change
 * - Configurable max session limit (oldest removed when full)
 * - Session ID generation with timestamp
 *
 * @example
 * ```typescript
 * const {
 *   recentSessions,
 *   addSession,
 *   saveCurrentSession,
 *   clearSessions
 * } = useInferenceSessions({
 *   maxSessions: 20,
 *   storageKey: 'my-inference-sessions'
 * });
 *
 * // After inference completes
 * const session = saveCurrentSession(config, response);
 * addSession(session);
 *
 * // Display recent sessions
 * {recentSessions.map(session => (
 *   <div key={session.id}>{session.prompt}</div>
 * ))}
 * ```
 */
export function useInferenceSessions(
  options: UseInferenceSessionsOptions = {}
): UseInferenceSessionsReturn {
  const {
    maxSessions = 10,
    storageKey = 'inference-sessions'
  } = options;

  const [recentSessions, setRecentSessions] = useState<InferenceSession[]>([]);

  // Load sessions from localStorage on mount
  useEffect(() => {
    try {
      const stored = localStorage.getItem(storageKey);
      if (stored) {
        const sessions = JSON.parse(stored) as InferenceSession[];
        setRecentSessions(sessions);
        logger.debug('Loaded inference sessions from localStorage', {
          count: sessions.length,
          storageKey
        });
      }
    } catch (error) {
      logger.error('Failed to load inference sessions from localStorage', {
        error,
        storageKey
      });
      // Clear corrupted data
      localStorage.removeItem(storageKey);
    }
  }, [storageKey]);

  // Persist sessions to localStorage whenever they change
  useEffect(() => {
    try {
      if (recentSessions.length > 0) {
        localStorage.setItem(storageKey, JSON.stringify(recentSessions));
        logger.debug('Persisted inference sessions to localStorage', {
          count: recentSessions.length,
          storageKey
        });
      }
    } catch (error) {
      logger.error('Failed to persist inference sessions to localStorage', {
        error,
        storageKey
      });
    }
  }, [recentSessions, storageKey]);

  /**
   * Add a new session to the history.
   * Automatically limits to maxSessions by removing oldest entries.
   */
  const addSession = useCallback((session: InferenceSession) => {
    setRecentSessions(prev => {
      const updated = [session, ...prev].slice(0, maxSessions);
      logger.info('Added inference session', {
        sessionId: session.id,
        totalSessions: updated.length,
        maxSessions
      });
      return updated;
    });
  }, [maxSessions]);

  /**
   * Remove a session by ID
   */
  const removeSession = useCallback((id: string) => {
    setRecentSessions(prev => {
      const updated = prev.filter(s => s.id !== id);
      logger.info('Removed inference session', {
        sessionId: id,
        remainingSessions: updated.length
      });
      return updated;
    });
  }, []);

  /**
   * Clear all sessions
   */
  const clearSessions = useCallback(() => {
    setRecentSessions([]);
    localStorage.removeItem(storageKey);
    logger.info('Cleared all inference sessions', { storageKey });
  }, [storageKey]);

  /**
   * Load a session by ID
   */
  const loadSession = useCallback((id: string) => {
    const session = recentSessions.find(s => s.id === id);
    if (session) {
      logger.debug('Loaded inference session', { sessionId: id });
    } else {
      logger.warn('Inference session not found', { sessionId: id });
    }
    return session;
  }, [recentSessions]);

  /**
   * Save current inference request/response as a session.
   * Generates a session ID with timestamp.
   */
  const saveCurrentSession = useCallback((
    config: InferenceConfig,
    response: InferResponse
  ): InferenceSession => {
    const session: InferenceSession = {
      id: `session-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`,
      created_at: new Date().toISOString(),
      prompt: config.prompt,
      request: config,
      response: response,
      status: 'completed',
      stack_id: config.stack_id,
      stack_name: undefined // Will be populated by caller if needed
    };

    logger.debug('Created inference session', {
      sessionId: session.id,
      promptLength: config.prompt.length,
      adaptersUsed: response.adapters_used?.length ?? 0
    });

    return session;
  }, []);

  return {
    recentSessions,
    addSession,
    removeSession,
    clearSessions,
    loadSession,
    saveCurrentSession
  };
}
