/**
 * Chat Hook Mock Factories
 *
 * Factory functions for creating mock chat-related hook return values.
 */

import { vi, type Mock } from 'vitest';
import type { ChatSession as LocalChatSession } from '@/types/chat';
import type { ChatMessage as LocalChatMessage } from '@/components/chat/ChatMessage';
import { createMockChatSession } from '@/test/mocks/data/chat';

// ============================================================================
// useChatSessionsApi Mock
// ============================================================================

/**
 * Return type for useChatSessionsApi hook mock
 */
export interface UseChatSessionsApiMockReturn {
  sessions: LocalChatSession[];
  isLoading: boolean;
  isUnsupported: boolean;
  unsupportedReason: string | null;
  createSession: Mock;
  updateSession: Mock;
  addMessage: Mock;
  updateMessage: Mock;
  deleteSession: Mock;
  getSession: Mock;
  updateSessionCollection: Mock;
}

/**
 * Options for createUseChatSessionsApiMock factory
 */
export interface UseChatSessionsApiMockOptions {
  /** Chat sessions (default: empty array) */
  sessions?: Partial<LocalChatSession>[];
  /** Loading state (default: false) */
  isLoading?: boolean;
  /** Backend unsupported state (default: false) */
  isUnsupported?: boolean;
  /** Unsupported reason message (default: null) */
  unsupportedReason?: string | null;
}

/**
 * Create a mock return value for useChatSessionsApi hook
 *
 * @example
 * ```typescript
 * // Empty sessions
 * const empty = createUseChatSessionsApiMock();
 *
 * // With sessions
 * const withSessions = createUseChatSessionsApiMock({
 *   sessions: [
 *     { id: 'session-1', name: 'Chat 1' },
 *     { id: 'session-2', name: 'Chat 2' },
 *   ],
 * });
 *
 * // Loading state
 * const loading = createUseChatSessionsApiMock({ isLoading: true });
 * ```
 */
export function createUseChatSessionsApiMock(
  options: UseChatSessionsApiMockOptions = {}
): UseChatSessionsApiMockReturn {
  const sessions = (options.sessions ?? []).map((s) => createMockChatSession(s));

  const createSession = vi.fn().mockImplementation(
    (name: string, stackId: string) =>
      Promise.resolve(
        createMockChatSession({
          id: `session-${Date.now()}`,
          name,
          stackId,
        })
      )
  );

  const getSession = vi.fn().mockImplementation((sessionId: string) =>
    sessions.find((s) => s.id === sessionId)
  );

  return {
    sessions,
    isLoading: options.isLoading ?? false,
    isUnsupported: options.isUnsupported ?? false,
    unsupportedReason: options.unsupportedReason ?? null,
    createSession,
    updateSession: vi.fn().mockResolvedValue(undefined),
    addMessage: vi.fn(),
    updateMessage: vi.fn(),
    deleteSession: vi.fn(),
    getSession,
    updateSessionCollection: vi.fn(),
  };
}

// ============================================================================
// useChatStreaming Mock
// ============================================================================

/**
 * Return type for useChatStreaming hook mock
 */
export interface UseChatStreamingMockReturn {
  isStreaming: boolean;
  streamedText: string;
  currentRequestId: string | null;
  sendMessage: Mock;
  cancelStream: Mock;
  chunks: string[];
  tokensReceived: number;
  streamDuration: number;
}

/**
 * Options for createUseChatStreamingMock factory
 */
export interface UseChatStreamingMockOptions {
  /** Currently streaming (default: false) */
  isStreaming?: boolean;
  /** Streamed text content (default: '') */
  streamedText?: string;
  /** Current request ID (default: null) */
  currentRequestId?: string | null;
  /** Accumulated chunks (default: []) */
  chunks?: string[];
  /** Tokens received count (default: 0) */
  tokensReceived?: number;
  /** Stream duration in ms (default: 0) */
  streamDuration?: number;
}

/**
 * Create a mock return value for useChatStreaming hook
 *
 * @example
 * ```typescript
 * // Idle state
 * const idle = createUseChatStreamingMock();
 *
 * // Streaming state
 * const streaming = createUseChatStreamingMock({
 *   isStreaming: true,
 *   streamedText: 'Hello, I am...',
 *   currentRequestId: 'req-123',
 * });
 * ```
 */
export function createUseChatStreamingMock(
  options: UseChatStreamingMockOptions = {}
): UseChatStreamingMockReturn {
  return {
    isStreaming: options.isStreaming ?? false,
    streamedText: options.streamedText ?? '',
    currentRequestId: options.currentRequestId ?? null,
    sendMessage: vi.fn().mockResolvedValue(undefined),
    cancelStream: vi.fn(),
    chunks: options.chunks ?? [],
    tokensReceived: options.tokensReceived ?? 0,
    streamDuration: options.streamDuration ?? 0,
  };
}

// ============================================================================
// useChatAdapterState Mock
// ============================================================================

/**
 * Return type for useChatAdapterState hook mock
 */
export interface UseChatAdapterStateMockReturn {
  adapterStates: Map<string, unknown>;
  isCheckingAdapters: boolean;
  allAdaptersReady: boolean;
  loadAllAdapters: Mock;
  showAdapterPrompt: boolean;
  dismissAdapterPrompt: Mock;
  continueWithUnready: Mock;
}

/**
 * Options for createUseChatAdapterStateMock factory
 */
export interface UseChatAdapterStateMockOptions {
  /** All adapters ready (default: true) */
  allAdaptersReady?: boolean;
  /** Checking adapters (default: false) */
  isCheckingAdapters?: boolean;
  /** Show adapter prompt (default: false) */
  showAdapterPrompt?: boolean;
}

/**
 * Create a mock return value for useChatAdapterState hook
 */
export function createUseChatAdapterStateMock(
  options: UseChatAdapterStateMockOptions = {}
): UseChatAdapterStateMockReturn {
  return {
    adapterStates: new Map(),
    isCheckingAdapters: options.isCheckingAdapters ?? false,
    allAdaptersReady: options.allAdaptersReady ?? true,
    loadAllAdapters: vi.fn(),
    showAdapterPrompt: options.showAdapterPrompt ?? false,
    dismissAdapterPrompt: vi.fn(),
    continueWithUnready: vi.fn(),
  };
}

// ============================================================================
// useChatRouterDecisions Mock
// ============================================================================

/**
 * Return type for useChatRouterDecisions hook mock
 */
export interface UseChatRouterDecisionsMockReturn {
  isLoadingDecision: boolean;
  fetchDecision: Mock;
  decisionHistory: unknown[];
  lastDecision: unknown | null;
  clearDecisions: Mock;
}

/**
 * Options for createUseChatRouterDecisionsMock factory
 */
export interface UseChatRouterDecisionsMockOptions {
  /** Loading decision state (default: false) */
  isLoadingDecision?: boolean;
  /** Last decision object (default: null) */
  lastDecision?: unknown | null;
}

/**
 * Create a mock return value for useChatRouterDecisions hook
 */
export function createUseChatRouterDecisionsMock(
  options: UseChatRouterDecisionsMockOptions = {}
): UseChatRouterDecisionsMockReturn {
  return {
    isLoadingDecision: options.isLoadingDecision ?? false,
    fetchDecision: vi.fn().mockResolvedValue(null),
    decisionHistory: [],
    lastDecision: options.lastDecision ?? null,
    clearDecisions: vi.fn(),
  };
}

// ============================================================================
// useSessionManager Mock
// ============================================================================

/**
 * Return type for useSessionManager hook mock
 */
export interface UseSessionManagerMockReturn {
  currentSessionId: string | null;
  messages: LocalChatMessage[];
  setMessages: Mock;
  setCurrentSessionId: Mock;
  clearSession: Mock;
  loadSession: Mock;
  createSession: Mock;
}

/**
 * Options for createUseSessionManagerMock factory
 */
export interface UseSessionManagerMockOptions {
  /** Current session ID (default: 'session-1') */
  currentSessionId?: string | null;
  /** Messages array (default: []) */
  messages?: LocalChatMessage[];
}

/**
 * Create a mock return value for useSessionManager hook
 */
export function createUseSessionManagerMock(
  options: UseSessionManagerMockOptions = {}
): UseSessionManagerMockReturn {
  return {
    currentSessionId: options.currentSessionId ?? 'session-1',
    messages: options.messages ?? [],
    setMessages: vi.fn(),
    setCurrentSessionId: vi.fn(),
    clearSession: vi.fn(),
    loadSession: vi.fn(),
    createSession: vi.fn(),
  };
}

// ============================================================================
// useChatModals Mock
// ============================================================================

/**
 * Return type for useChatModals hook mock
 */
export interface UseChatModalsMockReturn {
  isHistoryOpen: boolean;
  setIsHistoryOpen: Mock;
  isRouterActivityOpen: boolean;
  setIsRouterActivityOpen: Mock;
  isArchivePanelOpen: boolean;
  setIsArchivePanelOpen: Mock;
  shareDialogSessionId: string | null;
  setShareDialogSessionId: Mock;
  tagsDialogSessionId: string | null;
  setTagsDialogSessionId: Mock;
}

/**
 * Options for createUseChatModalsMock factory
 */
export interface UseChatModalsMockOptions {
  /** History panel open (default: false) */
  isHistoryOpen?: boolean;
  /** Router activity open (default: false) */
  isRouterActivityOpen?: boolean;
  /** Archive panel open (default: false) */
  isArchivePanelOpen?: boolean;
  /** Share dialog session ID (default: null) */
  shareDialogSessionId?: string | null;
  /** Tags dialog session ID (default: null) */
  tagsDialogSessionId?: string | null;
}

/**
 * Create a mock return value for useChatModals hook
 */
export function createUseChatModalsMock(
  options: UseChatModalsMockOptions = {}
): UseChatModalsMockReturn {
  return {
    isHistoryOpen: options.isHistoryOpen ?? false,
    setIsHistoryOpen: vi.fn(),
    isRouterActivityOpen: options.isRouterActivityOpen ?? false,
    setIsRouterActivityOpen: vi.fn(),
    isArchivePanelOpen: options.isArchivePanelOpen ?? false,
    setIsArchivePanelOpen: vi.fn(),
    shareDialogSessionId: options.shareDialogSessionId ?? null,
    setShareDialogSessionId: vi.fn(),
    tagsDialogSessionId: options.tagsDialogSessionId ?? null,
    setTagsDialogSessionId: vi.fn(),
  };
}
