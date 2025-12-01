/**
 * Chat-related React hooks
 *
 * This module exports hooks for managing chat functionality including
 * router decisions, message handling, chat session state, and adapter lifecycle.
 */

// Adapter state management
export {
  useChatAdapterState,
  type AdapterLifecycleState,
  type AdapterReadinessState,
  type UseChatAdapterStateOptions,
  type UseChatAdapterStateReturn,
} from './useChatAdapterState';

// Router decisions
export {
  useChatRouterDecisions,
  type RouterDecision,
  type UseChatRouterDecisionsOptions,
  type UseChatRouterDecisionsReturn,
} from './useChatRouterDecisions';

// Streaming
export {
  useChatStreaming,
  type UseChatStreamingOptions,
  type UseChatStreamingReturn,
} from './useChatStreaming';
