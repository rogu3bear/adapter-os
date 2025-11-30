/**
 * Example usage of ChatShareDialog component
 *
 * This file demonstrates how to integrate the ChatShareDialog
 * into your chat session UI.
 */

import React, { useState } from 'react';
import { Share2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { ChatShareDialog } from './ChatShareDialog';

// Example: Adding a share button to a chat session header
export function ChatSessionHeader({ sessionId }: { sessionId: string }) {
  const [shareDialogOpen, setShareDialogOpen] = useState(false);

  return (
    <div className="flex items-center justify-between p-4 border-b">
      <h2 className="text-lg font-semibold">Chat Session</h2>

      <div className="flex gap-2">
        {/* Share button */}
        <Button
          variant="outline"
          size="sm"
          onClick={() => setShareDialogOpen(true)}
        >
          <Share2 className="h-4 w-4 mr-2" />
          Share
        </Button>
      </div>

      {/* Share dialog */}
      <ChatShareDialog
        sessionId={sessionId}
        open={shareDialogOpen}
        onOpenChange={setShareDialogOpen}
      />
    </div>
  );
}

// Example: Using in a chat page
export function ChatSessionPage() {
  const sessionId = 'session-123'; // Get from route params
  const [shareDialogOpen, setShareDialogOpen] = useState(false);

  return (
    <div className="flex flex-col h-screen">
      <header className="border-b p-4">
        <div className="flex items-center justify-between">
          <h1 className="text-xl font-bold">My Chat Session</h1>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setShareDialogOpen(true)}
          >
            <Share2 className="h-4 w-4" />
          </Button>
        </div>
      </header>

      <main className="flex-1 overflow-y-auto">
        {/* Chat messages */}
      </main>

      <ChatShareDialog
        sessionId={sessionId}
        open={shareDialogOpen}
        onOpenChange={setShareDialogOpen}
      />
    </div>
  );
}

// Example: Programmatically opening the share dialog
export function useChatShare(sessionId: string) {
  const [shareDialogOpen, setShareDialogOpen] = useState(false);

  const openShareDialog = () => setShareDialogOpen(true);
  const closeShareDialog = () => setShareDialogOpen(false);

  const ShareDialog = () => (
    <ChatShareDialog
      sessionId={sessionId}
      open={shareDialogOpen}
      onOpenChange={setShareDialogOpen}
    />
  );

  return {
    openShareDialog,
    closeShareDialog,
    ShareDialog,
    isOpen: shareDialogOpen,
  };
}

// Usage of the hook:
// const { openShareDialog, ShareDialog } = useChatShare(sessionId);
// <Button onClick={openShareDialog}>Share</Button>
// <ShareDialog />
