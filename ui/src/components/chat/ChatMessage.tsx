import React, { memo } from 'react';
import { cn } from '@/components/ui/utils';
import { RouterIndicator } from './RouterIndicator';
import { EvidencePanel } from './EvidencePanel';
import { ProofBadge } from './ProofBadge';
import type { ExtendedRouterDecision } from '@/api/types';

export interface EvidenceItem {
  document_id: string;
  document_name: string;
  chunk_id: string;
  page_number: number | null;
  text_preview: string;
  relevance_score: number;
  rank: number;
}

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: Date;
  requestId?: string;
  routerDecision?: ExtendedRouterDecision | null;
  isStreaming?: boolean;
  evidence?: EvidenceItem[];
  isVerified?: boolean;
  verifiedAt?: string;
}

interface ChatMessageProps {
  message: ChatMessage;
  className?: string;
  onViewDocument?: (documentId: string, pageNumber?: number) => void;
}

// Custom comparison function for memo to prevent unnecessary re-renders
function areMessagesEqual(prevProps: ChatMessageProps, nextProps: ChatMessageProps): boolean {
  const prev = prevProps.message;
  const next = nextProps.message;

  // Compare all fields that affect rendering
  return (
    prev.id === next.id &&
    prev.role === next.role &&
    prev.content === next.content &&
    prev.isStreaming === next.isStreaming &&
    prev.timestamp.getTime() === next.timestamp.getTime() &&
    prev.requestId === next.requestId &&
    prev.isVerified === next.isVerified &&
    prev.verifiedAt === next.verifiedAt &&
    // Deep compare router decision
    (prev.routerDecision === next.routerDecision ||
     (prev.routerDecision && next.routerDecision &&
      prev.routerDecision.request_id === next.routerDecision.request_id &&
      JSON.stringify(prev.routerDecision.selected_adapters) === JSON.stringify(next.routerDecision.selected_adapters))) &&
    // Deep compare evidence
    (prev.evidence === next.evidence ||
     (prev.evidence && next.evidence &&
      prev.evidence.length === next.evidence.length &&
      prev.evidence.every((e, i) => e.chunk_id === next.evidence![i].chunk_id))) &&
    prevProps.className === nextProps.className
  );
}

export const ChatMessageComponent = memo(function ChatMessageComponent({ message, className, onViewDocument }: ChatMessageProps) {
  const isUser = message.role === 'user';

  return (
    <div
      className={cn(
        'flex flex-col gap-2 px-4 py-3',
        isUser ? 'items-end' : 'items-start',
        className
      )}
      role="article"
      aria-label={`${isUser ? 'User' : 'Assistant'} message`}
    >
      {/* Router indicator for assistant messages */}
      {!isUser && message.routerDecision && (
        <RouterIndicator decision={message.routerDecision} />
      )}

      {/* Message bubble */}
      <div
        className={cn(
          'max-w-[80%] rounded-lg px-4 py-2 text-sm',
          isUser
            ? 'bg-primary text-primary-foreground'
            : 'bg-muted text-muted-foreground',
          message.isStreaming && 'animate-pulse'
        )}
        role={isUser ? 'user-message' : 'assistant-message'}
        aria-live={message.isStreaming ? 'polite' : 'off'}
      >
        <div className="whitespace-pre-wrap break-words">
          {message.content}
          {message.isStreaming && (
            <span
              className="inline-block w-2 h-4 ml-1 bg-current animate-pulse"
              aria-label="Streaming in progress"
              aria-live="polite"
            />
          )}
        </div>
      </div>

      {/* Evidence panel for assistant messages with sources */}
      {!isUser && message.evidence && message.evidence.length > 0 && (
        <div className="max-w-[80%] w-full">
          <EvidencePanel
            evidence={message.evidence}
            isVerified={message.isVerified || false}
            verifiedAt={message.verifiedAt}
            onViewDocument={onViewDocument}
          />
        </div>
      )}

      {/* Timestamp with verification badge */}
      <div className="flex items-center gap-2">
        <time
          className="text-xs text-muted-foreground px-1"
          dateTime={message.timestamp.toISOString()}
          aria-label={`Sent at ${message.timestamp.toLocaleTimeString()}`}
        >
          {message.timestamp.toLocaleTimeString()}
        </time>
        {!isUser && message.isVerified && (
          <ProofBadge isVerified={message.isVerified} timestamp={message.verifiedAt} />
        )}
      </div>
    </div>
  );
}, areMessagesEqual);

