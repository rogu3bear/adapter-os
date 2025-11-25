import React, { memo } from 'react';
import { cn } from '@/components/ui/utils';
import { RouterIndicator } from './RouterIndicator';
import type { ExtendedRouterDecision } from '@/api/types';

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: Date;
  requestId?: string;
  routerDecision?: ExtendedRouterDecision | null;
  isStreaming?: boolean;
}

interface ChatMessageProps {
  message: ChatMessage;
  className?: string;
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
    // Deep compare router decision
    (prev.routerDecision === next.routerDecision ||
     (prev.routerDecision && next.routerDecision &&
      prev.routerDecision.request_id === next.routerDecision.request_id &&
      JSON.stringify(prev.routerDecision.selected_adapters) === JSON.stringify(next.routerDecision.selected_adapters))) &&
    prevProps.className === nextProps.className
  );
}

export const ChatMessageComponent = memo(function ChatMessageComponent({ message, className }: ChatMessageProps) {
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

      {/* Timestamp */}
      <time 
        className="text-xs text-muted-foreground px-1"
        dateTime={message.timestamp.toISOString()}
        aria-label={`Sent at ${message.timestamp.toLocaleTimeString()}`}
      >
        {message.timestamp.toLocaleTimeString()}
      </time>
    </div>
  );
}, areMessagesEqual);

