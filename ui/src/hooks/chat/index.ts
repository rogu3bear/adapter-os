/**
 * Chat-related React hooks
 *
 * This module exports hooks for managing chat functionality including
 * router decisions, message handling, chat session state, and adapter lifecycle.
 */

// Adapter state management
export {
  useChatAdapterState,
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

// Session management
export {
  useSessionManager,
} from './useSessionManager';

// Modal management
export {
  useChatModals,
  type ChatModalType,
} from './useChatModals';

// Archive management
export {
  useArchivedSessions,
  useDeletedSessions,
  useArchiveSession,
  useRestoreSession,
  useHardDeleteSession,
  useChatArchive,
} from './useChatArchive';

// Category management
export {
  useChatCategories,
  useCreateCategory,
  useSetSessionCategory,
} from './useChatCategories';

// Search
export {
  useChatSearch,
  chatSearchQueryKeys,
} from './useChatSearch';

// Session API
export {
  useChatSessionsApi,
} from './useChatSessionsApi';

// Sessions
export {
  useChatSessions,
} from './useChatSessions';

// Sharing
export {
  useSessionShares,
  useSessionsSharedWithMe,
  useShareSession,
  useRevokeShare,
  useChatSharing,
} from './useChatSharing';

// Tags
export {
  useChatTags,
  useCreateTag,
  useUpdateTag,
  useDeleteTag,
  useSessionTags,
  useAssignTagsToSession,
  useRemoveTagFromSession,
  useChatTagsNamespace,
} from './useChatTags';

// Messages and templates
export { useMessages } from './useMessages';
export { usePromptTemplates } from './usePromptTemplates';

// Session scope management
export { useSessionScope, default as useSessionScopeDefault } from './useSessionScope';
