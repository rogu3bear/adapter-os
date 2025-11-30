/**
 * Chat components exports
 *
 * This file provides centralized exports for all chat-related components.
 * Components are organized by category for better discoverability.
 */

// Core Chat Components
export type { ChatMessage } from './ChatMessage';
export { ChatMessageComponent } from './ChatMessage';
export { ChatErrorBoundary } from './ChatErrorBoundary';

// Adapter Loading Components
export { default as AdapterLoadingProgress } from './AdapterLoadingProgress';
export { default as AdapterLoadingStatus } from './AdapterLoadingStatus';
export { default as PreChatAdapterPrompt } from './PreChatAdapterPrompt';

// Evidence & Proof Components
export { EvidencePanel } from './EvidencePanel';
export { EvidenceItem } from './EvidenceItem';
export { ProofBadge } from './ProofBadge';

// Router Components
export { RouterActivitySidebar } from './RouterActivitySidebar';
export { RouterDetailsModal } from './RouterDetailsModal';
export { RouterIndicator } from './RouterIndicator';
export { default as RouterTechnicalView } from './RouterTechnicalView';
export { default as RouterSummaryView } from './RouterSummaryView';
export type { RouterDecisionSummary } from './RouterSummaryView';

// Chat Management Components
export { ChatShareDialog } from './ChatShareDialog';
export { ChatTagsManager } from './ChatTagsManager';
export { ChatCategoriesManager } from './ChatCategoriesManager';
export { ChatSearchBar } from './ChatSearchBar';
export { ChatArchivePanel } from './ChatArchivePanel';
export { ChatSessionActions } from './ChatSessionActions';
