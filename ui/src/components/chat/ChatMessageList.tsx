/**
 * ChatMessageList - Virtualized chat message list component
 *
 * Handles virtualization of chat messages using @tanstack/react-virtual
 * for efficient rendering of large message histories. Includes auto-scroll
 * behavior, streaming message updates, and evidence panel integration.
 */

import React, { useCallback, useEffect, useImperativeHandle, forwardRef, RefObject } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Layers } from 'lucide-react';
import { ChatMessageComponent } from './ChatMessage';
import { RunEvidencePanel } from './RunEvidencePanel';
import type { ChatMessage } from '@/types/components';

/**
 * Token stream entry for kernel/debug view
 */
export interface TokenStreamEntry {
  token: string;
  index?: number;
  logprob?: number | null;
  routerScore?: number | null;
  timestamp?: number;
}

/**
 * Streaming chunk with metadata
 */
export interface StreamingChunk {
  token: string;
  content: string;
  timestamp: number;
  index: number;
  logprob?: number | null;
  routerScore?: number | null;
}

/**
 * Context for document viewing (e.g., from document-focused chat)
 */
export interface DocumentContext {
  documentId: string;
  documentName: string;
  collectionId?: string;
}

/**
 * Context for dataset-scoped chat
 */
export interface DatasetContext {
  datasetId: string;
  datasetName: string;
  collectionId?: string;
  datasetVersionId?: string;
}

/**
 * Workspace state for evidence panels
 */
export interface WorkspaceActiveState {
  activeBaseModelId?: string | null;
  activePlanId?: string | null;
  activeAdapterIds?: string[] | null;
  manifestHashB3?: string | null;
  policyMaskDigestB3?: string | null;
  updatedAt?: string | null;
}

/**
 * Props for ChatMessageList component
 */
export interface ChatMessageListProps {
  /** Array of chat messages to display */
  messages: ChatMessage[];
  /** ID of the message currently being streamed, if any */
  streamingMessageId: string | null;
  /** Currently selected message ID for highlighting */
  selectedMessageId: string | null;
  /** Current streaming content for the streaming message */
  streamingContent: string;
  /** Token chunks for streaming visualization */
  chunks: StreamingChunk[];
  /** Callback when a message is selected */
  onSelectMessage: (messageId: string, traceId?: string) => void;
  /** Callback to view a document (for evidence navigation) */
  onViewDocument?: (documentId: string, pageNumber?: number, highlightText?: string) => void;
  /** Callback to export run evidence for a message */
  onExportRunEvidence?: (message: ChatMessage) => void;
  /** Ref to the scroll area element (Radix ScrollArea) */
  scrollAreaRef: RefObject<HTMLDivElement | null>;
  /** Developer mode flag for additional visualizations */
  developerMode?: boolean;
  /** Kernel mode flag for streaming overlays */
  kernelMode?: boolean;
  /** Document context for document-specific chat */
  documentContext?: DocumentContext;
  /** Dataset context for dataset-scoped chat */
  datasetContext?: DatasetContext;
  /** Workspace active state for evidence panel fallbacks */
  workspaceActiveState?: WorkspaceActiveState | null;
  /** Tenant ID for workspace context */
  tenantId?: string;
  /** Whether a router decision is loading */
  isLoadingDecision?: boolean;
}

/**
 * Ref handle for ChatMessageList component
 */
export interface ChatMessageListRef {
  /** Scroll to the bottom of the message list */
  scrollToBottom: () => void;
}

/**
 * Empty state component shown when there are no messages
 */
function EmptyMessageState({
  documentContext,
  datasetContext,
}: {
  documentContext?: DocumentContext;
  datasetContext?: DatasetContext;
}) {
  return (
    <div
      className="flex items-center justify-center h-full text-muted-foreground"
      role="status"
      aria-live="polite"
    >
      <div className="text-center">
        <Layers className="h-12 w-12 mx-auto mb-4 opacity-50" aria-hidden="true" />
        <p className="text-lg font-medium">Start a conversation</p>
        <p className="text-sm mt-1">
          {documentContext
            ? `I'm ready to help you with "${documentContext.documentName}". Ask me anything about this document.`
            : datasetContext
              ? `I'm ready to help you with the "${datasetContext.datasetName}" dataset. Ask me anything about this data.`
              : 'Select a stack and send a message to begin'}
        </p>
      </div>
    </div>
  );
}

/**
 * ChatMessageList - Virtualized list of chat messages
 *
 * Uses @tanstack/react-virtual for efficient rendering of large message histories.
 * Automatically scrolls to the bottom when new messages arrive.
 */
export const ChatMessageList = forwardRef<ChatMessageListRef, ChatMessageListProps>(
  function ChatMessageList(
    {
      messages,
      streamingMessageId,
      selectedMessageId,
      streamingContent,
      chunks,
      onSelectMessage,
      onViewDocument,
      onExportRunEvidence,
      scrollAreaRef,
      developerMode = false,
      kernelMode = false,
      documentContext,
      datasetContext,
      workspaceActiveState,
      tenantId,
      isLoadingDecision = false,
    },
    ref
  ) {
    /**
     * Get the scroll element from the Radix ScrollArea.
     * The viewport element has the data-radix-scroll-area-viewport attribute.
     */
    const getScrollElement = useCallback(() => {
      if (scrollAreaRef.current) {
        const viewport = scrollAreaRef.current.querySelector('[data-radix-scroll-area-viewport]');
        if (viewport) {
          return viewport as HTMLDivElement;
        }
      }
      return null;
    }, [scrollAreaRef]);

    /**
     * Virtualizer instance for efficient message rendering.
     * Estimates 150px per message and renders 5 extra items above/below viewport.
     */
    const virtualizer = useVirtualizer({
      count: messages.length,
      getScrollElement,
      estimateSize: () => 150,
      overscan: 5,
    });

    /**
     * Scroll to bottom programmatically.
     * Can be called from parent component via ref.
     */
    const scrollToBottom = useCallback(() => {
      if (messages.length > 0 && virtualizer) {
        virtualizer.scrollToIndex(messages.length - 1, {
          align: 'end',
          behavior: 'smooth',
        });
      }
    }, [messages.length, virtualizer]);

    // Expose scrollToBottom to parent via ref
    useImperativeHandle(ref, () => ({
      scrollToBottom,
    }), [scrollToBottom]);

    /**
     * Auto-scroll to bottom when new messages arrive.
     * Uses a small delay to allow the virtualizer to update sizes.
     */
    useEffect(() => {
      if (messages.length > 0 && virtualizer) {
        const timeoutId = setTimeout(() => {
          virtualizer.scrollToIndex(messages.length - 1, {
            align: 'end',
            behavior: 'smooth',
          });
        }, 100);
        return () => clearTimeout(timeoutId);
      }
      return undefined;
    }, [messages.length, virtualizer]);

    // Empty state
    if (messages.length === 0) {
      return (
        <div className="py-4">
          <EmptyMessageState
            documentContext={documentContext}
            datasetContext={datasetContext}
          />
        </div>
      );
    }

    return (
      <div className="py-4">
        <div
          style={{
            height: `${virtualizer.getTotalSize()}px`,
            width: '100%',
            position: 'relative',
          }}
        >
          {virtualizer.getVirtualItems().map((virtualItem) => {
            const message = messages[virtualItem.index];

            // Build the message object, updating streaming message with current content
            const displayMessage: ChatMessage =
              message.id === streamingMessageId
                ? {
                    ...message,
                    content: streamingContent,
                    tokenStream: chunks.map((chunk) => ({
                      token: chunk.token,
                      logprob: chunk.logprob,
                      routerScore: chunk.routerScore,
                      index: chunk.index,
                      timestamp: chunk.timestamp,
                    })),
                  }
                : message;

            return (
              <div
                key={message.id}
                data-index={virtualItem.index}
                ref={virtualizer.measureElement}
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  width: '100%',
                  transform: `translateY(${virtualItem.start}px)`,
                }}
              >
                <div className="space-y-2">
                  <ChatMessageComponent
                    message={displayMessage}
                    onViewDocument={onViewDocument}
                    onSelect={onSelectMessage}
                    isSelected={selectedMessageId === message.id}
                    developerMode={developerMode}
                    kernelMode={kernelMode}
                  />
                  {message.role === 'assistant' && (
                    <RunEvidencePanel
                      evidence={message.runMetadata}
                      traceId={message.traceId}
                      fallbackPolicyMask={message.routerDecision?.policy_mask_digest}
                      fallbackPlanId={workspaceActiveState?.activePlanId ?? undefined}
                      manifestFallback={workspaceActiveState?.manifestHashB3 ?? undefined}
                      workspaceIdFallback={tenantId}
                      showSeedValue={developerMode}
                      onExport={onExportRunEvidence ? () => onExportRunEvidence(message) : undefined}
                      pending={message.isStreaming}
                    />
                  )}
                </div>
              </div>
            );
          })}
        </div>
        {isLoadingDecision && (
          <div className="text-xs text-muted-foreground px-4" role="status" aria-live="polite">
            Loading router decision details...
          </div>
        )}
      </div>
    );
  }
);

export default ChatMessageList;
