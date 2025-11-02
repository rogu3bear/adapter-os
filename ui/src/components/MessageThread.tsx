//! Message thread display component
//!
//! Shows messages in chronological order with threading support.
//! Handles message editing and thread expansion.
//!
//! Citation: Message list pattern similar to ActivityFeedWidget event list
//! - Thread replies indented
//! - User avatars/identifiers
//! - Timestamp display per ui/src/components/dashboard/ActivityFeedWidget.tsx L184-L186

import React, { useState } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { useRelativeTime } from '@/hooks/useTimestamp';
import { Message, Workspace } from '@/api/types';
import {
  MessageSquare,
  Edit3,
  Check,
  X,
  ChevronDown,
  ChevronRight,
  AlertCircle,
  RefreshCw,
  User
} from 'lucide-react';
import { logger } from '@/utils/logger';

interface MessageThreadProps {
  messages: Message[];
  workspaceId: string;
  workspace: Workspace;
  loading: boolean;
  error: string | null;
  onEditMessage: (messageId: string, content: string) => Promise<void>;
  onGetThread: (threadId: string) => Promise<Message[]>;
  onRefresh: () => Promise<void>;
}

export function MessageThread({
  messages,
  workspaceId,
  workspace,
  loading,
  error,
  onEditMessage,
  onGetThread,
  onRefresh,
}: MessageThreadProps) {
  const [editingMessageId, setEditingMessageId] = useState<string | null>(null);
  const [editingContent, setEditingContent] = useState('');
  const [expandedThreads, setExpandedThreads] = useState<Set<string>>(new Set());
  const [threadMessages, setThreadMessages] = useState<Record<string, Message[]>>({});

  const handleEditStart = (message: Message) => {
    setEditingMessageId(message.id);
    setEditingContent(message.content);
  };

  const handleEditCancel = () => {
    setEditingMessageId(null);
    setEditingContent('');
  };

  const handleEditSave = async () => {
    if (!editingMessageId) return;

    try {
      await onEditMessage(editingMessageId, editingContent);
      setEditingMessageId(null);
      setEditingContent('');
      logger.info('Message edited successfully', {
        component: 'MessageThread',
        operation: 'edit_save',
        messageId: editingMessageId,
        workspaceId,
      });
    } catch (err) {
      logger.error('Failed to edit message', {
        component: 'MessageThread',
        operation: 'edit_save',
        messageId: editingMessageId,
        workspaceId,
      }, err instanceof Error ? err : new Error(String(err)));
    }
  };

  const toggleThread = async (threadId: string) => {
    const newExpanded = new Set(expandedThreads);

    if (newExpanded.has(threadId)) {
      newExpanded.delete(threadId);
    } else {
      newExpanded.add(threadId);

      // Load thread messages if not already loaded
      if (!threadMessages[threadId]) {
        try {
          const threadMsgs = await onGetThread(threadId);
          setThreadMessages(prev => ({ ...prev, [threadId]: threadMsgs }));
          logger.info('Thread messages loaded', {
            component: 'MessageThread',
            operation: 'load_thread',
            threadId,
            messageCount: threadMsgs.length,
            workspaceId,
          });
        } catch (err) {
          logger.error('Failed to load thread messages', {
            component: 'MessageThread',
            operation: 'load_thread',
            threadId,
            workspaceId,
          }, err instanceof Error ? err : new Error(String(err)));
        }
      }
    }

    setExpandedThreads(newExpanded);
  };

  const renderMessage = (message: Message, isThreadReply = false) => {
    const isEditing = editingMessageId === message.id;
    const relativeTime = useRelativeTime(message.created_at);
    const isThreadRoot = !!message.thread_id;

    return (
      <div
        key={message.id}
        className={`flex gap-3 p-4 ${isThreadReply ? 'ml-8 border-l-2 border-muted' : ''} ${
          isThreadRoot ? 'bg-muted/30' : ''
        }`}
      >
        {/* User Avatar Placeholder */}
        <div className="flex-shrink-0 w-8 h-8 rounded-full bg-primary/10 flex items-center justify-center">
          <User className="h-4 w-4 text-primary" />
        </div>

        {/* Message Content */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-1">
            <span className="text-sm font-medium">
              {message.from_user_display_name || message.from_user_id.slice(0, 8)}
            </span>
            <span className="text-xs text-muted-foreground">
              {relativeTime}
            </span>
            {message.edited_at && (
              <Badge variant="outline" className="text-xs">
                Edited
              </Badge>
            )}
            {isThreadReply && (
              <Badge variant="secondary" className="text-xs">
                Reply
              </Badge>
            )}
          </div>

          {isEditing ? (
            <div className="space-y-2">
              <textarea
                value={editingContent}
                onChange={(e) => setEditingContent(e.target.value)}
                className="w-full p-2 border rounded-md resize-none"
                rows={3}
                placeholder="Edit message..."
              />
              <div className="flex gap-2">
                <Button size="sm" onClick={handleEditSave}>
                  <Check className="h-3 w-3 mr-1" />
                  Save
                </Button>
                <Button size="sm" variant="outline" onClick={handleEditCancel}>
                  <X className="h-3 w-3 mr-1" />
                  Cancel
                </Button>
              </div>
            </div>
          ) : (
            <div className="space-y-2">
              <p className="text-sm whitespace-pre-wrap">{message.content}</p>

              {/* Message Actions */}
              <div className="flex items-center gap-2">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => handleEditStart(message)}
                  className="h-6 px-2 text-xs"
                >
                  <Edit3 className="h-3 w-3 mr-1" />
                  Edit
                </Button>

                {message.thread_id && (
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => toggleThread(message.thread_id!)}
                    className="h-6 px-2 text-xs"
                  >
                    {expandedThreads.has(message.thread_id) ? (
                      <ChevronDown className="h-3 w-3 mr-1" />
                    ) : (
                      <ChevronRight className="h-3 w-3 mr-1" />
                    )}
                    Thread ({threadMessages[message.thread_id]?.length || 0})
                  </Button>
                )}
              </div>

              {/* Thread Replies */}
              {message.thread_id && expandedThreads.has(message.thread_id) && (
                <div className="mt-2 space-y-1">
                  {threadMessages[message.thread_id]?.map(reply =>
                    renderMessage(reply, true)
                  )}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    );
  };

  if (loading && messages.length === 0) {
    return (
      <div className="space-y-4">
        {Array.from({ length: 3 }).map((_, i) => (
          <div key={i} className="flex gap-3 p-4">
            <div className="w-8 h-8 bg-muted animate-pulse rounded-full" />
            <div className="flex-1 space-y-2">
              <div className="h-4 bg-muted animate-pulse rounded w-32" />
              <div className="h-4 bg-muted animate-pulse rounded w-full" />
              <div className="h-3 bg-muted animate-pulse rounded w-24" />
            </div>
          </div>
        ))}
      </div>
    );
  }

  if (error) {
    return (
      <Alert>
        <AlertCircle className="h-4 w-4" />
        <AlertDescription>
          Failed to load messages: {error}
          <Button
            variant="outline"
            size="sm"
            onClick={onRefresh}
            className="ml-2"
          >
            <RefreshCw className="h-3 w-3 mr-1" />
            Retry
          </Button>
        </AlertDescription>
      </Alert>
    );
  }

  if (messages.length === 0) {
    return (
      <div className="text-center py-12">
        <MessageSquare className="h-12 w-12 text-muted-foreground mx-auto mb-4" />
        <h3 className="text-lg font-semibold mb-2">No Messages Yet</h3>
        <p className="text-muted-foreground">
          Be the first to send a message in this workspace!
        </p>
      </div>
    );
  }

  return (
    <ScrollArea className="h-96 w-full">
      <div className="space-y-1">
        {messages.map(message => renderMessage(message))}
      </div>
    </ScrollArea>
  );
}
