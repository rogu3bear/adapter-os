/**
 * Component prop types for chat-related UI components
 *
 * These types define the interfaces for components that handle chat interactions,
 * evidence display, message rendering, and session management.
 */

import type { ExtendedRouterDecision } from '@/api/types';
import type { ReplayResponse } from '@/api/replay-types';
import type { ChatSessionWithStatus } from '@/api/chat-types';
import type { AdapterLoadingItem } from '@/hooks/model-loading/types';

export interface RunMetadata {
  runId?: string;
  requestId?: string;
  traceId?: string;
  manifestHashB3?: string;
  policyMaskDigestB3?: string;
  planId?: string;
  workerId?: string;
  reasoningMode?: string;
  seedMaterial?: string | number;
  seededViaHkdf?: boolean;
}

/**
 * Evidence item for RAG-based responses
 */
export interface EvidenceItem {
  /** Document ID in the system */
  document_id: string;
  /** Human-readable document name */
  document_name: string;
  /** Chunk ID within the document */
  chunk_id: string;
  /** Page number (for PDF documents) */
  page_number: number | null;
  /** Preview text of the evidence */
  text_preview: string;
  /** Relevance score (0-1) */
  relevance_score: number;
  /** Rank in the evidence list */
  rank: number;
  /** Character range within the document for highlighting */
  char_range?: { start: number; end: number };
  /** Bounding box coordinates for PDF highlighting */
  bbox?: { x: number; y: number; width: number; height: number };
  /** Citation identifier for cross-referencing */
  citation_id?: string;
}

/**
 * Token throughput statistics for a message
 */
export interface ThroughputStats {
  /** Total tokens generated */
  tokensGenerated: number;
  /** Total latency in milliseconds */
  latencyMs: number;
  /** Tokens per second throughput */
  tokensPerSecond: number;
}

export interface TokenStreamEntry {
  token: string;
  index?: number;
  logprob?: number | null;
  routerScore?: number | null;
  timestamp?: number;
}

/**
 * Chat message structure
 */
export interface ChatMessage {
  /** Unique message ID */
  id: string;
  /** Message role (user or assistant) */
  role: 'user' | 'assistant';
  /** Message content */
  content: string;
  /** Message timestamp */
  timestamp: Date;
  /** Request ID for correlation */
  requestId?: string;
  /** Trace ID for telemetry lookup */
  traceId?: string;
  /** Cryptographic proof digest */
  proofDigest?: string;
  /** Router decision information */
  routerDecision?: ExtendedRouterDecision | null;
  /** Whether the message is currently streaming */
  isStreaming?: boolean;
  /** Evidence items for RAG responses */
  evidence?: EvidenceItem[];
  /** Whether the message has been verified */
  isVerified?: boolean | null;
  /** Timestamp of verification */
  verifiedAt?: string;
  /** List of unavailable pinned adapters */
  unavailablePinnedAdapters?: string[];
  /** Fallback strategy used when pinned adapters unavailable */
  pinnedRoutingFallback?: 'stack_only' | 'partial';
  /** Token throughput statistics */
  throughputStats?: ThroughputStats;
  /** Per-token streaming metadata for kernel/debug view */
  tokenStream?: TokenStreamEntry[];
  /** Run-level metadata captured during streaming */
  runMetadata?: RunMetadata;
  /** Optional error message if streaming failed */
  streamError?: string | null;
}

/**
 * Props for ChatInterface component
 * Main chat interaction interface
 */
export interface ChatInterfaceProps {
  /** Selected tenant ID */
  selectedTenant: string;
  /** Initial adapter stack ID to use (optional) */
  initialStackId?: string;
  /** Controlled stack ID - optional for base-model-only chat */
  selectedStackId?: string | null;
  /** Callback when stack selection changes */
  onStackChange?: (stackId: string | null) => void;
  /** Callback when the active session changes (for URL sync, etc.) */
  onSessionChange?: (sessionId: string | null) => void;
  /** Optional: load existing session */
  sessionId?: string;
  /** Document context for document-specific chat */
  documentContext?: {
    documentId: string;
    documentName: string;
    collectionId?: string;
  };
  /** Dataset context for dataset-scoped chat */
  datasetContext?: {
    datasetId: string;
    datasetName: string;
    collectionId?: string;
    datasetVersionId?: string;
  };
  /** Callback when user wants to view a document (for evidence navigation) */
  onViewDocument?: (documentId: string, pageNumber?: number, highlightText?: string) => void;
  /** Streaming render mode (tokens or chunks) */
  streamMode?: 'tokens' | 'chunks';
  /** Developer toggle to show raw traces */
  developerMode?: boolean;
  /** Callback when a message completes (for workbench right rail auto-update) */
  onMessageComplete?: (messageId: string, traceId?: string) => void;
  /** Callback when user selects/clicks a message. Receives traceId for trace fetching. */
  onMessageSelect?: (messageId: string, traceId?: string) => void;
  /** Currently selected message ID (for highlighting) */
  selectedMessageId?: string | null;
  /** Kernel mode flag for developer overlays */
  kernelMode?: boolean;
}

/**
 * Props for ChatMessage component
 * Individual message display with evidence and actions
 */
export interface ChatMessageProps {
  /** Message to display */
  message: ChatMessage;
  /** Optional CSS class name */
  className?: string;
  /** Callback to view a document */
  onViewDocument?: (documentId: string, pageNumber?: number) => void;
  /** Callback when message is selected (for workbench) */
  onSelect?: (messageId: string, traceId?: string) => void;
  /** Whether this message is currently selected */
  isSelected?: boolean;
  /** Render developer affordances (token visualization, scores) */
  developerMode?: boolean;
  /** Render kernel streaming overlays */
  kernelMode?: boolean;
  /**
   * Compact mode reduces widget count by consolidating evidence-related
   * widgets into a single EvidenceIndicator badge. Useful for list views
   * or when screen space is limited.
   */
  compactMode?: boolean;
}

/**
 * Props for ChatSearchBar component
 * Search interface for chat history
 */
export interface ChatSearchBarProps {
  /** Current search query */
  value: string;
  /** Callback when search query changes */
  onChange: (value: string) => void;
  /** Placeholder text */
  placeholder?: string;
  /** Whether search is loading */
  isLoading?: boolean;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for ChatArchivePanel component
 * Panel for managing archived and deleted chat sessions
 */
export interface ChatArchivePanelProps {
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for SessionCard component (internal to ChatArchivePanel)
 */
export interface SessionCardProps {
  /** Chat session to display */
  session: ChatSessionWithStatus;
  /** Callback to restore the session */
  onRestore: (sessionId: string) => void;
  /** Callback to permanently delete the session */
  onDelete?: (sessionId: string) => void;
  /** Whether restore operation is in progress */
  isRestoring?: boolean;
  /** Whether delete operation is in progress */
  isDeleting?: boolean;
}

/**
 * Props for EvidenceDrawer component
 * Sliding drawer for evidence and calculation display
 */
export interface EvidenceDrawerProps {
  /** Callback to view a document */
  onViewDocument?: (
    documentId: string,
    pageNumber?: number,
    highlightText?: string
  ) => void;
}

/**
 * Evidence drawer tab types
 */
export type EvidenceDrawerTab = 'rulebook' | 'calculation' | 'trace';

/**
 * Props for RulebookTab component
 * Displays evidence citations in the drawer
 */
export interface RulebookTabProps {
  /** Evidence items to display */
  evidence?: EvidenceItem[];
  /** Callback to view a document */
  onViewDocument?: (
    documentId: string,
    pageNumber?: number,
    highlightText?: string
  ) => void;
}

/**
 * Props for CalculationTab component
 * Displays router decision and receipt information
 */
export interface CalculationTabProps {
  /** Router decision information */
  routerDecision?: ExtendedRouterDecision | null;
  /** Request ID for the calculation */
  requestId?: string;
  /** Proof digest for verification */
  proofDigest?: string;
  /** Whether the calculation is verified */
  isVerified?: boolean | null;
  /** Timestamp of verification */
  verifiedAt?: string;
  /** Token throughput statistics */
  throughputStats?: ThroughputStats;
}

/**
 * Props for TraceTab component
 * Displays telemetry trace information
 */
export interface TraceTabProps {
  /** Trace ID to display */
  traceId?: string;
  /** Request ID for correlation */
  requestId?: string;
}

/**
 * Props for EvidenceDrawerTrigger component
 * Button to open evidence drawer
 */
export interface EvidenceDrawerTriggerProps {
  /** Message ID to associate with drawer */
  messageId: string;
  /** Evidence items to display */
  evidence?: EvidenceItem[];
  /** Router decision information */
  routerDecision?: ExtendedRouterDecision | null;
  /** Request ID */
  requestId?: string;
  /** Trace ID */
  traceId?: string;
  /** Proof digest */
  proofDigest?: string;
  /** Verification status */
  isVerified?: boolean | null;
  /** Verification timestamp */
  verifiedAt?: string;
  /** Throughput statistics */
  throughputStats?: ThroughputStats;
  /** Initial tab to show */
  initialTab?: EvidenceDrawerTab;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for ReplayButton component
 * Button to replay a message with deterministic inference
 */
export interface ReplayButtonProps {
  /** Request ID to replay */
  requestId: string;
  /** Original message content */
  originalContent: string;
  /** Callback when replay completes */
  onReplayComplete?: (result: ReplayResponse) => void;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for ChatErrorDisplay component
 * Displays chat errors with recovery options
 */
export interface ChatErrorDisplayProps {
  /** Error message */
  error: string | Error;
  /** Callback to retry the action */
  onRetry?: () => void;
  /** Callback to dismiss the error */
  onDismiss?: () => void;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for ChatLoadingOverlay component
 * Loading overlay for chat interface
 */
export interface ChatLoadingOverlayProps {
  /** Loading state with adapter details */
  loadingState: {
    adapters: AdapterLoadingItem[];
    overallProgress: number;
    estimatedTimeRemaining?: number;
  };

  /** Called when user clicks "Load and Chat" button */
  onLoadAll: () => void;

  /** Called when user cancels loading */
  onCancel: () => void;

  /** Optional kernel snapshot to surface worker/backend readiness */
  kernelInfo?: {
    workerName?: string | null;
    workerStatus?: string | null;
    backend?: string | null;
    backendMode?: string | null;
    baseModelName?: string | null;
    vramUsedMb?: number | null;
    vramTotalMb?: number | null;
    bootProgress?: number | null;
  };

  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for ChatTimeoutWarning component
 * Warning when chat request is taking too long
 */
export interface ChatTimeoutWarningProps {
  /** Time elapsed in seconds */
  elapsedSeconds: number;
  /** Callback to cancel the request */
  onCancel?: () => void;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for ChatSessionActions component
 * Actions menu for chat sessions
 */
export interface ChatSessionActionsProps {
  /** Session ID */
  sessionId: string;
  /** Session name */
  sessionName?: string;
  /** Callback when session is renamed */
  onRename?: (newName: string) => void;
  /** Callback when session is archived */
  onArchive?: () => void;
  /** Callback when session is deleted */
  onDelete?: () => void;
  /** Callback when session is shared */
  onShare?: () => void;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for ChatTagsManager component
 * Interface for managing chat session tags
 */
export interface ChatTagsManagerProps {
  /** Session ID */
  sessionId: string;
  /** Current tags */
  tags: string[];
  /** Callback when tags change */
  onTagsChange: (tags: string[]) => void;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for ChatCategoriesManager component
 * Interface for managing chat session categories
 */
export interface ChatCategoriesManagerProps {
  /** Session ID */
  sessionId: string;
  /** Current category */
  category?: string;
  /** Callback when category changes */
  onCategoryChange: (category: string | undefined) => void;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for PreChatAdapterPrompt component
 * Prompt to load adapters before chatting
 */
export interface PreChatAdapterPromptProps {
  /** Required adapter IDs */
  requiredAdapterIds: string[];
  /** Callback to load adapters */
  onLoadAdapters: () => void;
  /** Whether adapters are loading */
  isLoading?: boolean;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for MissingPinnedAdaptersWarning component
 * Warning when pinned adapters are unavailable
 */
export interface MissingPinnedAdaptersWarningProps {
  /** List of unavailable adapter names */
  unavailableAdapters: string[];
  /** Fallback strategy used */
  fallbackStrategy?: 'stack_only' | 'partial';
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for SimplifiedChatWidget component
 * Minimal chat widget for embedded use
 */
export interface SimplifiedChatWidgetProps {
  /** Adapter stack ID to use */
  stackId?: string;
  /** Initial message to send */
  initialMessage?: string;
  /** Height of the widget */
  height?: number | string;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for RouterActivitySidebar component
 * Sidebar showing router activity and adapter selection
 */
export interface RouterActivitySidebarProps {
  /** Current router decision */
  routerDecision?: ExtendedRouterDecision | null;
  /** Whether the sidebar is open */
  isOpen: boolean;
  /** Callback to toggle sidebar */
  onToggle: () => void;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for ChatInitialLoadState component
 * Initial loading state for chat interface
 */
export interface ChatInitialLoadStateProps {
  /** Loading message */
  message?: string;
  /** Optional CSS class name */
  className?: string;
}
