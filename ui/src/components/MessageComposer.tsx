//! Message composer component
//!
//! Allows users to compose and send messages in workspaces.
//! Supports threading and basic formatting.
//!
//! Citation: Form patterns from ui/src/components/Nodes.tsx (dialog forms)
//! - Textarea for message content
//! - Send button with loading state

import React, { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Card, CardContent } from '@/components/ui/card';
import { Send, Loader2 } from 'lucide-react';
import { logger } from '@/utils/logger';

interface MessageComposerProps {
  workspaceId: string;
  onSendMessage: (content: string, threadId?: string) => Promise<void>;
  disabled?: boolean;
  threadId?: string;
  placeholder?: string;
}

export function MessageComposer({
  workspaceId,
  onSendMessage,
  disabled = false,
  threadId,
  placeholder = "Type your message..."
}: MessageComposerProps) {
  const [content, setContent] = useState('');
  const [sending, setSending] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!content.trim() || sending || disabled) return;

    setSending(true);

    try {
      await onSendMessage(content.trim(), threadId);
      setContent('');
      logger.info('Message sent from composer', {
        component: 'MessageComposer',
        operation: 'send_message',
        workspaceId,
        threadId,
        contentLength: content.length,
      });
    } catch (err) {
      logger.error('Failed to send message from composer', {
        component: 'MessageComposer',
        operation: 'send_message',
        workspaceId,
        threadId,
      }, err instanceof Error ? err : new Error(String(err)));
    } finally {
      setSending(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      handleSubmit(e);
    }
  };

  return (
    <Card>
      <CardContent className="p-4">
        <form onSubmit={handleSubmit} className="space-y-3">
          <Textarea
            value={content}
            onChange={(e) => setContent(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={placeholder}
            disabled={disabled || sending}
            rows={3}
            className="resize-none"
            aria-label="Message content"
          />

          <div className="flex items-center justify-between">
            <div className="text-xs text-muted-foreground">
              Press Ctrl+Enter to send
            </div>

            <Button
              type="submit"
              disabled={!content.trim() || sending || disabled}
              size="sm"
              className="flex items-center gap-2"
            >
              {sending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Send className="h-4 w-4" />
              )}
              {sending ? 'Sending...' : 'Send'}
            </Button>
          </div>
        </form>
      </CardContent>
    </Card>
  );
}
