/**
 * Chat Hook Types
 *
 * Type definitions for chat-related hooks including streaming, sessions,
 * messages, search, and router decisions.
 */

import type { ChatMessage, ThroughputStats } from '@/components/chat/ChatMessage';
import type { SearchSessionsQuery } from '@/api/types';

// ============================================================================
// useChatStreaming Types
// ============================================================================

export interface UseChatStreamingOptions {
  /** Current chat session ID (required for message persistence) */
  sessionId: string | null;

  /** Stack ID to use for inference (adapter IDs will be resolved from stack) */
  stackId?: string;

  /** Routing determinism mode (deterministic | adaptive) */
  routingDeterminismMode?: 'deterministic' | 'adaptive';

  /** Per-adapter strength overrides (multiplier) */
  adapterStrengthOverrides?: Record<string, number>;

  /** Collection ID for RAG-enhanced inference */
  collectionId?: string;

  /** Document ID for document-specific chat */
  documentId?: string;

  /** Callback invoked when a user message is successfully sent */
  onMessageSent?: (message: ChatMessage) => void;

  /** Callback invoked when streaming completes and assistant message is finalized */
  onStreamComplete?: (response: ChatMessage) => void;

  /** Callback invoked when an error occurs during streaming */
  onError?: (error: Error) => void;
}

export interface UseChatStreamingReturn {
  // State
  /** Whether a streaming request is currently in progress */
  isStreaming: boolean;

  /** The accumulated text from the current stream */
  streamedText: string;

  /** Unique ID for the current request (for correlation with router decisions) */
  currentRequestId: string | null;

  /** Ordered chunks received during the current stream */
  chunks: Array<{ content: string; timestamp: number; index: number }>;

  // Actions
  /** Send a message and begin streaming the response */
  sendMessage: (content: string, adapterIds: string[]) => Promise<void>;

  /** Cancel the current streaming request */
  cancelStream: () => void;

  /** Reset streaming state (clears accumulated text and request ID) */
  resetStream: () => void;

  // Metrics
  /** Number of tokens received in the current stream */
  tokensReceived: number;

  /** Duration of the current/last stream in milliseconds */
  streamDuration: number | null;
}

// ============================================================================
// useChatSessionsApi Types
// ============================================================================

export interface UseChatSessionsOptions {
  /** Enable/disable the query */
  enabled?: boolean;
  /** Polling interval in ms */
  pollingInterval?: number;
}

// ============================================================================
// useMessages Types
// ============================================================================

export interface UseMessagesOptions {
  /** Chat session ID */
  sessionId: string;
  /** Enable/disable the query */
  enabled?: boolean;
}

export interface UseMessagesReturn {
  /** Chat messages */
  messages: ChatMessage[];
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
  /** Refetch messages */
  refetch: () => Promise<void>;
  /** Add a message optimistically */
  addMessage: (message: ChatMessage) => void;
  /** Update a message */
  updateMessage: (messageId: string, updates: Partial<ChatMessage>) => void;
}

// ============================================================================
// useChatSearch Types
// ============================================================================

export interface UseChatSearchOptions extends Omit<SearchSessionsQuery, 'q'> {
  /** Initial search query */
  initialQuery?: string;
  /** Enable/disable search */
  enabled?: boolean;
  /** Debounce delay in ms */
  debounceMs?: number;
}

export interface UseChatSearchReturn {
  /** Current search query */
  query: string;
  /** Update search query */
  setQuery: (query: string) => void;
  /** Search results */
  results: unknown[];
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
  /** Clear search */
  clearSearch: () => void;
  /** Total result count */
  totalResults: number;
}

// ============================================================================
// useChatRouterDecisions Types
// ============================================================================

export interface UseChatRouterDecisionsOptions {
  /** Request ID to fetch decisions for */
  requestId: string | null;
  /** Enable/disable the query */
  enabled?: boolean;
  /** Polling interval in ms */
  pollingInterval?: number;
}

export interface UseChatRouterDecisionsReturn {
  /** Router decisions */
  decisions: unknown[];
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
  /** Refetch decisions */
  refetch: () => Promise<void>;
}

// ============================================================================
// useChatAdapterState Types
// ============================================================================

export interface UseChatAdapterStateOptions {
  /** Chat session ID */
  sessionId: string;
  /** Enable/disable */
  enabled?: boolean;
}

export interface UseChatAdapterStateReturn {
  /** Currently active adapters */
  activeAdapters: string[];
  /** Set active adapters */
  setActiveAdapters: (adapterIds: string[]) => void;
  /** Add an adapter */
  addAdapter: (adapterId: string) => void;
  /** Remove an adapter */
  removeAdapter: (adapterId: string) => void;
  /** Clear all adapters */
  clearAdapters: () => void;
}

// ============================================================================
// useChatInitialLoad Types
// ============================================================================

export interface UseChatInitialLoadOptions {
  /** Session ID to load */
  sessionId: string | null;
  /** Enable/disable */
  enabled?: boolean;
  /** Callback when load complete */
  onLoadComplete?: () => void;
}

// ============================================================================
// useSessionManager Types
// ============================================================================

export interface UseSessionManagerOptions {
  /** Initial session ID */
  initialSessionId?: string;
  /** Auto-create session if none exists */
  autoCreate?: boolean;
  /** Callback when session changes */
  onSessionChange?: (sessionId: string | null) => void;
}
